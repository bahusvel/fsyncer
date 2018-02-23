extern crate futures;

use clap::ArgMatches;
use common::*;
use libc::{c_char, c_void, free};
use net2::TcpStreamExt;
use std;
use std::collections::HashMap;
use std::ffi::CString;
use std::io;
use std::io::{Read, Write};
use std::mem::{size_of, transmute};
use std::net::{TcpListener, TcpStream};
use std::ops::{DerefMut, Deref};
use std::process::Command;
use std::ptr::null;
use std::ptr;
use std::hash::{Hasher, Hash};
use std::slice;
use std::sync::{Mutex, Condvar, Arc};
use std::thread;
use std::time::Duration;
use zstd;
use std::path::{Path, PathBuf};
use std::fs;
use dssc::Compressor;
use dssc::chunkmap::ChunkMap;
use dssc::other::ZstdBlock;
use self::futures::Future;

struct NoHash(u64);

impl Hasher for NoHash {
    fn write(&mut self, bytes: &[u8]) {
        if bytes.len() > 8 {
            panic!("Input too big");
        }
        unsafe { ptr::copy(bytes.as_ptr(), &mut self.0 as *mut u64 as *mut u8, 0) }
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

#[link(name = "fsyncer", kind = "static")]
#[link(name = "fuse3")]
extern "C" {
    fn fsyncer_fuse_main(
        argc: i32,
        argv: *const *const c_char,
        sop: extern "C" fn(*const c_void, i32) -> i32,
    ) -> i32;
    fn encode_create(path: *const c_char, mode: u32, flags: u32) -> *const c_void;
    fn encode_unlink(path: *const c_char) -> *const c_void;
}

#[no_mangle]
#[allow(non_upper_case_globals)]
pub static mut server_path: *const c_char = null();

lazy_static!{
    static ref SYNC_LIST: Mutex<Vec<Client>> = Mutex::new(Vec::new());
    static ref CORK_VAR: Condvar = Condvar::new();
    static ref CORK: Mutex<bool> = Mutex::new(false);

    static ref CORK_FILE: CString = CString::new(".fsyncer-corked").unwrap();
}


#[derive(PartialEq)]
enum ClientStatus {
    DEAD,
    ALIVE,
}

struct Client {
    write: Box<Write + Send>,
    mode: client_mode,
    rt_comp: Option<Box<Compressor>>,
    status: ClientStatus,
    parked: Arc<Mutex<HashMap<u64, thread::Thread>>>,
}

struct Msg {
    tid: thread::Thread,
    op_type: op_type,
    data: Vec<u8>,
}

impl Client {
    /*
    fn send_msg(&mut self, msg_data: *const c_void) -> Result<Option<i32>, io::Error> {
        let msg = unsafe { &*(msg_data as *const op_msg) };
        let buf = unsafe { slice::from_raw_parts(msg_data as *const u8, msg.op_length as usize) };
        if let Some(ref mut rt_comp) = self.rt_comp {
            let mut nbuf = Vec::new();
            nbuf.extend_from_slice(&buf[..size_of::<op_msg>()]);
            rt_comp.encode(&buf[size_of::<op_msg>()..], &mut nbuf);
            let m = unsafe { &mut *(nbuf.as_mut_ptr() as *mut op_msg) };
            m.op_length = nbuf.len() as u32;
            self.write.write_all(&nbuf)?;
        } else {
            self.write.write_all(&buf)?;
        }

        if self.mode == client_mode::MODE_SYNC || self.mode == client_mode::MODE_SEMISYNC {
            let mut ack_buf = [0; size_of::<ack_msg>()];
            self.read.read_exact(&mut ack_buf)?;
            let ack = unsafe { transmute::<[u8; size_of::<ack_msg>()], ack_msg>(ack_buf) };
            return Ok(Some(ack.retcode));
        }

        Ok(None)
    }
    */

    fn cork(&mut self) -> Result<(), io::Error> {
        let msg = unsafe { encode_create(CORK_FILE.as_ptr(), 0o755, 0) };
        let ret = self.send_msg(msg);
        unsafe { free(msg as *mut c_void) };
        ret.map(|_| ())
    }

    fn uncork(&mut self) -> Result<(), io::Error> {
        let msg = unsafe { encode_unlink(CORK_FILE.as_ptr()) };
        let ret = self.send_msg(msg);
        unsafe { free(msg as *mut c_void) };
        ret.map(|_| ())
    }

    fn reader<R: Read>(mut read: R, parked: Arc<Mutex<HashMap<u64, thread::Thread>>>) {
        let mut ack_buf = [0; size_of::<ack_msg>()];
        let parked = parked.deref();
        loop {
            read.read_exact(&mut ack_buf);
            let ack = unsafe { transmute::<[u8; size_of::<ack_msg>()], ack_msg>(ack_buf) };
            parked
                .lock()
                .unwrap()
                .get(&ack.tid)
                .expect("Invalid TID")
                .unpark()
        }


    }

    fn send_msg(&mut self, msg_data: *const c_void) -> Result<(), io::Error> {
        let mut header = unsafe { *(msg_data as *const op_msg) };
        let mut nbuf = Vec::with_capacity(0);
        let mut buf =
            unsafe { slice::from_raw_parts(msg_data as *const u8, header.op_length as usize) };
        buf = &buf[size_of::<op_msg>()..];
        let current_thread = thread::current();

        header.tid =
            if self.mode == client_mode::MODE_SEMISYNC || self.mode == client_mode::MODE_SYNC {
                unsafe { transmute::<thread::ThreadId, u64>(current_thread.id()) }
            } else {
                0
            };

        if let Some(ref mut rt_comp) = self.rt_comp {
            rt_comp.encode(buf, &mut nbuf);
            header.op_length = (size_of::<op_msg>() + nbuf.len()) as u32;
            buf = &nbuf[..];
        }

        let hbuf = unsafe {
            slice::from_raw_parts(&header as *const op_msg as *const u8, size_of::<op_msg>())
        };
        self.write.write_all(&hbuf);
        self.write.write_all(&buf)?;

        if self.mode == client_mode::MODE_SEMISYNC || self.mode == client_mode::MODE_SYNC {
            let current_thread = thread::current();
            self.parked.lock().unwrap().insert(
                header.tid,
                current_thread,
            );
            thread::park();
            self.parked.lock().unwrap().remove(&header.tid);
        }
        Ok(())
    }
}

fn handle_client(
    mut stream: TcpStream,
    storage_path: PathBuf,
    dontcheck: bool,
    buffer_size: usize,
) -> Result<(), io::Error> {
    stream.set_send_buffer_size(buffer_size * 1024 * 1024)?;
    let mut init_buf = [0; size_of::<init_msg>()];
    stream.read_exact(&mut init_buf)?;

    println!("Calculating source hash...");
    let srchash = hash_metadata(storage_path.to_str().unwrap()).expect("Hash check failed");
    println!("Source hash is {:x}", srchash);

    let init = unsafe { transmute::<[u8; size_of::<init_msg>()], init_msg>(init_buf) };

    if (!dontcheck) && init.dsthash != srchash {
        println!(
            "%{:x} != {:x} client's hash does not match!",
            init.dsthash,
            srchash
        );
        println!("Dropping this client!");
        drop(stream);
        return Err(io::Error::new(io::ErrorKind::Other, "Hash mismatch"));
    }

    if init.mode == client_mode::MODE_SYNC || init.mode == client_mode::MODE_SEMISYNC {
        stream.set_nodelay(true)?;
    }

    let writer = if init.compress.contains(CompMode::STREAM_ZSTD) {
        Box::new(zstd::stream::Encoder::new(stream.try_clone()?, 0)?) as Box<Write + Send>
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

    let park_list = Arc::new(Mutex::new(HashMap::new()));
    let park_clone = park_list.clone();

    thread::spawn(move || Client::reader(stream, park_clone));

    SYNC_LIST.lock().unwrap().push(Client {
        write: writer,
        mode: init.mode,
        rt_comp: rt_comp,
        status: ClientStatus::ALIVE,
        parked: park_list,
    });

    println!("Client connected!");

    Ok(())
}

fn flush_thread() {
    loop {
        //println!("Flushed!");
        {
            let mut res = SYNC_LIST.lock().expect("Failed to lock SYNC_LIST");
            let list = res.deref_mut();

            for client in list.into_iter().filter(
                |c| c.mode == client_mode::MODE_ASYNC,
            )
            {
                if client.write.flush().is_err() {
                    client.status = ClientStatus::DEAD;
                }
            }
        }
        thread::sleep(Duration::from_secs(1));
    }

}

#[no_mangle]
pub extern "C" fn send_op(msg_data: *const c_void, ret_code: i32) -> i32 {

    let mut res = SYNC_LIST.lock().expect("Failed to lock SYNC_LIST");
    let list = res.deref_mut();

    for client in list.into_iter().filter(|c| c.status != ClientStatus::DEAD) {
        if client.send_msg(msg_data).is_err() {
            println!("Failed sending op to client");
            client.status = ClientStatus::DEAD;
        }
    }

    let mut corked = CORK.lock().unwrap();
    if *corked {
        for client in list.into_iter().filter(|c| c.status != ClientStatus::DEAD) {
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

    unsafe { free(msg_data as *mut c_void) };
    ret_code
}

pub fn display_fuse_help() {
    println!("Fuse options, specify at the end, after --:");
    let args = vec!["fsyncd", "--help"]
        .into_iter()
        .map(|arg| CString::new(arg).unwrap())
        .collect::<Vec<CString>>();
    // convert the strings to raw pointers
    let c_args = args.iter()
        .map(|arg| arg.as_ptr())
        .collect::<Vec<*const c_char>>();

    unsafe { fsyncer_fuse_main(c_args.len() as i32, c_args.as_ptr(), send_op) };

}

fn check_mount(path: &str) -> Result<bool, io::Error> {
    Ok(
        Command::new("mountpoint")
            .arg(path)
            .spawn()?
            .wait()?
            .success(),
    )
}

fn figure_out_paths(matches: &ArgMatches) -> Result<(PathBuf, PathBuf), io::Error> {
    let mount_path = Path::new(matches.value_of("mount-path").unwrap())
        .canonicalize()?;
    if matches.is_present("backing-store") &&
        !Path::new(matches.value_of("backing-store").unwrap()).exists()
    {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Backing path does not exist",
        ));
    }

    let backing_store = if matches.is_present("backing-store") {
        PathBuf::from(matches.value_of("backing-store").unwrap())
            .canonicalize()?
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

    thread::spawn(move || for stream in listener.incoming() {
        handle_client(
            stream.expect("Failed client connection"),
            backing_store.clone(),
            dont_check,
            buffer_size,
        );
    });
    /*
    thread::spawn(flush_thread);
    */
    let args = vec![
        "fsyncd".to_string(),
        matches.value_of("mount-path").unwrap().to_string(),
    ].into_iter()
        .chain(std::env::args().skip_while(|v| v != "--").skip(1))
        .map(|arg| CString::new(arg).unwrap())
        .collect::<Vec<CString>>();
    // convert the strings to raw pointers
    let c_args = args.iter()
        .map(|arg| arg.as_ptr())
        .collect::<Vec<*const c_char>>();

    unsafe { fsyncer_fuse_main(c_args.len() as i32, c_args.as_ptr(), send_op) };

    Ok(())
}
