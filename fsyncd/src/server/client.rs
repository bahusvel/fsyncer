extern crate iolimit;

use self::iolimit::LimitWriter;
use bincode::{deserialize_from, serialize, serialize_into, serialized_size};
use byteorder::{BigEndian, WriteBytesExt};
use common::*;
use dssc::{chunkmap::ChunkMap, other::ZstdBlock, Compressor};
use error::{Error, FromError};
use server::net::{MyRead, MyWrite};
use server::{cork_server, uncork_server, SERVER_PATH};
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::sync::{Arc, Condvar, Mutex};
use std::{mem::transmute, ops::Deref, thread, time::Duration};
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
    write: Box<dyn Write + Send>,
    rt_comp: Option<Box<dyn Compressor>>,
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
        mut netin: Box<dyn MyRead>,
        netout: Box<dyn MyWrite>,
        dontcheck: bool,
    ) -> Result<Self, Error<io::Error>> {
        let init = match Client::read_msg(&mut netin) {
            Ok(FsyncerMsg::InitMsg(msg)) => msg,
            Err(e) => return Err(trace_err!(e)),
            otherwise => panic!(
                "Expected init message from client, received {:?}",
                otherwise
            ),
        };

        let storage_path = unsafe { SERVER_PATH.as_ref().unwrap() };

        if !(init.mode == ClientMode::MODE_CONTROL
            || dontcheck
            || init.options.contains(Options::INITIAL_RSYNC))
        {
            eprintln!("Calculating source hash...");
            let srchash =
                hash_metadata(&storage_path).expect("Hash check failed");
            eprintln!("Source hash is {:x}", srchash);
            if init.dsthash != srchash {
                eprintln!(
                    "{:x} != {:x} client's hash does not match!",
                    init.dsthash, srchash
                );
                eprintln!("Dropping this client!");
                drop(netin);
                drop(netout);
                return Err(trace_err!(io::Error::new(
                    io::ErrorKind::Other,
                    "Hash mismatch",
                )));
            }
        }

        if init.options.contains(Options::INITIAL_RSYNC) {
            //trace!(stream.set_nodelay(true));
            eprintln!("Syncrhonising using rsync...");
            trace!(rsync::server(
                netin.as_raw_fd(),
                netout.as_raw_fd(),
                storage_path
            ));
            eprintln!("Done!");
        }

        let limiter = LimitWriter::new(netout, init.iolimit_bps);

        let writer = if init.compress.contains(CompMode::STREAM_ZSTD) {
            Box::new(trace!(zstd::stream::Encoder::new(limiter, 0))) as _
        } else if init.compress.contains(CompMode::STREAM_LZ4) {
            Box::new(trace!(lz4::EncoderBuilder::new().build(limiter))) as _
        } else {
            Box::new(limiter) as _
        };

        let rt_comp: Option<Box<dyn Compressor>> =
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

        thread::spawn(move || Client::reader(netin, net_clone));

        eprintln!("Client connected!");

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
                    let netlock = net.lock().unwrap();
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
                    eprintln!("Failed to read from client {}", e);
                    return;
                }
                msg => eprintln!("Unexpected message from client {:?}", msg),
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

        let size = trace!(
            serialized_size(&msg_data)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
        ) as usize;

        let mut serbuf = Vec::with_capacity(size);

        trace!(
            serialize_into(&mut serbuf, &msg_data)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
        );

        //eprintln!("Sending {} {}", header.op_length, hbuf.len() + buf.len());
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
