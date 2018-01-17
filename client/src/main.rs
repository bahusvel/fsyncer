#![feature(libc)]
extern crate clap;
extern crate client;
extern crate common;
extern crate libc;

use common::*;
use client::Client;
use clap::{App, Arg};
use libc::{c_void, c_char};
use std::ptr::null;
use std::ffi::CString;
use libc::{perror, puts};

#[link(name = "fsyncer_client", kind = "static")]
extern "C" {
    fn do_call(message: *const c_void) -> i32;
}

fn do_call_wrapper(message: *const c_void) -> i32 {
    //println!("Received call");
    let res = unsafe { do_call(message) };
    if res < 0 {
        unsafe { perror(CString::new("Error in replay").unwrap().as_ptr()) };
    }
    res
}

#[no_mangle]
#[allow(non_upper_case_globals)]
pub static mut dst_path: *const c_char = null();

fn main() {
    let matches = App::new("Fsyncer MirrorFS Client")
        .version("0.0")
        .author("Denis Lavrov <bahus.vel@gmail.com>")
        .about("Connects to fsyncer and mirrors the FS operations")
        .arg(
            Arg::with_name("host")
                .short("h")
                .long("host")
                .default_value("127.0.0.1")
                .help("Fsyncer host to connection to")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("port")
                .short("p")
                .long("port")
                .default_value("2323")
                .help("Port the fsyncer is running on")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("destination")
                .short("d")
                .long("destination")
                .help("Destination to replicate to")
                .required(true)
                .takes_value(true),
        )
        .arg(Arg::with_name("sync").short("s").long("sync").help(
            "Performs replication synchronously",
        ))
        .get_matches();

    println!("Calculating destination hash...");
    let dsthash = hash_mdata(matches.value_of("destination").expect(
        "No destination specified",
    ));
    println!("Destinaton hash is {:x}", dsthash);

    let mode = if matches.is_present("sync") {
        client_mode::MODE_SYNC
    } else {
        client_mode::MODE_ASYNC
    };

    let c_dst = CString::new(matches.value_of("destination").expect(
        "Destination not specified",
    ));
    unsafe {
        dst_path = c_dst.unwrap().into_raw();
    }

    let mut client = Client::new(
        matches.value_of("host").expect("No host specified"),
        matches
            .value_of("port")
            .map(|v| v.parse().expect("Invalid format for port"))
            .unwrap(),
        mode,
        dsthash,
        true,
        do_call_wrapper,
    ).expect("Failed to connect to fsyncer");


    println!(
        "Connected to {}",
        matches.value_of("host").expect("No host specified")
    );

    client.process_ops().expect("Stopped processing ops!");
}
