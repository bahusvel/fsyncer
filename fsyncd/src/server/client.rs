use bincode::{deserialize_from, serialize, serialized_size};
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use common::*;
use dssc::{chunkmap::ChunkMap, other::ZstdBlock, Compressor};
use net2::TcpStreamExt;
use server::{cork_server, uncork_server};
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::sync::{Arc, Condvar, Mutex};
use std::{mem::transmute, net::TcpStream, ops::Deref, path::PathBuf, thread};
use {lz4, zstd};

static NOP_MSG: FsyncerMsg = FsyncerMsg::NOP;

lazy_static! {
    static ref ENCODED_NOP: Vec<u8> = serialize(&NOP_MSG).unwrap();
    static ref NOP_SIZE: usize = serialized_size(&NOP_MSG).unwrap() as usize;
}

#[derive(PartialEq, Clone, Copy)]
pub enum ClientStatus {
    DEAD,
    ALIVE,
}

struct ClientNetwork {
    write: Box<Write + Send>,
    rt_comp: Option<Box<Compressor>>,
    // TODO remvoe this hashmap and use an array
    parked: HashMap<u64, Arc<ClientResponse<i32>>>,
    status: ClientStatus,
}

pub struct Client {
    pub mode: ClientMode,
    comp: CompMode,
    net: Arc<Mutex<ClientNetwork>>,
}

struct ClientResponse<T> {
    data: Mutex<Option<T>>,
    cvar: Condvar,
}

impl<T> ClientResponse<T> {
    pub fn new() -> Self {
        ClientResponse {
            data: Mutex::new(None),
            cvar: Condvar::new(),
        }
    }
    pub fn wait(&self) -> T {
        let mut lock = self.data.lock().unwrap();

        while lock.is_none() {
            lock = self.cvar.wait(lock).unwrap();
        }

        lock.take().unwrap()
    }

    pub fn notify(&self, data: T) {
        *self.data.lock().unwrap() = Some(data);
        self.cvar.notify_one()
    }
}

impl Client {
    pub fn from_stream(
        mut stream: TcpStream,
        storage_path: PathBuf,
        dontcheck: bool,
        buffer_size: usize,
    ) -> Result<Self, io::Error> {
        println!("Received connection from client {:?}", stream.peer_addr());
        stream.set_send_buffer_size(buffer_size * 1024 * 1024)?;

        let init = match Client::read_msg(&mut stream) {
            Ok(FsyncerMsg::InitMsg(msg)) => msg,
            Err(e) => panic!("Failed to get init message from client {}", e),
            otherwise => panic!(
                "Expected init message from client, received {:?}",
                otherwise
            ),
        };

        if init.mode != ClientMode::MODE_CONTROL && (!dontcheck) {
            println!("Calculating source hash...");
            let srchash = hash_metadata(storage_path.to_str().unwrap()).expect("Hash check failed");
            println!("Source hash is {:x}", srchash);
            if init.dsthash != srchash {
                println!(
                    "{:x} != {:x} client's hash does not match!",
                    init.dsthash, srchash
                );
                println!("Dropping this client!");
                drop(stream);
                return Err(io::Error::new(io::ErrorKind::Other, "Hash mismatch"));
            }
        }

        let writer = if init.compress.contains(CompMode::STREAM_ZSTD) {
            Box::new(zstd::stream::Encoder::new(stream.try_clone()?, 0)?) as Box<Write + Send>
        } else if init.compress.contains(CompMode::STREAM_LZ4) {
            Box::new(lz4::EncoderBuilder::new().build(stream.try_clone()?)?) as Box<Write + Send>
        } else {
            Box::new(stream.try_clone()?) as Box<Write + Send>
        };

        let rt_comp: Option<Box<Compressor>> = if init.compress.contains(CompMode::RT_DSSC_CHUNKED)
        {
            Some(Box::new(ChunkMap::new(0.5)))
        } else if init.compress.contains(CompMode::RT_DSSC_ZSTD) {
            Some(Box::new(ZstdBlock::default()))
        } else {
            None
        };

        let net = Arc::new(Mutex::new(ClientNetwork {
            write: writer,
            rt_comp: rt_comp,
            parked: HashMap::new(),
            status: ClientStatus::ALIVE,
        }));
        let net_clone = net.clone();

        thread::spawn(move || Client::reader(stream, net_clone));

        println!("Client connected!");

        Ok(Client {
            mode: init.mode,
            comp: init.compress,
            net: net,
        })
    }

    // Send a cork to this client, and block until it acknowledges
    pub fn cork(&self) -> Result<(), io::Error> {
        let current_thread = thread::current();
        let tid = unsafe { transmute::<thread::ThreadId, u64>(current_thread.id()) };
        self.send_msg(FsyncerMsg::Cork(tid), true)?;
        // Cannot park on control as it will block its reader thread
        if self.mode != ClientMode::MODE_CONTROL {
            self.wait_thread_response();
        }
        Ok(())
    }

    pub fn status(&self) -> ClientStatus {
        self.net.lock().unwrap().status
    }

    pub fn uncork(&self) -> Result<(), io::Error> {
        self.send_msg(FsyncerMsg::Uncork, false)
    }

    fn read_msg<R: Read>(read: &mut R) -> Result<FsyncerMsg, io::Error> {
        let _size = read.read_u32::<BigEndian>()?;
        // TODO use size to restrict reading
        Ok(deserialize_from(read).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?)
    }

    fn reader<R: Read>(mut read: R, net: Arc<Mutex<ClientNetwork>>) {
        let net = net.deref();
        loop {
            match Client::read_msg(&mut read) {
                Ok(FsyncerMsg::AckCork(tid)) => {
                    let mut netlock = net.lock().unwrap();
                    netlock.parked.get(&tid).map(|t| {
                        assert!(Arc::strong_count(t) <= 2);
                        t.notify(0)
                    });
                }
                Ok(FsyncerMsg::Ack(AckMsg { retcode: code, tid })) => {
                    let mut netlock = net.lock().unwrap();
                    netlock.parked.get(&tid).map(|t| {
                        assert!(Arc::strong_count(t) <= 2);
                        t.notify(code)
                    });
                }
                Ok(FsyncerMsg::Cork(_)) => cork_server(),
                Ok(FsyncerMsg::Uncork) => uncork_server(),
                Err(e) => {
                    let mut netlock = net.lock().unwrap();
                    netlock.status = ClientStatus::DEAD;
                    // Will kill this thread
                    println!("Failed to read from client {}", e);
                    return;
                }
                msg => println!("Unexpected message from client {:?}", msg),
            }
        }
    }

    pub fn flush(&self) -> Result<(), io::Error> {
        //Without the nop message compression algorithms dont flush immediately.
        if self.comp.intersects(CompMode::STREAM_MASK) {
            self.send_msg(FsyncerMsg::NOP, true)?
        } else {
            self.net.lock().unwrap().write.flush()?
        }
        Ok(())
    }

    pub fn send_msg(&self, msg_data: FsyncerMsg, flush: bool) -> Result<(), io::Error> {
        fn inner(
            serbuf: &[u8],
            mut size: usize,
            net: &mut ClientNetwork,
            flush: bool,
            comp: bool,
        ) -> Result<(), io::Error> {
            let mut nbuf = Vec::new();

            let buf = if let Some(ref mut rt_comp) = net.rt_comp {
                rt_comp.encode(&serbuf[..], &mut nbuf);
                size = nbuf.len();
                &nbuf[..]
            } else {
                &serbuf[..]
            };

            // Uggly way to shortcut error checking
            net.write.write_u32::<BigEndian>(size as u32)?;
            net.write.write_all(&buf)?;
            if flush {
                //println!("Doing funky flush");
                net.write.flush()?;
                // Without the nop message compression algorithms dont flush immediately.
                if comp {
                    inner(&ENCODED_NOP[..], *NOP_SIZE, net, false, comp)?;
                    net.write.flush()?;
                }
                //println!("Finished funky flush");
            }
            Ok(())
        }

        let size = serialized_size(&msg_data)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))? as usize;

        let serbuf = serialize(&msg_data).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        //println!("Sending {} {}", header.op_length, hbuf.len() + buf.len());
        let mut net = self.net.lock().unwrap();

        if net.status == ClientStatus::DEAD {
            // Ignore writes to dead clients, they will be harvested later
            return Ok(());
        }

        let res = inner(
            &serbuf[..],
            size,
            &mut *net,
            flush,
            self.comp.intersects(CompMode::STREAM_MASK),
        );

        if res.is_err() {
            net.status = ClientStatus::DEAD;
        }

        res
    }

    pub fn wait_thread_response(&self) -> i32 {
        {
            let tid = unsafe { transmute::<thread::ThreadId, u64>(thread::current().id()) };
            let mut net = self.net.lock().unwrap();
            net.parked
                .entry(tid)
                .or_insert(Arc::new(ClientResponse::new()))
                .clone()
        }.wait()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // Unblock all threads that could be waiting on this client
        for (_, thread) in self.net.lock().unwrap().parked.iter() {
            assert!(Arc::strong_count(thread) <= 2);
            thread.notify(-1);
        }
    }
}
