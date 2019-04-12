metablock!(cfg(target_family = "unix") {
    mod dispatch_unix;
    pub use self::dispatch_unix::dispatch;
});

metablock!(cfg(target_os = "windows") {
    mod dispatch_windows;
    pub use self::dispatch_windows::dispatch;
    extern crate dokan;
    use self::dokan::AddPrivileges;
    use common::ERROR_SUCCESS;
});

extern crate threadpool;
use self::threadpool::ThreadPool;
use bincode::{deserialize, serialize};
use byteorder::{BigEndian, ReadBytesExt};
use clap::ArgMatches;
use common::*;
use dssc::{chunkmap::ChunkMap, other::ZstdBlock, Compressor};
use lz4;
use net2::TcpStreamExt;
use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex};
use std::{mem::size_of, net::TcpStream, path::Path};
use zstd;

pub struct Client<F: Send + Fn(&VFSCall) -> i32> {
    write: Arc<Mutex<Box<Write + Send>>>,
    read: Box<Read + Send>,
    rcv_buf: Vec<u8>,
    mode: ClientMode,
    rt_comp: Option<Box<Compressor>>,
    pool: Option<ThreadPool>,
    op_callback: F,
}

fn send_msg<W: Write>(mut write: W, msg: FsyncerMsg) -> Result<(), io::Error> {
    //println!("Sending {} {}", header.op_length, hbuf.len() + buf.len());
    let buf =
        serialize(&msg).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    write.write_all(&buf[..])?;
    write.flush()
}

impl<F: 'static + Clone + Send + Fn(&VFSCall) -> i32> Client<F> {
    pub fn new(
        host: &str,
        port: i32,
        init_msg: InitMsg,
        buffer_size: usize,
        dispatch_threads: usize,
        op_callback: F,
    ) -> Result<Self, io::Error> {
        if dispatch_threads != 1 && init_msg.mode != ClientMode::MODE_SYNC {
            panic!(
                "Only synchronous mode is compatible with multiple dispatch \
                 threads"
            );
        }

        let mut stream = TcpStream::connect(format!("{}:{}", host, port))?;

        if init_msg.mode != ClientMode::MODE_ASYNC {
            stream.set_nodelay(true)?;
        }

        stream.set_recv_buffer_size(buffer_size)?;

        send_msg(&mut stream, FsyncerMsg::InitMsg(init_msg.clone()))?;

        let reader = if init_msg.compress.contains(CompMode::STREAM_ZSTD) {
            Box::new(zstd::stream::Decoder::new(stream.try_clone()?)?)
                as Box<Read + Send>
        } else if init_msg.compress.contains(CompMode::STREAM_LZ4) {
            Box::new(lz4::Decoder::new(stream.try_clone()?)?)
                as Box<Read + Send>
        } else {
            Box::new(stream.try_clone()?) as Box<Read + Send>
        };

        let rt_comp: Option<Box<Compressor>> =
            if init_msg.compress.contains(CompMode::RT_DSSC_CHUNKED) {
                Some(Box::new(ChunkMap::new(0.5)))
            } else if init_msg.compress.contains(CompMode::RT_DSSC_ZSTD) {
                Some(Box::new(ZstdBlock::default()))
            } else {
                None
            };

        Ok(Client {
            write: Arc::new(Mutex::new(Box::new(stream))),
            read: reader,
            rcv_buf: Vec::with_capacity(32 * 1024),
            mode: init_msg.mode,
            rt_comp: rt_comp,
            pool: if dispatch_threads == 1 {
                None
            } else {
                Some(ThreadPool::new(dispatch_threads))
            },
            op_callback,
        })
    }

    fn send_msg(&mut self, msg_data: FsyncerMsg) -> Result<(), io::Error> {
        send_msg(&mut *self.write.lock().unwrap(), msg_data)
    }

    fn read_msg<'a, 'b>(&'a mut self) -> Result<FsyncerMsg<'b>, io::Error> {
        let length = self.read.read_u32::<BigEndian>()? as usize;

        debug!(length);

        if self.rcv_buf.len() < length {
            if self.rcv_buf.capacity() < length {
                let extra = length - self.rcv_buf.len();
                self.rcv_buf.reserve(extra);
            }
            unsafe { self.rcv_buf.set_len(length) };
        }

        self.read.read_exact(&mut self.rcv_buf[..length])?;

        let mut dbuf = Vec::new();
        let msgbuf = if let Some(ref mut rt_comp) = self.rt_comp {
            rt_comp.decode(&self.rcv_buf[size_of::<u32>()..length], &mut dbuf);
            &dbuf[..]
        } else {
            &self.rcv_buf[..length]
        };
        deserialize(msgbuf).map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    pub fn cork_server(&mut self) -> Result<(), io::Error> {
        self.send_msg(FsyncerMsg::Cork(0))?;
        loop {
            let msg = self.read_msg()?;
            if let FsyncerMsg::Cork(tid) = msg {
                println!("Acknowledging cork");
                return self.send_msg(FsyncerMsg::AckCork(tid));
            }
        }
    }

    pub fn uncork_server(&mut self) -> Result<(), io::Error> {
        self.send_msg(FsyncerMsg::Uncork)
    }

    pub fn process_ops(&mut self) -> Result<(), io::Error> {
        loop {
            match self.read_msg() {
                Ok(FsyncerMsg::SyncOp(call, tid)) => {
                    if self.mode == ClientMode::MODE_SEMISYNC {
                        self.send_msg(FsyncerMsg::Ack(AckMsg {
                            retcode: ClientAck::Ack,
                            tid,
                        }))?;
                    }

                    let need_ack = self.mode == ClientMode::MODE_SYNC
                        || self.mode == ClientMode::MODE_FLUSHSYNC;
                    let write = self.write.clone();
                    let callback = self.op_callback.clone();

                    let f = move || {
                        let res = (callback)(&call);
                        if need_ack {
                            send_msg(
                                &mut *write.lock().unwrap(),
                                FsyncerMsg::Ack(AckMsg {
                                    retcode: ClientAck::RetCode(res),
                                    tid,
                                }),
                            )
                            .expect("Failed to send ack");
                        }
                    };

                    if self.pool.is_none() {
                        f();
                    } else {
                        self.pool.as_ref().unwrap().execute(f);
                    }
                }
                Ok(FsyncerMsg::AsyncOp(call)) => {
                    // TODO check return status
                    //debug!(call);
                    let _res = (self.op_callback)(&call);
                }
                Ok(FsyncerMsg::Cork(tid)) => {
                    println!("Received cork request");
                    self.send_msg(FsyncerMsg::AckCork(tid))?
                }
                Ok(FsyncerMsg::NOP) | Ok(FsyncerMsg::Uncork) => {} /* Nothing, safe to ingore */
                Err(err) => return Err(err),
                msg => println!(
                    "Unexpected message for current client state {:?}",
                    msg
                ),
            }
        }
    }
}

pub fn client_main(matches: ArgMatches) {
    println!("Calculating destination hash...");
    let client_matches = matches.subcommand_matches("client").unwrap();
    let client_path = Path::new(
        client_matches
            .value_of("mount-path")
            .expect("Destination not specified"),
    )
    .canonicalize()
    .expect("Failed to normalize path");

    let dsthash = hash_metadata(&client_path).expect("Hash failed");
    println!("Destinaton hash is {:x}", dsthash);

    let host = client_matches.value_of("host").expect("No host specified");

    let mode = match client_matches.value_of("sync").unwrap() {
        "sync" => ClientMode::MODE_SYNC,
        "async" => ClientMode::MODE_ASYNC,
        "semi" => ClientMode::MODE_SEMISYNC,
        "flush" => ClientMode::MODE_FLUSHSYNC,
        _ => panic!("That is not possible"),
    };

    let buffer_size = parse_human_size(matches.value_of("buffer").unwrap())
        .expect("Buffer size format incorrect");

    let mut compress = CompMode::empty();

    match client_matches.value_of("stream-compressor").unwrap() {
        "default" | "lz4" => {
            println!("Using a LZ4 stream compressor");
            compress.insert(CompMode::STREAM_LZ4)
        }
        "zstd" => {
            println!("Using a ZSTD stream compressor");
            compress.insert(CompMode::STREAM_ZSTD)
        }
        _ => (),
    }

    match client_matches.value_of("rt-compressor").unwrap() {
        "default" | "zstd" => {
            println!("Using a RT_DSSC_ZSTD realtime compressor");
            compress.insert(CompMode::RT_DSSC_ZSTD)
        }
        "chunked" => {
            println!("Using a RT_DSSC_CHUNKED realtime compressor");
            compress.insert(CompMode::RT_DSSC_CHUNKED)
        }
        "none" | _ => (),
    }

    let iolimit_bps =
        parse_human_size(client_matches.value_of("iolimit").unwrap())
            .expect("Invalid format for iolimit");

    #[cfg(target_os = "windows")]
    unsafe {
        if AddPrivileges() == 0 {
            panic!("Failed to add security priviledge");
        }
    }

    let mut client = Client::new(
        host,
        matches
            .value_of("port")
            .map(|v| v.parse().expect("Invalid format for port"))
            .unwrap(),
        InitMsg {
            mode,
            dsthash,
            compress,
            iolimit_bps,
        },
        buffer_size,
        client_matches
            .value_of("threads")
            .map(|v| v.parse().expect("Invalid thread number"))
            .unwrap(),
        move |call| unsafe {
            let e = dispatch(call, &client_path);
            #[cfg(target_family = "unix")]
            let failed = e < 0;
            #[cfg(target_os = "windows")]
            let failed = e != ERROR_SUCCESS;
            if failed {
                println!(
                    "Dispatch {:?} failed {:?}({})",
                    call,
                    io::Error::from_raw_os_error(e),
                    e
                );
            }
            e
        },
    )
    .expect("Failed to connect to fsyncer");

    println!("Connected to {}", host);

    client.process_ops().expect("Stopped processing ops!");
}
