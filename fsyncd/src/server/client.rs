extern crate iolimit;

use self::iolimit::LimitWriter;
use bincode::{deserialize_from, serialize, serialize_into, serialized_size};
use byteorder::{BigEndian, WriteBytesExt};
use common::*;
use dssc::{chunkmap::ChunkMap, other::ZstdBlock, Compressor};
use error::{Error, FromError};
use net2::TcpStreamExt;
use server::{cork_server, uncork_server};
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::sync::{Arc, Condvar, Mutex};
use std::{
    mem::transmute, net::TcpStream, ops::Deref, path::PathBuf, thread,
    time::Duration,
};
use {lz4, zstd};

const CLIENT_RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);

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
    parked: HashMap<u64, Arc<ClientResponse<ClientAck>>>,
    status: ClientStatus,
}

pub struct Client {
    pub mode: ClientMode,
    comp: CompMode,
    net: Arc<Mutex<ClientNetwork>>,
}

pub struct ClientResponse<T> {
    data: Mutex<Option<T>>,
    cvar: Condvar,
}

struct NagleFlush(TcpStream);

#[cfg(target_os = "linux")]
impl Write for NagleFlush {
    fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
        use libc::*;
        use std::os::unix::io::AsRawFd;
        let res = unsafe {
            send(
                self.0.as_raw_fd(),
                buf.as_ptr() as *const _,
                buf.len(),
                MSG_MORE,
            )
        };
        if res == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(res as usize)
    }
    fn flush(&mut self) -> Result<(), io::Error> {
        use libc::*;
        use std::mem;
        use std::os::unix::io::AsRawFd;
        let optval = 0;
        unsafe {
            setsockopt(
                self.0.as_raw_fd(),
                SOL_TCP,
                TCP_CORK,
                &optval as *const _ as *const _,
                mem::size_of::<i32>() as u32,
            )
        };
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
impl Write for NagleFlush {
    fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
        self.0.write()
    }
    fn flush(&mut self) -> Result<(), io::Error> {
        self.0.set_nodelay(true)?;
        self.0.set_nodelay(false)
    }
}

impl<T> ClientResponse<T> {
    pub fn new() -> Self {
        ClientResponse {
            data: Mutex::new(None),
            cvar: Condvar::new(),
        }
    }
    pub fn wait(&self) -> Option<T> {
        let mut lock = self.data.lock().unwrap();

        while lock.is_none() {
            let (ll, timeout) = self
                .cvar
                .wait_timeout(lock, CLIENT_RESPONSE_TIMEOUT)
                .unwrap();
            lock = ll;
            if timeout.timed_out() {
                return None;
            }
        }
        Some(lock.take().unwrap())
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
    ) -> Result<Self, Error<io::Error>> {
        println!("Received connection from client {:?}", stream.peer_addr());
        trace!(stream.set_send_buffer_size(buffer_size));

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
            let srchash =
                hash_metadata(&storage_path).expect("Hash check failed");
            println!("Source hash is {:x}", srchash);
            if init.dsthash != srchash {
                println!(
                    "{:x} != {:x} client's hash does not match!",
                    init.dsthash, srchash
                );
                println!("Dropping this client!");
                drop(stream);
                return Err(trace_err!(io::Error::new(
                    io::ErrorKind::Other,
                    "Hash mismatch",
                )));
            }
        }

        let limiter = LimitWriter::new(
            NagleFlush(trace!(stream.try_clone())),
            init.iolimit_bps,
        );

        let writer = if init.compress.contains(CompMode::STREAM_ZSTD) {
            Box::new(trace!(zstd::stream::Encoder::new(limiter, 0)))
                as Box<Write + Send>
        } else if init.compress.contains(CompMode::STREAM_LZ4) {
            Box::new(trace!(lz4::EncoderBuilder::new().build(limiter)))
                as Box<Write + Send>
        } else {
            Box::new(limiter) as Box<Write + Send>
        };

        let rt_comp: Option<Box<Compressor>> =
            if init.compress.contains(CompMode::RT_DSSC_CHUNKED) {
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
    pub fn cork(&self) -> Result<(), Error<io::Error>> {
        let current_thread = thread::current();
        let tid =
            unsafe { transmute::<thread::ThreadId, u64>(current_thread.id()) };
        let wait = trace!(self.response_msg(
            FsyncerMsg::Cork(tid),
            true,
            self.mode != ClientMode::MODE_CONTROL
        ));
        // Cannot park on control as it will block its reader thread
        if self.mode != ClientMode::MODE_CONTROL {
            wait.unwrap().wait().expect("Client did not respond");
        }
        Ok(())
    }

    pub fn status(&self) -> ClientStatus {
        self.net.lock().unwrap().status
    }

    pub fn uncork(&self) -> Result<(), Error<io::Error>> {
        Ok(trace!(self.send_msg(FsyncerMsg::Uncork, false)))
    }

    fn read_msg<R: Read>(read: &mut R) -> Result<FsyncerMsg, Error<io::Error>> {
        deserialize_from(read) // Is this potentially slow?
            .map_err(|e| trace_err!(io::Error::new(io::ErrorKind::Other, e)))
    }

    fn reader<R: Read>(mut read: R, net: Arc<Mutex<ClientNetwork>>) {
        let net = net.deref();
        loop {
            match Client::read_msg(&mut read) {
                Ok(FsyncerMsg::AckCork(tid)) => {
                    let mut netlock = net.lock().unwrap();
                    netlock.parked.get(&tid).map(|t| {
                        assert!(Arc::strong_count(t) <= 2);
                        t.notify(ClientAck::Ack)
                    });
                }
                Ok(FsyncerMsg::Ack(AckMsg { retcode: code, tid })) => {
                    let mut netlock = net.lock().unwrap();
                    // I shouldn't randomly insert whatever the client sends,
                    // but oh well...
                    let cond = netlock
                        .parked
                        .entry(tid)
                        .or_insert(Arc::new(ClientResponse::new()));
                    assert!(Arc::strong_count(cond) <= 2);
                    cond.notify(code);
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

    pub fn flush(&self) -> Result<(), Error<io::Error>> {
        //Without the nop message compression algorithms dont flush
        // immediately.
        if self.comp.intersects(CompMode::STREAM_MASK) {
            trace!(self.send_msg(FsyncerMsg::NOP, true))
        } else {
            trace!(self.net.lock().unwrap().write.flush())
        }
        Ok(())
    }

    pub fn response_msg(
        &self,
        msg_data: FsyncerMsg,
        flush: bool,
        want_response: bool,
    ) -> Result<Option<Arc<ClientResponse<ClientAck>>>, Error<io::Error>> {
        fn inner(
            serbuf: &[u8],
            mut size: usize,
            net: &mut ClientNetwork,
            flush: bool,
            comp: bool,
        ) -> Result<(), Error<io::Error>> {
            let mut nbuf = Vec::new();

            let buf = if let Some(ref mut rt_comp) = net.rt_comp {
                rt_comp.encode(&serbuf[..], &mut nbuf);
                size = nbuf.len();
                &nbuf[..]
            } else {
                &serbuf[..]
            };

            trace!(net.write.write_u32::<BigEndian>(size as u32));
            trace!(net.write.write_all(&buf));
            if flush {
                trace!(net.write.flush());
                // Without the nop message compression algorithms dont flush
                // immediately.
                if comp {
                    trace!(inner(
                        &ENCODED_NOP[..],
                        *NOP_SIZE,
                        net,
                        false,
                        comp
                    ));
                    trace!(net.write.flush());
                }
            }
            Ok(())
        }

        let size = trace!(serialized_size(&msg_data)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e)))
            as usize;

        let mut serbuf = Vec::with_capacity(size);

        trace!(serialize_into(&mut serbuf, &msg_data)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e)));

        //println!("Sending {} {}", header.op_length, hbuf.len() + buf.len());
        let mut net = self.net.lock().unwrap();

        if net.status == ClientStatus::DEAD {
            // Ignore writes to dead clients, they will be harvested later
            return Err(trace_err!(io::Error::new(
                io::ErrorKind::Other,
                "Client is dead"
            )));
        }

        let resp = if want_response {
            let tid = unsafe {
                transmute::<thread::ThreadId, u64>(thread::current().id())
            };
            Some(
                net.parked
                    .entry(tid)
                    .or_insert(Arc::new(ClientResponse::new()))
                    .clone(),
            )
        } else {
            None
        };

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
        res.map(|_| resp)
    }

    pub fn send_msg(
        &self,
        msg_data: FsyncerMsg,
        flush: bool,
    ) -> Result<(), Error<io::Error>> {
        self.response_msg(msg_data, flush, false)?;
        Ok(())
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // Unblock all threads that could be waiting on this client
        for (_, thread) in self.net.lock().unwrap().parked.iter() {
            assert!(Arc::strong_count(thread) <= 2);
            thread.notify(ClientAck::Dead);
        }
    }
}
