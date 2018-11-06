use bincode::deserialize;
use clap::ArgMatches;
use common::*;
use dispatch::dispatch;
use dssc::chunkmap::ChunkMap;
use dssc::other::ZstdBlock;
use dssc::Compressor;
use lz4;
use net2::TcpStreamExt;
use std::io;
use std::io::{Read, Write};
use std::mem::{size_of, transmute};
use std::net::TcpStream;
use zstd;

pub struct Client<F: Fn(VFSCall) -> i32> {
    write: Box<Write + Send>,
    read: Box<Read + Send>,
    mode: ClientMode,
    rt_comp: Option<Box<Compressor>>,
    op_callback: F,
}

impl<F: Fn(VFSCall) -> i32> Client<F> {
    pub fn new(
        host: &str,
        port: i32,
        mode: ClientMode,
        dsthash: u64,
        compress: CompMode,
        buffer_size: usize,
        op_callback: F,
    ) -> Result<Self, io::Error> {
        let mut stream = TcpStream::connect(format!("{}:{}", host, port))?;

        stream.set_recv_buffer_size(buffer_size * 1024 * 1024)?;

        if mode == ClientMode::MODE_SYNC || mode == ClientMode::MODE_SEMISYNC {
            stream.set_nodelay(true)?;
        }

        let init = unsafe {
            transmute::<InitMsg, [u8; size_of::<InitMsg>()]>(InitMsg {
                mode,
                dsthash,
                compress,
            })
        };

        stream.write_all(&init)?;

        let reader = if compress.contains(CompMode::STREAM_ZSTD) {
            Box::new(zstd::stream::Decoder::new(stream.try_clone()?)?) as Box<Read + Send>
        } else if compress.contains(CompMode::STREAM_LZ4) {
            Box::new(lz4::Decoder::new(stream.try_clone()?)?) as Box<Read + Send>
        } else {
            Box::new(stream.try_clone()?) as Box<Read + Send>
        };

        let rt_comp: Option<Box<Compressor>> = if compress.contains(CompMode::RT_DSSC_CHUNKED) {
            Some(Box::new(ChunkMap::new(0.5)))
        } else if compress.contains(CompMode::RT_DSSC_ZSTD) {
            Some(Box::new(ZstdBlock::default()))
        } else {
            None
        };

        Ok(Client {
            write: Box::new(stream),
            read: reader,
            mode,
            rt_comp: rt_comp,
            op_callback,
        })
    }

    fn write_ack(&mut self, retcode: i32, tid: u64) -> Result<(), io::Error> {
        let ack =
            unsafe { transmute::<AckMsg, [u8; size_of::<AckMsg>()]>(AckMsg { retcode, tid }) };
        self.write.write_all(&ack)
    }

    pub fn process_ops(&mut self) -> Result<(), io::Error> {
        let mut rcv_buf = [0; 33 * 1024];
        const MSG_SIZE: usize = size_of::<u32>();
        loop {
            self.read.read_exact(&mut rcv_buf[..MSG_SIZE])?;
            let length = unsafe { *(rcv_buf.as_ptr() as *const u32) } as usize;

            assert!(MSG_SIZE + length <= rcv_buf.len());

            self.read
                .read_exact(&mut rcv_buf[MSG_SIZE..MSG_SIZE + length])?;

            let mut dbuf = Vec::new();
            let msgbuf = if let Some(ref mut rt_comp) = self.rt_comp {
                rt_comp.decode(&rcv_buf[size_of::<u32>()..MSG_SIZE + length], &mut dbuf);
                &dbuf[..]
            } else {
                &rcv_buf[MSG_SIZE..MSG_SIZE + length]
            };

            match deserialize(msgbuf) {
                Ok(FsyncerMsg::SyncOp(call, tid)) => {
                    if self.mode == ClientMode::MODE_SEMISYNC {
                        self.write_ack(0, tid)?;
                    }

                    let res = (self.op_callback)(call);

                    if self.mode == ClientMode::MODE_SYNC {
                        self.write_ack(res, tid)?;
                    }
                }
                Ok(FsyncerMsg::AsyncOp(call)) => {
                    // TODO check return status
                    let _res = (self.op_callback)(call);
                }
                Ok(FsyncerMsg::NOP) => {} // Nothing, safe to ingore
                Err(err) => println!("Failed to read message from client {}", err),
                msg => println!("Unexpected message for current client state {:?}", msg),
            }
        }
    }
}

/*
fn do_call_wrapper(message: *const c_void) -> i32 {
    //println!("Received call");
    let res = unsafe { do_call(message) };
    if res < 0 {
        unsafe { perror(CString::new("Error in replay").unwrap().as_ptr()) };
    }
    res
}
*/

pub fn client_main(matches: ArgMatches) {
    println!("Calculating destination hash...");
    let dsthash = hash_metadata(
        matches
            .value_of("mount-path")
            .expect("No destination specified"),
    ).expect("Hash failed");
    println!("Destinaton hash is {:x}", dsthash);

    let mode = match matches.value_of("sync").unwrap() {
        "sync" => ClientMode::MODE_SYNC,
        "async" => ClientMode::MODE_ASYNC,
        "semisync" => ClientMode::MODE_SEMISYNC,
        _ => panic!("That is not possible"),
    };

    let buffer_size = matches
        .value_of("buffer")
        .and_then(|b| b.parse().ok())
        .expect("Buffer format incorrect");

    let client_path = matches
        .value_of("mount-path")
        .expect("Destination not specified");

    let mut comp = CompMode::empty();

    match matches.value_of("stream-compressor").unwrap() {
        "default" | "lz4" => {
            println!("Using a LZ4 stream compressor");
            comp.insert(CompMode::STREAM_LZ4)
        }
        "zstd" => {
            println!("Using a ZSTD stream compressor");
            comp.insert(CompMode::STREAM_ZSTD)
        }
        _ => (),
    }

    match matches.value_of("rt-compressor").unwrap() {
        "default" | "zstd" => {
            println!("Using a RT_DSSC_ZSTD realtime compressor");
            comp.insert(CompMode::RT_DSSC_ZSTD)
        }
        "chunked" => {
            println!("Using a RT_DSSC_CHUNKED realtime compressor");
            comp.insert(CompMode::RT_DSSC_CHUNKED)
        }
        "none" | _ => (),
    }

    let mut client = Client::new(
        matches.value_of("client").expect("No host specified"),
        matches
            .value_of("port")
            .map(|v| v.parse().expect("Invalid format for port"))
            .unwrap(),
        mode,
        dsthash,
        comp,
        buffer_size,
        |call| unsafe { dispatch(call, client_path) },
    ).expect("Failed to connect to fsyncer");

    println!(
        "Connected to {}",
        matches.value_of("client").expect("No host specified")
    );

    client.process_ops().expect("Stopped processing ops!");
}
