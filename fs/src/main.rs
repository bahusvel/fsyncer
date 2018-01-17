#![feature(libc)]

extern crate common;
#[macro_use]
extern crate lazy_static;
extern crate libc;
extern crate net2;
extern crate zstd;
extern crate clap;

use libc::{c_char, c_void, free};
use std::ffi::CString;
use std::net::{TcpListener, TcpStream};
use net2::TcpStreamExt;
use std::io;
use std::io::{Read, Write};
use std::mem::{size_of, transmute};
use std::thread;
use std::sync::Mutex;
use std::slice;
use std::ops::DerefMut;
use common::*;
use clap::{App, Arg, AppSettings};
use std::ptr::null;

struct Client {
    write: Box<Write + Send>,
    read: Box<Read + Send>,
    mode: client_mode,
}

#[link(name = "fsyncer", kind = "static")]
#[link(name = "fuse3")]
extern "C" {
    fn fsyncer_fuse_main(
        argc: i32,
        argv: *const *const c_char,
        sop: extern "C" fn(*const c_void) -> i32,
    ) -> i32;
    fn hash_metadata(path: *const c_char) -> u64;
}

#[no_mangle]
#[allow(non_upper_case_globals)]
pub static mut dst_path: *const c_char = null();

lazy_static!{
    static ref SYNC_LIST: Mutex<Vec<Client>> = Mutex::new(Vec::new());
}

fn handle_client(mut stream: TcpStream, dontcheck: bool) -> Result<(), io::Error> {
    stream.set_send_buffer_size(1024 * 1024)?;
    let mut init_buf = [0; size_of::<init_msg>()];
    stream.read_exact(&mut init_buf)?;

    println!("Calculating source hash...");
    let srchash = unsafe { hash_metadata(dst_path) };
    println!("Source hash is {:x}", srchash);

    let init = unsafe { transmute::<[u8; size_of::<init_msg>()], init_msg>(init_buf) };

    if dontcheck && init.dsthash != srchash {
        println!(
            "%{:x} != {:x} client's hash does not match!",
            init.dsthash,
            srchash
        );
        println!("Dropping this client!");
        drop(stream);
        return Err(io::Error::new(io::ErrorKind::Other, "Hash mismatch"));
    }

    if init.mode == client_mode::MODE_SYNC {
        stream.set_nodelay(true)?;
    }

    let writer = if init.compress && init.mode == client_mode::MODE_ASYNC {
        Box::new(zstd::stream::Encoder::new(stream.try_clone()?, 0)?) as Box<Write + Send>
    } else {
        Box::new(stream.try_clone()?) as Box<Write + Send>
    };

    SYNC_LIST.lock().expect("Failed to lock SYNC_LIST").push(
        Client {
            write: writer,
            read: Box::new(stream),
            mode: init.mode,
        },
    );

    println!("Client connected!");

    Ok(())
}

#[no_mangle]
pub extern "C" fn send_op(msg_data: *const c_void) -> i32 {
    let msg = unsafe { &*(msg_data as *const op_msg) };
    let mut res = SYNC_LIST.lock().expect("Failed to lock SYNC_LIST");
    let list = res.deref_mut();
    let mut delete = Vec::new();
    for (i, ref mut client) in list.into_iter().enumerate() {
        let buf = unsafe { slice::from_raw_parts(msg_data as *const u8, msg.op_length as usize) };
        if client.write.write_all(&buf).is_err() {
            println!("Failed sending op to client");
            delete.push(i);
            continue;
        }
        if client.mode == client_mode::MODE_SYNC {
            let mut ack_buf = [0; size_of::<ack_msg>()];
            let ack = client.read.read_exact(&mut ack_buf);
            if ack.is_err() {
                println!("Failed receiving ack from client {:?}", ack);
                delete.push(i);
                continue;
            }
        }
    }
    for i in delete {
        list.remove(i);
    }
    unsafe { free(msg_data as *mut c_void) };
    0
}

fn main() {
    let matches = App::new("Fsyncer MirrorFS Server")
        .version("0.0")
        .author("Denis Lavrov <bahus.vel@gmail.com>")
        .about(
            "Serves the filesystem and replicates all changes to clients",
        )
        .setting(AppSettings::TrailingVarArg)
        .arg(
            Arg::with_name("storage")
                .short("s")
                .long("storage")
                .help("Fsyncer host to connection to")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("port")
                .short("p")
                .long("port")
                .default_value("2323")
                .help("Port the fsyncer is running on")
                .takes_value(true),
        )
        .arg(Arg::with_name("dont-check").long("dont-check").help(
            "Disables comparison of the source and destination",
        ))
        .get_matches_from(std::env::args().take_while(|v| v != "--"));

    let c_dst = CString::new(matches.value_of("storage").expect("Storage not specified"));
    unsafe {
        dst_path = c_dst.unwrap().into_raw();
    }

    // FIXME use net2::TcpBuilder to set SO_REUSEADDR
    let listener = TcpListener::bind(format!(
        "0.0.0.0:{}",
        matches
            .value_of("port")
            .map(|v| v.parse::<i32>().expect("Invalid format for port"))
            .unwrap()
    )).expect("Could not create server socket");

    let dont_check = matches.is_present("dont-check");

    thread::spawn(move || for stream in listener.incoming() {
        handle_client(stream.expect("Failed client connection"), dont_check);
    });

    let args = std::env::args()
        .take(1)
        .chain(std::env::args().skip_while(|v| v != "--").skip(1))
        .map(|arg| CString::new(arg).unwrap())
        .collect::<Vec<CString>>();
    // convert the strings to raw pointers
    let c_args = args.iter()
        .map(|arg| arg.as_ptr())
        .collect::<Vec<*const c_char>>();

    unsafe { fsyncer_fuse_main(c_args.len() as i32, c_args.as_ptr(), send_op) };
}
