#![feature(libc)]
#![feature(const_string_new)]

#[cfg(feature = "profile")]
extern crate cpuprofiler;
#[cfg(feature = "profile")]
extern crate nix;

#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_derive;
extern crate bincode;
extern crate byteorder;
extern crate clap;
extern crate dssc;
extern crate errno;
extern crate libc;
extern crate lz4;
extern crate net2;
extern crate serde;
extern crate walkdir;
extern crate zstd;

mod client;
mod common;
mod server;

use std::process::exit;

use clap::{App, Arg, ArgGroup, ErrorKind, SubCommand};
use client::client_main;
use client::Client;
use common::{ClientMode, CompMode};
use server::{display_fuse_help, server_main};

#[cfg(feature = "profile")]
extern "C" fn stop_profiler(_: i32) {
    use cpuprofiler::PROFILER;
    PROFILER.lock().unwrap().stop().unwrap();
    println!("Stopped profiler");
    exit(0);
}

#[cfg(feature = "profile")]
fn start_profiler() {
    use cpuprofiler::PROFILER;
    use nix::sys::signal;
    PROFILER.lock().unwrap().start("./fsyncd.profile").unwrap();

    println!("Started profiler");

    let sig_action = signal::SigAction::new(
        signal::SigHandler::Handler(stop_profiler),
        signal::SaFlags::empty(),
        signal::SigSet::empty(),
    );
    unsafe {
        signal::sigaction(signal::SIGINT, &sig_action)
            .expect("Failed to declare signal handler for profiling")
    };
}

include!(concat!(env!("OUT_DIR"), "/version.rs"));

fn main() {
    let matches = App::new("Fsyncer Replication Daemon")
        .version(VERSION)
        .author("Denis Lavrov <bahus.vel@gmail.com>")
        .about("Filesystem replication daemon")
        .arg(
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
        ).subcommand(
            SubCommand::with_name("client")
                .arg(
                    Arg::with_name("mount-path")
                        .help("Mount path for the daemon")
                        .required(true)
                        .takes_value(true),
                ).arg(
                    Arg::with_name("host")
                        .required(true)
                        .takes_value(true)
                        .default_value("localhost"),
                ).arg(
                    Arg::with_name("rt-compressor")
                        .long("rt-compressor")
                        .possible_values(&["default", "chunked", "zstd", "none"])
                        .default_value("none")
                        .default_value_if("stream-compressor", Some("none"), "default")
                        .help("Discrete compression method to use")
                        .takes_value(true),
                ).arg(
                    Arg::with_name("stream-compressor")
                        .long("stream-compressor")
                        .possible_values(&["default", "zstd", "lz4", "none"])
                        .default_value_if("sync", Some("sync"), "none")
                        .default_value_if("sync", Some("semisync"), "none")
                        .default_value("default")
                        .help("Stream compression method to use")
                        .takes_value(true),
                ).arg(
                    Arg::with_name("sync")
                        .short("s")
                        .long("sync")
                        .possible_values(&["sync", "async", "semi", "flush"])
                        .default_value("async")
                        .help("Selects replication mode"),
                ),
        ).subcommand(
            SubCommand::with_name("server")
                .arg(
                    Arg::with_name("mount-path")
                        .help("Mount path for the daemon")
                        .required(true)
                        .takes_value(true),
                ).arg(
                    Arg::with_name("dont-check")
                        .long("dont-check")
                        .help("Disables comparison of the source and destination"),
                ).arg(
                    Arg::with_name("backing-store")
                        .short("b")
                        .long("backing-store")
                        .help(
                            "Explicitly specifies which directory server should use to store files",
                        ).takes_value(true),
                ).arg(Arg::with_name("flush-interval")
                        .long("flush-interval")
                        .default_value("1")
                        .help("Sets the interval in seconds for periodic flush for synchronous clients, 0 disables flushing altogether")
                        .takes_value(true)
                ),
        ).subcommand(
            SubCommand::with_name("checksum").arg(
                Arg::with_name("mount-path")
                    .help("Mount path for the daemon")
                    .required(true)
                    .takes_value(true),
            ),
        ).subcommand(
            SubCommand::with_name("control")
                .group(ArgGroup::with_name("cmd").required(true))
                .arg(
                    Arg::with_name("host")
                        .required(true)
                        .takes_value(true)
                        .default_value("localhost"),
                ).arg(Arg::with_name("cork").group("cmd"))
                .arg(Arg::with_name("uncork").group("cmd")),
        ).get_matches_from_safe(std::env::args().take_while(|v| v != "--"))
        .unwrap_or_else(|e| match e.kind {
            ErrorKind::HelpDisplayed => {
                eprintln!("{}", e);
                display_fuse_help();
                exit(1);
            }
            _ => e.exit(),
        });

    #[cfg(feature = "profile")]
    start_profiler();

    match matches.subcommand_name() {
        Some("server") => {
            server_main(matches).expect("Server error");
        }
        Some("client") => {
            client_main(matches);
        }
        Some("checksum") => {
            use common::hash_metadata;
            let matches = matches.subcommand_matches("checksum").unwrap();
            let hash = hash_metadata(
                matches
                    .value_of("mount-path")
                    .expect("No destination specified"),
            ).expect("Hash failed");
            println!("{:x}", hash);
        }
        Some("control") => {
            let control_matches = matches.subcommand_matches("control").unwrap();
            let host = control_matches.value_of("host").expect("No host specified");
            let port = matches
                .value_of("port")
                .map(|v| v.parse().expect("Invalid format for port"))
                .unwrap();
            let buffer = matches
                .value_of("buffer")
                .and_then(|b| b.parse().ok())
                .expect("Buffer format incorrect");

            println!("cmd {:?}", control_matches.value_of("cmd"));

            let mut client = Client::new(
                host,
                port,
                ClientMode::MODE_CONTROL,
                0,
                CompMode::empty(),
                buffer,
                |_| 0,
            ).expect("Failed to initialize client");

            match control_matches.value_of("cmd").unwrap() {
                "cork" => {
                    println!("Corking");
                    client.cork_server()
                }
                "uncork" => {
                    println!("Uncorking");
                    client.uncork_server()
                }
                _ => unreachable!(),
            }.expect("Failed to execute command server");
        }
        _ => unreachable!(),
    }
}
