#![feature(libc)]
extern crate libc;
extern crate net2;
#[macro_use]
extern crate lazy_static;

use libc::{c_char, c_void};
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

#[repr(C)]
struct options {
    real_path: *const c_char,
    port: i32,
    consistent: i32,
    dontcheck: i32,
    show_help: i32,
}

unsafe impl Send for options {}

#[repr(C)]
enum op_type {
    MKNOD,
	MKDIR,
	UNLINK,
	RMDIR,
	SYMLINK,
	RENAME,
	LINK,
	CHMOD,
	CHOWN,
	TRUNCATE,
	WRITE,
	FALLOCATE,
	SETXATTR,
	REMOVEXATTR,
	CREATE,
	UTIMENS,
}

#[repr(C)]
#[derive(PartialEq)]
enum client_mode { MODE_ASYNC, MODE_SYNC, MODE_CONTROL }

#[repr(C)]
pub struct op_msg {
    op_length: u32,
    op_type: op_type,
    data: [u8]
}

#[repr(C)]
struct init_msg {
    mode: client_mode,
    dsthash: u64,
}

#[repr(C)]
struct ack_msg {
	retcode: i32
}

struct Client {
    stream: TcpStream,
    mode: client_mode,
}

extern {
    fn fsyncer_parse_opts(argc: i32, argv: *const *const c_char) -> options;
    fn fsyncer_fuse_main(argc: i32, argv: *const *const c_char) -> i32;
    fn hash_metadata(path: *const c_char) -> u64;
}

lazy_static!{
    static ref SYNC_LIST: Mutex<Vec<Client>> = Mutex::new(Vec::new());
}

fn handle_client(mut stream: TcpStream, options: &options) -> Result<(), io::Error>{
    stream.set_send_buffer_size(1024*1024)?;
    let mut init_buf = [0; size_of::<init_msg>()];
    stream.read_exact(&mut init_buf)?;

    println!("Calculating source hash...");
    let srchash = unsafe {hash_metadata(options.real_path)};
    println!("Source hash is {:x}", srchash);

    let init = unsafe {transmute::<[u8; size_of::<init_msg>()],init_msg>(init_buf)};

    if ((options.dontcheck == 0) && init.dsthash != srchash) {
        println!("%{:x} != {:x} client's hash does not match!",
               init.dsthash, srchash);
        println!("Dropping this client!");
        drop(stream);
        return Err(io::Error::new(io::ErrorKind::Other, "Hash mismatch"));
    }

    if (init.mode == client_mode::MODE_SYNC) {
        stream.set_nodelay(true)?;
    }

    SYNC_LIST.lock().expect("Failed to lock SYNC_LIST").push(Client{stream: stream, mode: init.mode});

    println!("Client connected!");

    Ok(())
}

#[no_mangle]
pub extern "C" fn send_op(msg: *const op_msg) -> i32 {
    let mut res = SYNC_LIST.lock().expect("Failed to lock SYNC_LIST");
    let list = res.deref_mut();
    let mut delete = Vec::new();
    for (i, ref mut client) in list.into_iter().enumerate() {
        let buf = unsafe {slice::from_raw_parts(msg as *const u8, (*msg).op_length as usize)};
        if client.stream.write_all(&buf).is_err() {
            println!("Failed sending op to client");
            delete.push(i);
            continue;
        }
        if client.mode == client_mode::MODE_SYNC {
            let mut ack_buf = [0; size_of::<ack_msg>()];
            if client.stream.read_exact(&mut ack_buf).is_err() {
                println!("Failed receiving ack from client");
                delete.push(i);
                continue;
            }
        }
    }
    for i in delete {
        list.remove(i);
    }
    0
}

fn main() {
    let args = std::env::args().map(|arg| CString::new(arg).unwrap() ).collect::<Vec<CString>>();
    // convert the strings to raw pointers
    let c_args = args.iter().map(|arg| arg.as_ptr()).collect::<Vec<*const c_char>>();

    let options = unsafe {fsyncer_parse_opts(c_args.len() as i32, c_args.as_ptr())};

    let listener = TcpListener::bind(format!("0.0.0.0:{}", options.port)).expect("Could not create server socket");

    thread::spawn(move || {
        for stream in listener.incoming() {
            handle_client(stream.expect("Failed client connection"), &options);
        }
    });

    unsafe {fsyncer_fuse_main(c_args.len() as i32, c_args.as_ptr())};
}