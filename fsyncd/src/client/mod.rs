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
use error::{Error, FromError};
use net2::TcpStreamExt;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::{fs::File, mem::size_of, net::TcpStream, path::Path};
use url::Url;

pub struct ServerConnection<O: Write + Send + 'static> {
    write: Arc<Mutex<O>>,
    read: Box<Read>,
    rcv_buf: Vec<u8>,
    mode: ClientMode,
    rt_comp: Option<Box<Compressor>>,
}

fn send_msg<W: Write>(mut write: W, msg: FsyncerMsg) -> Result<(), io::Error> {
    //eprintln!("Sending {} {}", header.op_length, hbuf.len() + buf.len());
    let buf =
        serialize(&msg).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    write.write_all(&buf[..])?;
    write.flush()
}

pub struct ConnectionBuilder<
    I: Read + Send + 'static,
    O: Write + Send + 'static,
> {
    netin: I,
    netout: O,
    init_msg: InitMsg,
    rsynced: bool,
}

impl ConnectionBuilder<Box<Read + Send>, Box<Write + Send>> {
    pub fn with_url(
        url: &Url,
        nodelay: bool,
        buffer_size: usize,
        init_msg: InitMsg,
    ) -> Result<Self, Error<io::Error>> {
        let (netin, netout) = match url.scheme() {
            "tcp" => {
                let mut url = url.clone();
                if url.port().is_none() {
                    url.set_port(Some(2323)).unwrap();
                }
                let mut stream = trace!(TcpStream::connect(url));
                if nodelay {
                    trace!(stream.set_nodelay(true));
                }
                trace!(stream.set_recv_buffer_size(buffer_size));
                (
                    Box::new(trace!(stream.try_clone())) as Box<Read + Send>,
                    Box::new(stream) as Box<Write + Send>,
                )
            }
            #[cfg(target_family = "unix")]
            "unix" => {
                use std::os::unix::net::UnixStream;
                let stream = trace!(UnixStream::connect(url.path()));
                (
                    Box::new(trace!(stream.try_clone())) as Box<Read + Send>,
                    Box::new(stream) as Box<Write + Send>,
                )
            }
            "stdio" => {
                use std::os::unix::io::FromRawFd;
                unsafe {
                    (
                        Box::new(File::from_raw_fd(0)) as Box<Read + Send>,
                        Box::new(File::from_raw_fd(1)) as Box<Write + Send>,
                    )
                }
            }
            otherwise => panic!("Scheme {} is not supported", otherwise),
        };
        ConnectionBuilder::with_net(netin, netout, init_msg)
    }
}

impl<I: Read + Send + 'static, O: Write + Send + 'static>
    ConnectionBuilder<I, O>
{
    pub fn with_net(
        netin: I,
        mut netout: O,
        init_msg: InitMsg,
    ) -> Result<Self, Error<io::Error>> {
        trace!(send_msg(&mut netout, FsyncerMsg::InitMsg(init_msg.clone())));
        Ok(ConnectionBuilder {
            netin,
            netout,
            init_msg,
            rsynced: false,
        })
    }
    pub fn rsync(mut self, path: &Path) -> Result<Self, Error<io::Error>> {
        if self.rsynced {
            return Ok(self);
        }
        if !self.init_msg.options.contains(Options::INITIAL_RSYNC) {
            panic!(
                "Cannot rsync without telling server first, use INITIAL_RSYNC \
                 option"
            )
        }
        let (ni, no) = trace!(rsync::client(self.netin, self.netout, path));
        self.netin = ni;
        self.netout = no;
        Ok(self)
    }
    pub fn build(self) -> Result<ServerConnection<O>, Error<io::Error>> {
        if self.init_msg.options.contains(Options::INITIAL_RSYNC)
            && !self.rsynced
        {
            panic!(
                "If you requested rsync, you must rsync first before building \
                 a connection"
            )
        }
        let reader = if self.init_msg.compress.contains(CompMode::STREAM_ZSTD) {
            Box::new(trace!(zstd::stream::Decoder::new(self.netin)))
                as Box<Read>
        } else if self.init_msg.compress.contains(CompMode::STREAM_LZ4) {
            Box::new(trace!(lz4::Decoder::new(self.netin))) as Box<Read>
        } else {
            Box::new(self.netin) as Box<Read>
        };

        let rt_comp: Option<Box<Compressor>> =
            if self.init_msg.compress.contains(CompMode::RT_DSSC_CHUNKED) {
                Some(Box::new(ChunkMap::new(0.5)))
            } else if self.init_msg.compress.contains(CompMode::RT_DSSC_ZSTD) {
                Some(Box::new(ZstdBlock::default()))
            } else {
                None
            };

        Ok(ServerConnection {
            write: Arc::new(Mutex::new(self.netout)),
            read: reader,
            rcv_buf: Vec::with_capacity(32 * 1024),
            mode: self.init_msg.mode,
            rt_comp: rt_comp,
        })
    }
}

impl<O: Write + Send + 'static> ServerConnection<O> {
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
                eprintln!("Acknowledging cork");
                return self.send_msg(FsyncerMsg::AckCork(tid));
            }
        }
    }

    pub fn uncork_server(&mut self) -> Result<(), io::Error> {
        self.send_msg(FsyncerMsg::Uncork)
    }

    pub fn process_ops(
        &mut self,
        dispatch_threads: usize,
        path: PathBuf,
    ) -> Result<(), io::Error> {
        fn callback(call: &VFSCall, client_path: &Path) -> i32 {
            let e = unsafe { dispatch(call, client_path) };
            #[cfg(target_family = "unix")]
            let failed = e < 0;
            #[cfg(target_os = "windows")]
            let failed = e as u32 != ERROR_SUCCESS;
            if failed {
                eprintln!(
                    "Dispatch {:?} failed {:?}({})",
                    call,
                    io::Error::from_raw_os_error(e),
                    e
                );
            }
            e
        }

        let pool = if dispatch_threads > 1 {
            Some(ThreadPool::new(dispatch_threads))
        } else {
            None
        };

        let path = Arc::new(path.to_path_buf());

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
                    let path = path.clone();

                    let f = move || {
                        let res = (callback)(&call, &path);
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
                    if let Some(pool) = pool.as_ref() {
                        pool.execute(f);
                    } else {
                        f();
                    }
                }
                Ok(FsyncerMsg::AsyncOp(call)) => {
                    // TODO check return status
                    //debug!(call);
                    let _res = callback(&call, &path);
                }
                Ok(FsyncerMsg::Cork(tid)) => {
                    eprintln!("Received cork request");
                    self.send_msg(FsyncerMsg::AckCork(tid))?
                }
                Ok(FsyncerMsg::NOP) | Ok(FsyncerMsg::Uncork) => {} /* Nothing, safe to ingore */
                Err(err) => return Err(err),
                msg => eprintln!(
                    "Unexpected message for current client state {:?}",
                    msg
                ),
            }
        }
    }
}

fn parse_options(matches: &ArgMatches) -> InitMsg {
    let client_matches = matches.subcommand_matches("client").unwrap();

    let mode = match client_matches.value_of("sync").unwrap() {
        "sync" => ClientMode::MODE_SYNC,
        "async" => ClientMode::MODE_ASYNC,
        "semi" => ClientMode::MODE_SEMISYNC,
        "flush" => ClientMode::MODE_FLUSHSYNC,
        _ => panic!("That is not possible"),
    };

    let mut compress = CompMode::empty();

    match client_matches.value_of("stream-compressor").unwrap() {
        "default" | "lz4" => {
            eprintln!("Using a LZ4 stream compressor");
            compress.insert(CompMode::STREAM_LZ4)
        }
        "zstd" => {
            eprintln!("Using a ZSTD stream compressor");
            compress.insert(CompMode::STREAM_ZSTD)
        }
        _ => (),
    }

    match client_matches.value_of("rt-compressor").unwrap() {
        "default" | "zstd" => {
            eprintln!("Using a RT_DSSC_ZSTD realtime compressor");
            compress.insert(CompMode::RT_DSSC_ZSTD)
        }
        "chunked" => {
            eprintln!("Using a RT_DSSC_CHUNKED realtime compressor");
            compress.insert(CompMode::RT_DSSC_CHUNKED)
        }
        "none" | _ => (),
    }

    let mut options = Options::empty();

    if client_matches.is_present("rsync") {
        options.insert(Options::INITIAL_RSYNC);
    }

    let iolimit_bps =
        parse_human_size(client_matches.value_of("iolimit").unwrap())
            .expect("Invalid format for iolimit");

    InitMsg {
        mode,
        dsthash: 0,
        compress,
        iolimit_bps,
        options,
    }
}

pub fn client_main(matches: ArgMatches) {
    let url = Url::parse(matches.value_of("url").unwrap())
        .expect("Invalid url specified");

    let client_matches = matches.subcommand_matches("client").unwrap();

    let mut init_msg = parse_options(client_matches);
    let buffer_size = parse_human_size(matches.value_of("buffer").unwrap())
        .expect("Buffer size format incorrect");

    let dispatch_threads = client_matches
        .value_of("threads")
        .map(|v| v.parse().expect("Invalid thread number"))
        .unwrap();

    if dispatch_threads != 1 && init_msg.mode != ClientMode::MODE_SYNC {
        panic!(
            "Only synchronous mode is compatible with multiple dispatch \
             threads"
        );
    }

    let client_path = Path::new(
        client_matches
            .value_of("mount-path")
            .expect("Destination not specified"),
    )
    .canonicalize()
    .expect("Failed to normalize path");

    eprintln!("Calculating destination hash...");

    init_msg.dsthash = hash_metadata(&client_path).expect("Hash failed");
    eprintln!("Destinaton hash is {:x}", init_msg.dsthash);

    #[cfg(target_os = "windows")]
    unsafe {
        if AddPrivileges() == 0 {
            panic!("Failed to add security priviledge");
        }
    }

    let need_rsync = init_msg.options.contains(Options::INITIAL_RSYNC);
    let mut builder = ConnectionBuilder::with_url(
        &url,
        init_msg.mode != ClientMode::MODE_ASYNC,
        buffer_size,
        init_msg,
    )
    .expect("Failed to connect to client");
    if need_rsync {
        builder = builder
            .rsync(&client_path)
            .expect("Failed to rsync with server")
    }
    let mut client =
        builder.build().expect("Failed to create server connection");

    eprintln!("Connected to {}", url);
    client
        .process_ops(dispatch_threads, client_path)
        .expect("Stopped processing ops!");
}
