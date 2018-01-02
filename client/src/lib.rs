#![feature(libc)]
extern crate libc;

extern crate common;
extern crate net2;

pub use common::*;
use std::net::TcpStream;
use net2::TcpStreamExt;
use std::io;
use std::io::{Read, Write};
use std::mem::{size_of, transmute};
use libc::{c_void};

pub struct Client {
    stream: TcpStream,
    mode: client_mode,
    op_callback: fn(msg: *const c_void) -> i32,
}

impl Client {
    pub fn new(host: &str, port: i32, mode: client_mode, op_callback: fn(msg: *const c_void) -> i32) -> Result<Self, io::Error> {
        let stream = TcpStream::connect(format!("{}:{}", host, port))?;
        if mode == client_mode::MODE_SYNC {
            stream.set_nodelay(true)?;
        }

        Ok(Client {stream, mode, op_callback})
    }

    pub fn send_init(&mut self, dsthash: u64) -> Result<(), io::Error> {
        let init = unsafe {transmute::<init_msg, [u8; size_of::<init_msg>()]>(init_msg {mode: self.mode, dsthash: dsthash})};

        self.stream.write_all(&init)
    }

    pub fn process_ops(&mut self) -> Result<(), io::Error> {
        let mut header_buf = [0; size_of::<op_msg>()];
        let mut rcv_buf = [0; 33 * 1024];
        loop {
            self.stream.read_exact(&mut header_buf)?;
            let msg = unsafe {transmute::<[u8; size_of::<op_msg>()], op_msg>(header_buf)};
            rcv_buf[..size_of::<op_msg>()].copy_from_slice(&header_buf);
            self.stream.read_exact(&mut rcv_buf[size_of::<op_msg>()..msg.op_length as usize])?;

            let res = (self.op_callback)(rcv_buf.as_ptr() as *const c_void);
            if self.mode == client_mode::MODE_SYNC {
                let ack = unsafe {transmute::<ack_msg, [u8; size_of::<ack_msg>()]>(ack_msg {retcode: res})};
                self.stream.write_all(&ack)?;
            }
        }
    }
}
