#![feature(libc)]
#![feature(const_string_new)]
#![feature(test)]
#![feature(concat_idents)]

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
extern crate either;
extern crate errno;
extern crate libc;
extern crate lz4;
extern crate net2;
extern crate regex;
extern crate serde;
extern crate url;
extern crate walkdir;
extern crate zstd;

#[macro_use]
pub mod error;

#[macro_export]
macro_rules! iter_try {
    ($e:expr) => {
        match $e {
            Err(e) => return Some(Err(trace_err!(e))),
            Ok(e) => e,
        }
    };
}

#[macro_export]
macro_rules! flagset {
    ($val:expr, $flag:expr) => {
        $val & $flag == $flag
    };
}

#[macro_export]
macro_rules! debug {
    ($($e:expr),+) => {
        #[cfg(debug_assertions)]
        #[allow(unused_unsafe)]
        {
            if unsafe {::DEBUG } {
                $(
                    print!(concat!(stringify!($e), "={:?} "), $e);
                )*
                eprintln!();
            }
        }
    }
}

#[macro_export]
macro_rules! debugif {
    ($c:expr, $($e:expr),+) => {
        if $c {
            debug!($($e),*)
        }
    };
}

#[macro_export]
macro_rules! is_variant {
    ($val:expr, $variant:path) => {
        if let $variant(..) = $val {
            true
        } else {
            false
        }
    };
    ($val:expr, $variant:path, struct) => {
        if let $variant { .. } = $val {
            true
        } else {
            false
        }
    };
}

#[macro_export]
macro_rules! metablock {
    ($meta:meta { $($list:item)* } ) => {
        $(
            #[$meta] $list
        )*
    };
}

mod client;
mod common;
mod server;

use std::process::exit;

use clap::{App, AppSettings, Arg, ArgGroup, ErrorKind, SubCommand};
use client::{client_main, ConnectionBuilder};
use common::{parse_human_size, ClientMode, CompMode, InitMsg, Options};
use server::server_main;
use std::path::Path;

metablock!(cfg(target_family = "unix") {
    use server::display_fuse_help;
    use journal::viewer_main;
    mod journal;
});

metablock!(cfg(target_os = "windows") {
    extern crate winapi;
    pub use server::write_windows::*;
    pub use server::win_translate_path;
});

#[cfg(feature = "profile")]
extern "C" fn stop_profiler(_: i32) {
    use cpuprofiler::PROFILER;
    PROFILER.lock().unwrap().stop().unwrap();
    eprintln!("Stopped profiler");
    exit(0);
}

#[cfg(feature = "profile")]
fn start_profiler() {
    use cpuprofiler::PROFILER;
    use nix::sys::signal;
    PROFILER.lock().unwrap().start("./fsyncd.profile").unwrap();

    eprintln!("Started profiler");

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

pub static mut DEBUG: bool = false;

const VERSION: &str = env!("VERSION");

fn main() {
    let server = SubCommand::with_name("server")
        .arg(
            Arg::with_name("mount-path")
                .help("Mount path for the daemon")
                .required(true)
                .takes_value(true),
        ).arg(
            Arg::with_name("journal")
                .long("journal")
                .takes_value(true)
                .default_value("off")
                .possible_values(&["bilog", "off"]),
        ).arg(Arg::with_name("journal-sync").long("journal-sync"))
        .arg(
            Arg::with_name("journal-path")
                .long("journal-path")
                .takes_value(true)
                .default_value("test.fj")
                .required_ifs(&[("bilog", "journal"), ("undo", "journal")]),
        ).arg(
            Arg::with_name("journal-size")
                .long("journal-size")
                .takes_value(true)
                .default_value("1024M"),
        ).arg(
            Arg::with_name("dont-check")
                .long("dont-check")
                .help("Disables comparison of the source and destination"),
        ).arg(
            Arg::with_name("backing-store")
                .short("b")
                .long("backing-store")
                .help(
                    "Explicitly specifies which directory server should use \
                     to store files",
                ).takes_value(true),
        ).arg(
            Arg::with_name("diff-writes")
                .long("diff-writes")
                .help("Performs delta compression on overlapping writes"),
        ).arg(
            Arg::with_name("flush-interval")
                .long("flush-interval")
                .default_value("1")
                .help(
                    "Sets the interval in seconds for periodic flush for \
                     synchronous clients, 0 disables flushing altogether",
                ).takes_value(true),
        );

    let client = SubCommand::with_name("client")
        .arg(
            Arg::with_name("mount-path")
                .help("Mount path for the daemon")
                .required(true)
                .takes_value(true),
        ).arg(
            Arg::with_name("rt-compressor")
                .long("rt-compressor")
                .possible_values(&["default", "chunked", "zstd", "none"])
                .default_value("none")
                .help("Discrete compression method to use")
                .takes_value(true),
        ).arg(
            Arg::with_name("stream-compressor")
                .long("stream-compressor")
                .possible_values(&["default", "zstd", "lz4", "none"])
                .default_value_if("sync", Some("sync"), "none")
                .default_value_if("sync", Some("semi"), "none")
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
        ).arg(
            Arg::with_name("threads")
                .short("t")
                .long("threads")
                .takes_value(true)
                .default_value("1")
                .help("Sets number of dispatch threads"),
        ).arg(Arg::with_name("rsync").long("rsync").help(
            "Do initial replication using rsync, NOTE: rsync must be present \
             in path",
        )).arg(
            Arg::with_name("iolimit")
                .long("iolimit")
                .help("Restricts network transmission, 0 means unlimited")
                .short("l")
                .takes_value(true)
                .default_value("0"),
        );
    #[cfg(target_os = "windows")]
    let server =
        server.arg(Arg::with_name("send-sids").long("send-sids").help(
            "Send SIDs instead of usernames when replicating, NOTE SIDs on \
             source and destination must match,adjust them manually or use a \
             domain controller",
        ));

    let matches = App::new("Fsyncer Replication Daemon")
        .version(VERSION)
        .author("Denis Lavrov <bahus.vel@gmail.com>")
        .about("Filesystem replication daemon")
        .arg(
            Arg::with_name("url")
                .required(true)
                .takes_value(true)
                .default_value("tcp://localhost:2323")
                .help(
                    "Can be tcp://<host>:<port>, unix:<path>, stdio:, server \
                     binds on this address, client connects",
                ),
        ).arg(
            Arg::with_name("buffer")
                .long("buffer")
                .default_value("1M")
                .help("TX/RX buffer size")
                .takes_value(true),
        ).arg(
            Arg::with_name("debug")
                .long("debug")
                .help("Enables debug output"),
        ).subcommand(client)
        .subcommand(server)
        .subcommand(
            SubCommand::with_name("journal")
                .subcommand(
                    SubCommand::with_name("view")
                        .arg(Arg::with_name("verbose").long("verbose")),
                ).subcommand(
                    SubCommand::with_name("replay").arg(
                        Arg::with_name("backing-store")
                            .short("b")
                            .long("backing-store")
                            .help(
                                "Explicitly specifies which directory server \
                                 should use to store files",
                            ).takes_value(true)
                            .required(true),
                    ),
                ).arg(
                    Arg::with_name("journal-path")
                        .long("journal-path")
                        .short("j")
                        .takes_value(true)
                        .default_value("test.fj")
                        .required(true),
                ).arg(Arg::with_name("reverse").long("reverse").short("r"))
                .arg(
                    Arg::with_name("filter")
                        .long("filter")
                        .short("f")
                        .takes_value(true),
                ).arg(
                    Arg::with_name("inverse-filter").long("inverse").short("i"),
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
        ).subcommand(
            SubCommand::with_name("fakeshell")
                .arg(Arg::with_name("netin").required(true).takes_value(true))
                .arg(Arg::with_name("netout").required(true).takes_value(true))
                .arg(
                    Arg::with_name("extra-args")
                        .takes_value(true)
                        .multiple(true),
                ).settings(&[AppSettings::TrailingVarArg, AppSettings::Hidden]),
        ).get_matches_from_safe(std::env::args().take_while(|v| v != "--"))
        .unwrap_or_else(|e| match e.kind {
            ErrorKind::HelpDisplayed => {
                eprintln!("{}", e);
                #[cfg(target_family = "unix")]
                display_fuse_help();
                exit(1);
            }
            _ => e.exit(),
        });

    if matches.is_present("debug") {
        unsafe { DEBUG = true };
    }

    #[cfg(feature = "profile")]
    start_profiler();

    match matches.subcommand_name() {
        Some("server") => {
            server_main(matches).expect("Server error");
        }
        Some("client") => {
            client_main(matches);
        }
        #[cfg(target_family = "unix")]
        Some("journal") => {
            viewer_main(matches);
        }
        Some("checksum") => {
            use common::hash_metadata;
            let matches = matches.subcommand_matches("checksum").unwrap();
            let hash = hash_metadata(Path::new(
                matches
                    .value_of("mount-path")
                    .expect("No destination specified"),
            )).expect("Hash failed");
            eprintln!("{:x}", hash);
        }
        Some("control") => {
            use url::Url;
            let control_matches =
                matches.subcommand_matches("control").unwrap();
            let buffer_size =
                parse_human_size(matches.value_of("buffer").unwrap())
                    .expect("Buffer size format incorrect");
            let url = Url::parse(matches.value_of("url").unwrap())
                .expect("Invalid url specified");

            eprintln!("cmd {:?}", control_matches.value_of("cmd"));

            let mut client = ConnectionBuilder::with_url(
                &url,
                true,
                buffer_size,
                InitMsg {
                    mode: ClientMode::MODE_CONTROL,
                    compress: CompMode::empty(),
                    dsthash: 0,
                    iolimit_bps: 0,
                    options: Options::empty(),
                },
            ).expect("Failed to initialize client")
            .build()
            .expect("Failed to create server connection");

            match control_matches.value_of("cmd").unwrap() {
                "cork" => {
                    eprintln!("Corking");
                    client.cork_server()
                }
                "uncork" => {
                    eprintln!("Uncorking");
                    client.uncork_server()
                }
                _ => unreachable!(),
            }.expect("Failed to execute command server");
        }
        Some("fakeshell") => {
            use common::rsync::rsync_bridge;
            use std::fs::File;
            use std::os::unix::io::FromRawFd;
            let matches = matches.subcommand_matches("fakeshell").unwrap();
            let netin = matches
                .value_of("netin")
                .map(|v| v.parse().expect("Invalid integer"))
                .unwrap();
            let netout = matches
                .value_of("netout")
                .map(|v| v.parse().expect("Invalid integer"))
                .unwrap();
            unsafe {
                rsync_bridge(
                    File::from_raw_fd(netin),
                    File::from_raw_fd(netout),
                    File::from_raw_fd(1),
                    File::from_raw_fd(0),
                    true,
                ).unwrap();
            };
        }
        _ => panic!("you must provide a command"),
    }
}
