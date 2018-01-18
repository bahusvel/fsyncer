#![feature(libc)]
extern crate libc;
extern crate zstd;
extern crate net2;

use std::net::TcpStream;
use net2::TcpStreamExt;
use std::io;
use std::io::{Read, Write};
use std::mem::{size_of, transmute};
use libc::{c_void, c_char};
use std::ffi::CString;

pub struct Client {
    write: Box<Write + Send>,
    read: Box<Read + Send>,
    mode: client_mode,
    op_callback: fn(msg: *const c_void) -> i32,
}

impl Client {
    pub fn new(
        host: &str,
        port: i32,
        mode: client_mode,
        dsthash: u64,
        compress: bool,
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

        let reader = if compress && mode == client_mode::MODE_ASYNC {
            Box::new(zstd::stream::Decoder::new(stream.try_clone()?)?) as Box<Read + Send>
        } else {
            Box::new(stream.try_clone()?) as Box<Read + Send>
        };


        Ok(Client {
            write: Box::new(stream),
            read: reader,
            mode,
            op_callback,
        })
    }

    pub fn process_ops(&mut self) -> Result<(), io::Error> {
        let mut header_buf = [0; size_of::<op_msg>()];
        let mut rcv_buf = [0; 33 * 1024];
        loop {
            self.read.read_exact(&mut header_buf)?;
            let msg = unsafe { transmute::<[u8; size_of::<op_msg>()], op_msg>(header_buf) };
            rcv_buf[..size_of::<op_msg>()].copy_from_slice(&header_buf);
            self.read.read_exact(
                &mut rcv_buf[size_of::<op_msg>()..
                                 msg.op_length as usize],
            )?;

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



#[repr(C)]
#[derive(PartialEq, Clone, Copy)]
pub enum op_type {
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
#[derive(PartialEq, Clone, Copy)]
#[allow(non_camel_case_types)]
pub enum client_mode {
    MODE_ASYNC,
    MODE_SYNC,
    MODE_CONTROL,
}

#[repr(C)]
pub struct op_msg {
    pub op_length: u32,
    pub op_type: op_type,
}

#[repr(C)]
pub struct init_msg {
    pub mode: client_mode,
    pub dsthash: u64,
    pub compress: bool,
}

#[repr(C)]
pub struct ack_msg {
    pub retcode: i32,
}

#[link(name = "fsyncer_common", kind = "static")]
extern "C" {
    fn hash_metadata(path: *const c_char) -> u64;
}

pub fn hash_mdata(path: &str) -> u64 {
    let s = CString::new(path).unwrap();
    unsafe { hash_metadata(s.as_ptr()) }
}
