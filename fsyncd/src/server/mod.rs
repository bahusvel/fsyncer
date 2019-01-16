macro_rules! trans_ppath {
    ($path:expr) => {
        translate_path(CStr::from_ptr($path), &SERVER_PATH)
    };
}

mod client;
mod fusemain;
mod fuseops;
mod read;
mod write;

use self::client::{Client, ClientStatus};
use self::fusemain::fuse_main;
use clap::ArgMatches;
use common::*;
use journal::{BilogEntry, Journal, JournalEntry};

use libc::{c_char, c_int};
use std::fs::{self, OpenOptions};
use std::io;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, RwLock};
use std::{
    borrow::Cow, env, ffi::CString, mem::transmute, ops::Deref, process::Command, thread,
    time::Duration,
};

pub static mut SERVER_PATH: String = String::new();
static mut JOURNAL: Option<Mutex<Journal>> = None;

lazy_static! {
    static ref SYNC_LIST: RwLock<Vec<Client>> = RwLock::new(Vec::new());
    static ref CORK_VAR: Condvar = Condvar::new();
    static ref CORK: Mutex<bool> = Mutex::new(false);
}

fn flush_thread(interval: u64) {
    loop {
        let list = SYNC_LIST.read().expect("Failed to lock SYNC_LIST");
        for client in list.iter().filter(|c| c.mode == ClientMode::MODE_ASYNC) {
            if client.flush().is_err() {
                println!("Failed to flush to client");
            }
        }
        drop(list);
        thread::sleep(Duration::from_secs(interval));
    }
}

fn harvester_thread() {
    loop {
        // Check to see if there are dead nodes (without exclusive lock)
        let have_dead_nodes = SYNC_LIST
            .read()
            .unwrap()
            .iter()
            .filter(|c| c.status() == ClientStatus::DEAD)
            .count()
            != 0;
        // if there are, obtain exclusive lock and remove them
        if have_dead_nodes {
            SYNC_LIST
                .write()
                .unwrap()
                .retain(|c| c.status() != ClientStatus::DEAD);
        }
        thread::sleep(Duration::from_secs(1));
    }
}

pub fn cork_server() {
    println!("Corking");
    *CORK.lock().unwrap() = true;
    // Cork the individual clients
    let list = SYNC_LIST.read().expect("Failed to lock SYNC_LIST");
    for client in list.deref() {
        if let Err(e) = client.cork() {
            println!("Failed to cork client {}", e);
        }
    }
    println!("Cork done");
}

pub fn uncork_server() {
    println!("Uncorking");
    let list = SYNC_LIST.read().expect("Failed to lock SYNC_LIST");
    for client in list.deref() {
        if let Err(e) = client.uncork() {
            println!("Failed to uncork client {}", e);
        }
    }
    drop(list);
    *CORK.lock().unwrap() = false;
    CORK_VAR.notify_all();
    println!("Uncork done");
}

macro_rules! is_variant {
    ($val:expr, $variant:path) => {
        if let $variant(_) = $val {
            true
        } else {
            false
        }
    };
}

fn send_call<'a>(call: Cow<'a, VFSCall<'a>>, client: &Client, ret: i32) -> Result<(), io::Error> {
    match client.mode {
        ClientMode::MODE_SYNC | ClientMode::MODE_SEMISYNC | ClientMode::MODE_FLUSHSYNC => {
            // In flushsync mode all ops except for fsync are sent async
            if client.mode == ClientMode::MODE_FLUSHSYNC && !is_variant!(&*call, VFSCall::fsync) {
                return client.send_msg(FsyncerMsg::AsyncOp(call), false);
            }

            let tid = unsafe { transmute::<thread::ThreadId, u64>(thread::current().id()) };
            let client_ret = client
                .send_msg(FsyncerMsg::SyncOp(call, tid), true)
                .map(|_| client.wait_thread_response())?;

            if client.mode == ClientMode::MODE_SYNC && client_ret != ret {
                println!(
                    "Response from client {} does not match server {}",
                    client_ret, ret
                );
            }
            Ok(())
        }
        ClientMode::MODE_ASYNC => client.send_msg(FsyncerMsg::AsyncOp(call), false),
        ClientMode::MODE_CONTROL => Ok(()), // Don't send control anything
    }
}

pub fn pre_op(call: &VFSCall) {
    // This is safe, journal is only initialized once.
    if unsafe { JOURNAL.is_none() } {
        return;
    }
    let bilog = BilogEntry::from_vfscall(call, unsafe { &SERVER_PATH })
        .expect("Failed to generate journal entry from vfscall");
    {
        // Reduce the time journal lock is held
        let mut j = unsafe { JOURNAL.as_ref().unwrap() }.lock().unwrap();
        j.write_entry(bilog).expect("Failed to write journal entry");
    }
}

pub fn post_op(call: &VFSCall, ret: i32) -> i32 {
    let mut corked = CORK.lock().unwrap();
    while *corked {
        corked = CORK_VAR.wait(corked).unwrap();
    }

    let list = SYNC_LIST.read().expect("Failed to lock SYNC_LIST");
    for client in list.deref() {
        let res = send_call(Cow::Borrowed(call), client, ret);
        if res.is_err() {
            println!("Failed sending message to client {}", res.unwrap_err());
        }
    }
    ret
    /* Cork lock is held until here, it is used to make sure that any pending operations get sent over the network, the flush operation will force them to the other side */
}

pub fn display_fuse_help() {
    println!("Fuse options, specify at the end, after --:");
    let args = vec!["fsyncd", "--help"]
        .into_iter()
        .map(|arg| CString::new(arg).unwrap())
        .collect::<Vec<CString>>();
    // convert the strings to raw pointers
    let c_args = args
        .iter()
        .map(|arg| arg.as_ptr())
        .collect::<Vec<*const c_char>>();

    unsafe { fuse_main(c_args.len() as c_int, c_args.as_ptr()) };
}

fn check_mount(path: &str) -> Result<bool, io::Error> {
    Ok(Command::new("mountpoint")
        .arg(path)
        .spawn()?
        .wait()?
        .success())
}

fn figure_out_paths(matches: &ArgMatches) -> Result<(PathBuf, PathBuf), io::Error> {
    let mount_path = Path::new(matches.value_of("mount-path").unwrap()).canonicalize()?;

    if matches.is_present("backing-store")
        && !Path::new(matches.value_of("backing-store").unwrap()).exists()
    {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Backing path does not exist",
        ));
    }

    let backing_store = if matches.is_present("backing-store") {
        PathBuf::from(matches.value_of("backing-store").unwrap()).canonicalize()?
    } else {
        mount_path.with_file_name(format!(
            ".fsyncer-{}",
            mount_path
                .file_name()
                .expect("You specified a weird file path")
                .to_str()
                .unwrap()
        ))
    };

    if !backing_store.exists() && mount_path.exists() {
        if check_mount(mount_path.to_str().unwrap())? {
            fs::create_dir_all(&mount_path)?;
            let res = Command::new("mount")
                .arg("--move")
                .arg(matches.value_of("mount-path").unwrap())
                .arg(backing_store.to_str().unwrap())
                .spawn()?
                .wait()?;
            if !res.success() {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Failed to move old mountpoint",
                ));
            }
        } else {
            fs::rename(&mount_path, &backing_store)?;
        }
    }

    if backing_store.exists() && !mount_path.exists() {
        fs::create_dir_all(&mount_path)?;
    } else if !backing_store.exists() && !mount_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Mount path does not exist",
        ));
    }

    Ok((mount_path, backing_store))
}

fn open_journal(path: &str, size: u64, sync: bool) -> Result<Journal, io::Error> {
    let exists = Path::new(path).exists();
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)?;
    if exists {
        if f.metadata()
            .expect("Failed to retrieve file metadata")
            .len()
            < size
        {
            panic!("Refusing to shrink journal size")
        }
        f.set_len(size)?;
        Journal::open(f, sync)
    } else {
        f.set_len(size)?;
        Journal::new(f, sync)
    }
}

pub fn server_main(matches: ArgMatches) -> Result<(), io::Error> {
    let server_matches = matches.subcommand_matches("server").unwrap();
    let (mount_path, backing_store) = figure_out_paths(&server_matches)?;
    println!("{:?}, {:?}", mount_path, backing_store);
    unsafe {
        SERVER_PATH = String::from(backing_store.to_str().unwrap());
    }

    // FIXME use net2::TcpBuilder to set SO_REUSEADDR
    let listener = TcpListener::bind(format!(
        "0.0.0.0:{}",
        matches
            .value_of("port")
            .map(|v| v.parse::<i32>().expect("Invalid format for port"))
            .unwrap()
    ))?;

    let dont_check = server_matches.is_present("dont-check");
    let buffer_size = matches
        .value_of("buffer")
        .and_then(|b| b.parse().ok())
        .expect("Buffer format incorrect");

    thread::spawn(move || {
        for stream in listener.incoming() {
            let client = Client::from_stream(
                stream.expect("Failed client connection"),
                backing_store.clone(),
                dont_check,
                buffer_size,
            )
            .expect("Failed handling client");
            SYNC_LIST.write().unwrap().push(client);
        }
    });

    let interval = server_matches
        .value_of("flush-interval")
        .map(|v| v.parse::<u64>().expect("Invalid format for flush interval"))
        .unwrap();

    if interval != 0 {
        thread::spawn(move || flush_thread(interval));
    }

    thread::spawn(harvester_thread);

    let journal_size = parse_human_size(server_matches.value_of("journal-size").unwrap())
        .expect("Invalid format for journal-size");
    let journal_sync = server_matches.is_present("journal-sync");

    match server_matches.value_of("journal").unwrap() {
        "bilog" => unsafe {
            let journal_path = server_matches
                .value_of("journal-path")
                .expect("Journal path must be set in bilog mode");
            JOURNAL = Some(Mutex::new(
                open_journal(journal_path, journal_size as u64, journal_sync)
                    .expect("Failed to open journal"),
            ))
        },
        "off" => {}
        _ => panic!("Unknown journal type"),
    }

    // Fuse args parsing
    let args = vec![
        "fsyncd".to_string(),
        server_matches.value_of("mount-path").unwrap().to_string(),
    ]
    .into_iter()
    .chain(env::args().skip_while(|v| v != "--").skip(1))
    .map(|arg| CString::new(arg).unwrap())
    .collect::<Vec<CString>>();
    // convert the strings to raw pointers
    let c_args = args
        .iter()
        .map(|arg| arg.as_ptr())
        .collect::<Vec<*const c_char>>();

    unsafe { fuse_main(c_args.len() as c_int, c_args.as_ptr()) };

    Ok(())
}
