use std::net::TcpStream;
use net2::TcpStreamExt;
use std::io;
use std::io::{Read, Write};
use std::mem::{size_of, transmute};
use libc::{c_void, c_char};
use std::ffi::CString;
use clap::ArgMatches;
use std::ptr::null;
use libc::perror;
use common::*;
use zstd;
use dssc::Compressor;
use dssc::flate::FlateCompressor;

extern "C" {
    fn do_call(message: *const c_void) -> i32;
}

#[no_mangle]
#[allow(non_upper_case_globals)]
pub static mut client_path: *const c_char = null();

pub struct Client {
    write: Box<Write + Send>,
    read: Box<Read + Send>,
    mode: client_mode,
    rt_comp: Option<Box<Compressor>>,
    op_callback: fn(msg: *const c_void) -> i32,
}

impl Client {
    pub fn new(
        host: &str,
        port: i32,
        mode: client_mode,
        dsthash: u64,
        compress: CompMode,
        op_callback: fn(msg: *const c_void) -> i32,
    ) -> Result<Self, io::Error> {
        let mut stream = TcpStream::connect(format!("{}:{}", host, port))?;

        stream.set_recv_buffer_size(1024 * 1024)?;

        if mode == client_mode::MODE_SYNC {
            stream.set_nodelay(true)?;
        }

        let init = unsafe {
            transmute::<init_msg, [u8; size_of::<init_msg>()]>(init_msg {
                mode,
                dsthash,
                compress,
            })
        };

        stream.write_all(&init)?;

        let reader = if compress.contains(CompMode::STREAM_ZSTD) {
            Box::new(zstd::stream::Decoder::new(stream.try_clone()?)?) as Box<Read + Send>
        } else {
            Box::new(stream.try_clone()?) as Box<Read + Send>
        };

        let rt_comp: Option<Box<Compressor>> = if compress.contains(CompMode::RT_DSSC_ZLIB) {
            Some(Box::new(FlateCompressor::default()))
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

    pub fn process_ops(&mut self) -> Result<(), io::Error> {
        let mut header_buf = [0; size_of::<op_msg>()];
        let mut rcv_buf = [0; 33 * 1024];
        loop {
            self.read.read_exact(&mut header_buf)?;
            let msg = unsafe { &mut *(rcv_buf.as_ptr() as *mut op_msg) };
            rcv_buf[..size_of::<op_msg>()].copy_from_slice(&header_buf);
            self.read.read_exact(
                &mut rcv_buf[size_of::<op_msg>()..
                                 msg.op_length as usize],
            )?;

            if let Some(ref mut rt_comp) = self.rt_comp {
                //FIXME also inefficient
                let dbuf = rt_comp.decode(&rcv_buf[size_of::<op_msg>()..msg.op_length as usize]);
                rcv_buf[size_of::<op_msg>()..size_of::<op_msg>() + dbuf.len()]
                    .copy_from_slice(&dbuf);
                //msg.op_length = (size_of::<op_msg>() + dbuf.len()) as u32;
            }

            let res = (self.op_callback)(rcv_buf.as_ptr() as *const c_void);

            if self.mode == client_mode::MODE_SYNC {
                let ack = unsafe {
                    transmute::<ack_msg, [u8; size_of::<ack_msg>()]>(ack_msg { retcode: res })
                };
                self.write.write_all(&ack)?;
            }
        }
    }
}

fn do_call_wrapper(message: *const c_void) -> i32 {
    //println!("Received call");
    let res = unsafe { do_call(message) };
    if res < 0 {
        unsafe { perror(CString::new("Error in replay").unwrap().as_ptr()) };
    }
    res
}

pub fn client_main(matches: ArgMatches) {
    println!("Calculating destination hash...");
    let dsthash = hash_metadata(matches.value_of("mount-path").expect(
        "No destination specified",
    )).expect("Hash failed");
    println!("Destinaton hash is {:x}", dsthash);

    let mode = if matches.is_present("sync") {
        client_mode::MODE_SYNC
    } else {
        client_mode::MODE_ASYNC
    };

    let c_dst = CString::new(matches.value_of("mount-path").expect(
        "Destination not specified",
    )).unwrap();
    unsafe {
        client_path = c_dst.into_raw();
    }

    let mut comp = CompMode::empty();

    match matches.value_of("stream-compressor").unwrap() {
        "default" => {
            println!("Using a ZSTD stream compressor");
            comp.insert(CompMode::STREAM_ZSTD)
        }
        _ => (),
    }

    match matches.value_of("rt-compressor").unwrap() {
        "default" => {
            println!("Using a DSSC_ZLIB realtime compressor");
            comp.insert(CompMode::RT_DSSC_ZLIB)
        }
        _ => (),
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
        do_call_wrapper,
    ).expect("Failed to connect to fsyncer");


    println!(
        "Connected to {}",
        matches.value_of("client").expect("No host specified")
    );

    client.process_ops().expect("Stopped processing ops!");
}
