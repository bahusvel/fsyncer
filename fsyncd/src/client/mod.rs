mod dispatch;

use self::dispatch::dispatch;
use bincode::deserialize;
use bincode::{serialize_into, serialized_size};
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use clap::ArgMatches;
use common::*;
use dssc::chunkmap::ChunkMap;
use dssc::other::ZstdBlock;
use dssc::Compressor;
use lz4;
use net2::TcpStreamExt;
use std::io;
use std::io::{Read, Write};
use std::mem::size_of;
use std::net::TcpStream;
use zstd;

pub struct Client<F: Fn(VFSCall) -> i32> {
    write: Box<Write + Send>,
    read: Box<Read + Send>,
    mode: ClientMode,
    rt_comp: Option<Box<Compressor>>,
    op_callback: F,
}

fn send_msg<W: Write>(mut write: W, msg: FsyncerMsg) -> Result<(), io::Error> {
    let size = serialized_size(&msg).map_err(|e| io::Error::new(io::ErrorKind::Other, e))? as usize;

    //println!("Sending {} {}", header.op_length, hbuf.len() + buf.len());
    write.write_u32::<BigEndian>(size as u32)?;
    serialize_into(&mut write, &msg).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    write.flush()
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

        send_msg(
            &mut stream,
            FsyncerMsg::InitMsg(InitMsg {
                mode,
                dsthash,
                compress,
            }),
        )?;

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

    fn send_msg(&mut self, msg_data: FsyncerMsg) -> Result<(), io::Error> {
        send_msg(&mut self.write, msg_data)
    }

    fn read_msg(&mut self) -> Result<FsyncerMsg, io::Error> {
        let mut rcv_buf = [0; 33 * 1024];
        let length = self.read.read_u32::<BigEndian>()? as usize;

        assert!(length <= rcv_buf.len());

        self.read.read_exact(&mut rcv_buf[..length])?;

        let mut dbuf = Vec::new();
        let msgbuf = if let Some(ref mut rt_comp) = self.rt_comp {
            rt_comp.decode(&rcv_buf[size_of::<u32>()..length], &mut dbuf);
            &dbuf[..]
        } else {
            &rcv_buf[..length]
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
                        self.send_msg(FsyncerMsg::Ack(AckMsg { retcode: 0, tid }))?;
                    }

                    let res = (self.op_callback)(call);

                    if self.mode == ClientMode::MODE_SYNC {
                        self.send_msg(FsyncerMsg::Ack(AckMsg { retcode: res, tid }))?;
                    }
                }
                Ok(FsyncerMsg::AsyncOp(call)) => {
                    // TODO check return status
                    let _res = (self.op_callback)(call);
                }
                Ok(FsyncerMsg::Cork(tid)) => self.send_msg(FsyncerMsg::AckCork(tid))?,
                Ok(FsyncerMsg::NOP) | Ok(FsyncerMsg::Uncork) => {} // Nothing, safe to ingore
                Err(err) => return Err(err),
                msg => println!("Unexpected message for current client state {:?}", msg),
            }
        }
    }
}

pub fn client_main(matches: ArgMatches) {
    println!("Calculating destination hash...");
    let client_matches = matches.subcommand_matches("client").unwrap();
    let client_path = client_matches
        .value_of("mount-path")
        .expect("Destination not specified");

    let dsthash = hash_metadata(client_path).expect("Hash failed");
    println!("Destinaton hash is {:x}", dsthash);

    let host = client_matches.value_of("host").expect("No host specified");

    let mode = match client_matches.value_of("sync").unwrap() {
        "sync" => ClientMode::MODE_SYNC,
        "async" => ClientMode::MODE_ASYNC,
        "semisync" => ClientMode::MODE_SEMISYNC,
        _ => panic!("That is not possible"),
    };

    let buffer_size = matches
        .value_of("buffer")
        .and_then(|b| b.parse().ok())
        .expect("Buffer format incorrect");

    let mut comp = CompMode::empty();

    match client_matches.value_of("stream-compressor").unwrap() {
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

    match client_matches.value_of("rt-compressor").unwrap() {
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
        host,
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

    println!("Connected to {}", host);

    client.process_ops().expect("Stopped processing ops!");
}