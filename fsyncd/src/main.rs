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

use std::process::exit;

use clap::{App, Arg, ArgGroup, ErrorKind};
use server::{server_main, display_fuse_help};
use client::client_main;

pub use client::client_path;
pub use server::{server_path, send_op};

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
        )
        .arg(
            Arg::with_name("client")
                .short("c")
                .long("client")
                .default_value("127.0.0.1:2323")
                .help("This daemon will act as a client and connect to this host")
                .group("mode")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("server")
                .long("server")
                .help("This daemon acts as a server")
                .group("mode"),
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
        .get_matches_from_safe(std::env::args().take_while(|v| v != "--"))
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
    } else {
        server_main(matches);
    }
}
