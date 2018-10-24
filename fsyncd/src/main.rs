#![feature(libc)]
#![feature(conservative_impl_trait)]

#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate lazy_static;
extern crate clap;
extern crate dssc;
extern crate libc;
extern crate net2;
extern crate walkdir;
extern crate zstd;

mod client;
mod common;
mod server;

use std::process::exit;

use clap::{App, Arg, ArgGroup, ErrorKind};
use client::client_main;
use server::{display_fuse_help, server_main};

pub use client::client_path;
pub use server::{send_op, server_path};

fn main() {
    let matches = App::new("Fsyncer Replication Daemon")
        .version("0.1")
        .author("Denis Lavrov <bahus.vel@gmail.com>")
        .about("Filesystem replication daemon")
        .group(ArgGroup::with_name("mode").required(true))
        .arg(
            Arg::with_name("mount-path")
                .help("Mount path for the daemon")
                .required(true)
                .takes_value(true),
        ).arg(
            Arg::with_name("client")
                .long("client")
                .help("This daemon will act as a client and connect to this host")
                .group("mode")
                .takes_value(true),
        ).arg(
            Arg::with_name("server")
                .long("server")
                .help("This daemon acts as a server")
                .group("mode"),
        ).arg(
            Arg::with_name("checksum")
                .long("checksum")
                .help("This daemon acts as a server")
                .group("mode"),
        ).arg(
            Arg::with_name("backing-store")
                .short("b")
                .long("backing-store")
                .help("Explicitly specifies which directory server should use to store files")
                .takes_value(true)
                .requires("server"),
        ).arg(
            Arg::with_name("port")
                .short("p")
                .long("port")
                .default_value("2323")
                .help("Port the fsyncer is running on")
                .takes_value(true),
        ).arg(
            Arg::with_name("buffer")
                .long("buffer")
                .default_value("32")
                .help("TX/RX Buffer size in megabytes")
                .takes_value(true),
        ).arg(
            Arg::with_name("rt-compressor")
                .long("rt-compressor")
                .possible_values(&["default", "chunked", "zstd", "none"])
                .default_value_if("client", None, "none")
                .default_value_if("stream-compressor", Some("none"), "default")
                .help("Discrete compression method to use")
                .takes_value(true)
                .requires("client"),
        ).arg(
            Arg::with_name("stream-compressor")
                .long("stream-compressor")
                .possible_values(&["default", "none"])
                .default_value_if("sync", Some("sync"), "none")
                .default_value_if("sync", Some("semisync"), "none")
                .default_value_if("client", None, "default")
                .help("Stream compression method to use")
                .takes_value(true)
                .requires("client"),
        ).arg(
            Arg::with_name("dont-check")
                .long("dont-check")
                .help("Disables comparison of the source and destination")
                .requires("server"),
        ).arg(
            Arg::with_name("sync")
                .short("s")
                .long("sync")
                .possible_values(&["sync", "async", "semisync"])
                .default_value_if("client", None, "async")
                .help("Selects replication mode")
                .requires("client"),
        ).get_matches_from_safe(std::env::args().take_while(|v| v != "--"))
        .unwrap_or_else(|e| match e.kind {
            ErrorKind::HelpDisplayed => {
                eprintln!("{}", e);
                display_fuse_help();
                exit(1);
            }
            _ => e.exit(),
        });

    if matches.is_present("client") {
        client_main(matches);
    } else if matches.is_present("server") {
        server_main(matches).expect("Server error");
    } else if matches.is_present("checksum") {
        use common::hash_metadata;
        let hash = hash_metadata(
            matches
                .value_of("mount-path")
                .expect("No destination specified"),
        ).expect("Hash failed");
        println!("{:x}", hash);
    } else {
        unreachable!();
    }
}
