#![feature(libc)]

#[macro_use]
extern crate lazy_static;
extern crate clap;
extern crate libc;
extern crate zstd;
extern crate net2;

mod server;
mod client;
mod common;

use clap::{App, Arg};
use server::server_main;
use client::client_main;

fn main() {
    let matches = App::new("Fsyncer Replication Daemon")
        .version("0.0")
        .author("Denis Lavrov <bahus.vel@gmail.com>")
        .about("Filesystem replication daemon")
        .arg(
            Arg::with_name("destination")
                .short("d")
                .long("destination")
                .help("Destination to replicate to")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("source")
                .short("s")
                .long("source")
                .help("Underlying storage to use by fs")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("host")
                .short("h")
                .long("host")
                .default_value("127.0.0.1")
                .help("Fsyncer host to connect to")
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
        .arg(Arg::with_name("dont-check").long("dont-check").help(
            "Disables comparison of the source and destination",
        ))
        .arg(Arg::with_name("sync").short("s").long("sync").help(
            "Performs replication synchronously",
        ))
        .get_matches_from(std::env::args().take_while(|v| v != "--"));
}
