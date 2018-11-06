mod fusemain;
mod fuseops;
mod write;

use self::fusemain::fuse_main;
use bincode::{serialize, serialized_size};
use clap::ArgMatches;
use common::*;
use dssc::chunkmap::ChunkMap;
use dssc::other::ZstdBlock;
use dssc::Compressor;
use libc::{c_char, c_int};
use lz4;
use net2::TcpStreamExt;
use std;
use std::collections::HashMap;
use std::ffi::CString;
use std::fs;
use std::io;
use std::io::{Read, Write};
use std::mem::{size_of, transmute};
use std::net::{TcpListener, TcpStream};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr::null;
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread;
use std::time::Duration;
use zstd;

#[no_mangle]
#[allow(non_upper_case_globals)]
pub static mut server_path: *const c_char = null();

pub static mut SERVER_PATH_RUST: String = String::new();

lazy_static! {
    static ref SYNC_LIST: RwLock<Vec<Client>> = RwLock::new(Vec::new());
    static ref CORK_VAR: Condvar = Condvar::new();
    static ref CORK: Mutex<bool> = Mutex::new(false);
}

#[derive(PartialEq)]
enum ClientStatus {
    DEAD,
    ALIVE,
}

struct ClientNetwork {
    write: Box<Write + Send>,
    rt_comp: Option<Box<Compressor>>,
    parked: HashMap<u64, thread::Thread>,
    status: ClientStatus,
}

struct Client {
    id: String,
    mode: ClientMode,
    net: Arc<Mutex<ClientNetwork>>,
}

impl Client {
    fn cork(&mut self) -> Result<(), io::Error> {
        let _ret = self.send_msg(FsyncerMsg::Cork);
        self.flush()
    }

    fn uncork(&mut self) -> Result<(), io::Error> {
        let _ret = self.send_msg(FsyncerMsg::Uncork);
        self.flush()
    }

    fn reader<R: Read>(mut read: R, net: Arc<Mutex<ClientNetwork>>) {
        let mut ack_buf = [0; size_of::<AckMsg>()];
        let net = net.deref();
        loop {
            read.read_exact(&mut ack_buf)
                .expect("Failed to read message from client");
            let ack = unsafe { transmute::<[u8; size_of::<AckMsg>()], AckMsg>(ack_buf) };
            let mut netlock = net.lock().unwrap();
            netlock.parked.remove(&ack.tid).map(|t| t.unpark());
        }
    }

    fn flush(&self) -> Result<(), io::Error> {
        //HACK: Without the nop message compression algorithms dont flush immediately.
        self.send_msg(FsyncerMsg::NOP)?;
        self.net.lock().unwrap().write.flush()
    }

    fn send_msg(&self, msg_data: FsyncerMsg) -> Result<(), io::Error> {
        let mut size = serialized_size(&msg_data)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))? as usize;

        let serbuf = serialize(&msg_data).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        let mut net = self.net.lock().unwrap();
        let mut nbuf = Vec::new();

        let buf = if let Some(ref mut rt_comp) = net.rt_comp {
            rt_comp.encode(&serbuf[..], &mut nbuf);
            size = nbuf.len();
            &nbuf[..]
        } else {
            &serbuf[..]
        };

        let hbuf = unsafe { transmute::<u32, [u8; size_of::<u32>()]>(size as u32) };

        //println!("Sending {} {}", header.op_length, hbuf.len() + buf.len());

        // FIXME this is bad in synchronous mode (it will send the length in one packet then the buffer in another)
        net.write.write_all(&hbuf[..])?;
        net.write.write_all(&buf)?;

        Ok(())
    }

    fn park(&self, current_thread: thread::Thread) {
        let mut net = self.net.lock().unwrap();
        net.parked.insert(
            unsafe { transmute::<thread::ThreadId, u64>(current_thread.id()) },
            current_thread,
        );
        drop(net);
        thread::park();
    }
}

fn handle_client(
    mut stream: TcpStream,
    storage_path: PathBuf,
    dontcheck: bool,
    buffer_size: usize,
) -> Result<(), io::Error> {
    stream.set_send_buffer_size(buffer_size * 1024 * 1024)?;
    let mut init_buf = [0; size_of::<InitMsg>()];
    stream.read_exact(&mut init_buf)?;

    println!("Calculating source hash...");
    let srchash = hash_metadata(storage_path.to_str().unwrap()).expect("Hash check failed");
    println!("Source hash is {:x}", srchash);

    let init = unsafe { transmute::<[u8; size_of::<InitMsg>()], InitMsg>(init_buf) };

    if (!dontcheck) && init.dsthash != srchash {
        println!(
            "{:x} != {:x} client's hash does not match!",
            init.dsthash, srchash
        );
        println!("Dropping this client!");
        drop(stream);
        return Err(io::Error::new(io::ErrorKind::Other, "Hash mismatch"));
    }

    if init.mode == ClientMode::MODE_SYNC || init.mode == ClientMode::MODE_SEMISYNC {
        stream.set_nodelay(true)?;
    }

    let writer = if init.compress.contains(CompMode::STREAM_ZSTD) {
        Box::new(zstd::stream::Encoder::new(stream.try_clone()?, 0)?) as Box<Write + Send>
    } else if init.compress.contains(CompMode::STREAM_LZ4) {
        Box::new(lz4::EncoderBuilder::new().build(stream.try_clone()?)?) as Box<Write + Send>
    } else {
        Box::new(stream.try_clone()?) as Box<Write + Send>
    };

    let rt_comp: Option<Box<Compressor>> = if init.compress.contains(CompMode::RT_DSSC_CHUNKED) {
        Some(Box::new(ChunkMap::new(0.5)))
    } else if init.compress.contains(CompMode::RT_DSSC_ZSTD) {
        Some(Box::new(ZstdBlock::default()))
    } else {
        None
    };

    let net = Arc::new(Mutex::new(ClientNetwork {
        write: writer,
        rt_comp: rt_comp,
        parked: HashMap::new(),
        status: ClientStatus::ALIVE,
    }));
    let net_clone = net.clone();

    thread::spawn(move || Client::reader(stream, net_clone));

    SYNC_LIST.write().unwrap().push(Client {
        id: "".to_string(),
        mode: init.mode,
        net: net,
    });

    println!("Client connected!");

    Ok(())
}

fn flush_thread() {
    loop {
        let list = SYNC_LIST.read().expect("Failed to lock SYNC_LIST");
        for client in list.iter().filter(|c| c.mode == ClientMode::MODE_ASYNC) {
            if client.flush().is_err() {
                println!("Failed to flush to client");
            }
        }
        drop(list);
        thread::sleep(Duration::from_secs(1));
    }
}

pub fn handle_op(call: VFSCall) {
    let list = SYNC_LIST.read().expect("Failed to lock SYNC_LIST");

    for client in list.deref() {
        let res =
            if client.mode == ClientMode::MODE_SYNC || client.mode == ClientMode::MODE_SEMISYNC {
                let current_thread = thread::current();
                let tid = unsafe { transmute::<thread::ThreadId, u64>(current_thread.id()) };
                let res = client.send_msg(FsyncerMsg::SyncOp(call.clone(), tid));
                if res.is_ok() {
                    client.park(current_thread);
                }
                res
            } else {
                client.send_msg(FsyncerMsg::AsyncOp(call.clone()))
            };
        if res.is_err() {
            println!("Failed sending message to client {}", res.unwrap_err());
        }
    }

    /*
    let mut corked = CORK.lock().unwrap();
    if *corked {
        for client in list.into_iter() {
            if client.cork().is_err() || client.write.flush().is_err() {
                println!("Failed corking client");
                client.status = ClientStatus::DEAD;
            }
        }

        while *corked {
            corked = CORK_VAR.wait(corked).unwrap();
        }

        for client in list.into_iter().filter(|c| c.status != ClientStatus::DEAD) {
            if client.uncork().is_err() {
                println!("Failed uncorking client");
                client.status = ClientStatus::DEAD;
            }
        }
    }
    */
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

pub fn server_main(matches: ArgMatches) -> Result<(), io::Error> {
    let (mount_path, backing_store) = figure_out_paths(&matches)?;
    println!("{:?}, {:?}", mount_path, backing_store);

    let c_dst = CString::new(backing_store.to_str().unwrap()).unwrap();
    unsafe {
        server_path = c_dst.into_raw();
        SERVER_PATH_RUST = String::from(backing_store.to_str().unwrap());
    }

    // FIXME use net2::TcpBuilder to set SO_REUSEADDR
    let listener = TcpListener::bind(format!(
        "0.0.0.0:{}",
        matches
            .value_of("port")
            .map(|v| v.parse::<i32>().expect("Invalid format for port"))
            .unwrap()
    ))?;

    let dont_check = matches.is_present("dont-check");
    let buffer_size = matches
        .value_of("buffer")
        .and_then(|b| b.parse().ok())
        .expect("Buffer format incorrect");

    thread::spawn(move || {
        for stream in listener.incoming() {
            handle_client(
                stream.expect("Failed client connection"),
                backing_store.clone(),
                dont_check,
                buffer_size,
            ).expect("Failed handling client");
        }
    });

    thread::spawn(flush_thread);

    let args = vec![
        "fsyncd".to_string(),
        matches.value_of("mount-path").unwrap().to_string(),
    ].into_iter()
    .chain(std::env::args().skip_while(|v| v != "--").skip(1))
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
