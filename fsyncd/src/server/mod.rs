mod fusemain;
mod fuseops;
mod read;
mod write;

use self::fusemain::fuse_main;
use bincode::{deserialize_from, serialize, serialized_size};
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use clap::ArgMatches;
use common::*;
use dssc::chunkmap::ChunkMap;
use dssc::other::ZstdBlock;
use dssc::Compressor;
use libc::{c_char, c_int};
use lz4;
use net2::TcpStreamExt;
use std;
use std::borrow::Cow;
use std::collections::HashMap;
use std::ffi::CString;
use std::fs;
use std::io;
use std::io::{Read, Write};
use std::mem::transmute;
use std::net::{TcpListener, TcpStream};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread;
use std::time::Duration;
use zstd;

pub static mut SERVER_PATH_RUST: String = String::new();
static NOP_MSG: FsyncerMsg = FsyncerMsg::NOP;

lazy_static! {
    static ref SYNC_LIST: RwLock<Vec<Client>> = RwLock::new(Vec::new());
    static ref CORK_VAR: Condvar = Condvar::new();
    static ref CORK: Mutex<bool> = Mutex::new(false);
    static ref ENCODED_NOP: Vec<u8> = serialize(&NOP_MSG).unwrap();
    static ref NOP_SIZE: usize = serialized_size(&NOP_MSG).unwrap() as usize;
}

#[derive(PartialEq)]
enum ClientStatus {
    DEAD,
    ALIVE,
}

struct ClientNetwork {
    write: Box<Write + Send>,
    rt_comp: Option<Box<Compressor>>,
    // TODO remvoe this hashmap and use an array
    parked: HashMap<u64, Arc<ClientResponse<i32>>>,
    status: ClientStatus,
}

struct Client {
    mode: ClientMode,
    comp: CompMode,
    net: Arc<Mutex<ClientNetwork>>,
}

struct ClientResponse<T> {
    data: Mutex<Option<T>>,
    cvar: Condvar,
}

impl<T> ClientResponse<T> {
    pub fn new() -> Self {
        ClientResponse {
            data: Mutex::new(None),
            cvar: Condvar::new(),
        }
    }
    pub fn wait(&self) -> T {
        let mut lock = self.data.lock().unwrap();

        while lock.is_none() {
            lock = self.cvar.wait(lock).unwrap();
        }

        lock.take().unwrap()
    }

    pub fn notify(&self, data: T) {
        *self.data.lock().unwrap() = Some(data);
        self.cvar.notify_one()
    }
}

impl Client {
    // Send a cork to this client, and block until it acknowledges
    fn cork(&self) -> Result<(), io::Error> {
        let current_thread = thread::current();
        let tid = unsafe { transmute::<thread::ThreadId, u64>(current_thread.id()) };
        self.send_msg(FsyncerMsg::Cork(tid), true)?;
        // Cannot park on control as it will block its reader thread
        if self.mode != ClientMode::MODE_CONTROL {
            self.wait_thread_response();
        }
        Ok(())
    }

    fn uncork(&self) -> Result<(), io::Error> {
        self.send_msg(FsyncerMsg::Uncork, false)
    }

    fn read_msg<R: Read>(read: &mut R) -> Result<FsyncerMsg, io::Error> {
        let _size = read.read_u32::<BigEndian>()?;
        // TODO use size to restrict reading
        Ok(deserialize_from(read).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?)
    }

    fn reader<R: Read>(mut read: R, net: Arc<Mutex<ClientNetwork>>) {
        let net = net.deref();
        loop {
            match Client::read_msg(&mut read) {
                Ok(FsyncerMsg::AckCork(tid)) => {
                    let mut netlock = net.lock().unwrap();
                    netlock.parked.get(&tid).map(|t| {
                        assert!(Arc::strong_count(t) <= 2);
                        t.notify(0)
                    });
                }
                Ok(FsyncerMsg::Ack(AckMsg { retcode: code, tid })) => {
                    let mut netlock = net.lock().unwrap();
                    netlock.parked.get(&tid).map(|t| {
                        assert!(Arc::strong_count(t) <= 2);
                        t.notify(code)
                    });
                }
                Ok(FsyncerMsg::Cork(_)) => cork_server(),
                Ok(FsyncerMsg::Uncork) => uncork_server(),
                Err(e) => {
                    let mut netlock = net.lock().unwrap();
                    netlock.status = ClientStatus::DEAD;
                    // Will kill this thread
                    println!("Failed to read from client {}", e);
                    return;
                }
                msg => println!("Unexpected message from client {:?}", msg),
            }
        }
    }

    fn flush(&self) -> Result<(), io::Error> {
        //Without the nop message compression algorithms dont flush immediately.
        if self.comp.intersects(CompMode::STREAM_MASK) {
            self.send_msg(FsyncerMsg::NOP, true)?
        } else {
            self.net.lock().unwrap().write.flush()?
        }
        Ok(())
    }

    fn send_msg(&self, msg_data: FsyncerMsg, flush: bool) -> Result<(), io::Error> {
        fn inner(
            serbuf: &[u8],
            mut size: usize,
            net: &mut ClientNetwork,
            flush: bool,
            comp: bool,
        ) -> Result<(), io::Error> {
            let mut nbuf = Vec::new();

            let buf = if let Some(ref mut rt_comp) = net.rt_comp {
                rt_comp.encode(&serbuf[..], &mut nbuf);
                size = nbuf.len();
                &nbuf[..]
            } else {
                &serbuf[..]
            };

            // Uggly way to shortcut error checking
            net.write.write_u32::<BigEndian>(size as u32)?;
            net.write.write_all(&buf)?;
            if flush {
                //println!("Doing funky flush");
                net.write.flush()?;
                // Without the nop message compression algorithms dont flush immediately.
                if comp {
                    inner(&ENCODED_NOP[..], *NOP_SIZE, net, false, comp)?;
                    net.write.flush()?;
                }
                //println!("Finished funky flush");
            }
            Ok(())
        }

        let size = serialized_size(&msg_data)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))? as usize;

        let serbuf = serialize(&msg_data).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        //println!("Sending {} {}", header.op_length, hbuf.len() + buf.len());
        let mut net = self.net.lock().unwrap();

        if net.status == ClientStatus::DEAD {
            // Ignore writes to dead clients, they will be harvested later
            return Ok(());
        }

        let res = inner(
            &serbuf[..],
            size,
            &mut *net,
            flush,
            self.comp.intersects(CompMode::STREAM_MASK),
        );

        if res.is_err() {
            net.status = ClientStatus::DEAD;
        }

        res
    }

    fn wait_thread_response(&self) -> i32 {
        {
            let tid = unsafe { transmute::<thread::ThreadId, u64>(thread::current().id()) };
            let mut net = self.net.lock().unwrap();
            net.parked
                .entry(tid)
                .or_insert(Arc::new(ClientResponse::new()))
                .clone()
        }
        .wait()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // Unblock all threads that could be waiting on this client
        for (_, thread) in self.net.lock().unwrap().parked.iter() {
            assert!(Arc::strong_count(thread) <= 2);
            thread.notify(-1);
        }
    }
}

fn handle_client(
    mut stream: TcpStream,
    storage_path: PathBuf,
    dontcheck: bool,
    buffer_size: usize,
) -> Result<(), io::Error> {
    println!("Received connection from client {:?}", stream.peer_addr());
    stream.set_send_buffer_size(buffer_size * 1024 * 1024)?;

    let init = match Client::read_msg(&mut stream) {
        Ok(FsyncerMsg::InitMsg(msg)) => msg,
        Err(e) => panic!("Failed to get init message from client {}", e),
        otherwise => panic!(
            "Expected init message from client, received {:?}",
            otherwise
        ),
    };

    if init.mode != ClientMode::MODE_CONTROL && (!dontcheck) {
        println!("Calculating source hash...");
        let srchash = hash_metadata(storage_path.to_str().unwrap()).expect("Hash check failed");
        println!("Source hash is {:x}", srchash);
        if init.dsthash != srchash {
            println!(
                "{:x} != {:x} client's hash does not match!",
                init.dsthash, srchash
            );
            println!("Dropping this client!");
            drop(stream);
            return Err(io::Error::new(io::ErrorKind::Other, "Hash mismatch"));
        }
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
        mode: init.mode,
        comp: init.compress,
        net: net,
    });

    println!("Client connected!");

    Ok(())
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
            .filter(|c| c.net.lock().unwrap().status == ClientStatus::DEAD)
            .count()
            != 0;
        // if there are, obtain exclusive lock and remove them
        if have_dead_nodes {
            SYNC_LIST
                .write()
                .unwrap()
                .retain(|c| c.net.lock().unwrap().status != ClientStatus::DEAD);
        }
        thread::sleep(Duration::from_secs(1));
    }
}

fn cork_server() {
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

fn uncork_server() {
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

pub fn handle_op(call: VFSCall, ret: i32) -> i32 {
    let mut corked = CORK.lock().unwrap();
    while *corked {
        corked = CORK_VAR.wait(corked).unwrap();
    }

    let list = SYNC_LIST.read().expect("Failed to lock SYNC_LIST");
    for client in list.deref() {
        let res = send_call(Cow::Borrowed(&call), client, ret);
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

pub fn server_main(matches: ArgMatches) -> Result<(), io::Error> {
    let server_matches = matches.subcommand_matches("server").unwrap();
    let (mount_path, backing_store) = figure_out_paths(&server_matches)?;
    println!("{:?}, {:?}", mount_path, backing_store);
    unsafe {
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

    let dont_check = server_matches.is_present("dont-check");
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
            )
            .expect("Failed handling client");
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

    let args = vec![
        "fsyncd".to_string(),
        server_matches.value_of("mount-path").unwrap().to_string(),
    ]
    .into_iter()
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
