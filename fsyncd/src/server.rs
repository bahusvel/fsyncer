use clap::ArgMatches;
use common::*;
use libc::{c_char, c_void, free};
use net2::TcpStreamExt;
use std;
use std::ffi::CString;
use std::io;
use std::io::{Read, Write};
use std::mem::{size_of, transmute};
use std::net::{TcpListener, TcpStream};
use std::ops::DerefMut;
use std::process::Command;
use std::ptr::null;
use std::slice;
use std::sync::Mutex;
use std::thread;
use zstd;
use std::path::{Path, PathBuf};
use std::fs;
use dssc::Compressor;
use dssc::flate::FlateCompressor;

struct Client {
    write: Box<Write + Send>,
    read: Box<Read + Send>,
    mode: client_mode,
    rt_comp: Option<Box<Compressor>>,
}

#[link(name = "fsyncer", kind = "static")]
#[link(name = "fuse3")]
extern "C" {
    fn fsyncer_fuse_main(
        argc: i32,
        argv: *const *const c_char,
        sop: extern "C" fn(*const c_void) -> i32,
    ) -> i32;
    fn hash_metadata(path: *const c_char) -> u64;
}

#[no_mangle]
#[allow(non_upper_case_globals)]
pub static mut server_path: *const c_char = null();

lazy_static!{
    static ref SYNC_LIST: Mutex<Vec<Client>> = Mutex::new(Vec::new());
}

fn handle_client(mut stream: TcpStream, dontcheck: bool) -> Result<(), io::Error> {
    stream.set_send_buffer_size(1024 * 1024)?;
    let mut init_buf = [0; size_of::<init_msg>()];
    stream.read_exact(&mut init_buf)?;

    println!("Calculating source hash...");
    let srchash = unsafe { hash_metadata(server_path) };
    println!("Source hash is {:x}", srchash);

    let init = unsafe { transmute::<[u8; size_of::<init_msg>()], init_msg>(init_buf) };

    if dontcheck && init.dsthash != srchash {
        println!(
            "%{:x} != {:x} client's hash does not match!",
            init.dsthash,
            srchash
        );
        println!("Dropping this client!");
        drop(stream);
        return Err(io::Error::new(io::ErrorKind::Other, "Hash mismatch"));
    }

    if init.mode == client_mode::MODE_SYNC {
        stream.set_nodelay(true)?;
    }

    let writer = if init.compress && init.mode == client_mode::MODE_ASYNC {
        Box::new(zstd::stream::Encoder::new(stream.try_clone()?, 0)?) as Box<Write + Send>
    } else {
        Box::new(stream.try_clone()?) as Box<Write + Send>
    };

    let rt_comp: Option<Box<Compressor>> =
        if init.compress && init.mode == client_mode::MODE_SYNC {
            Some(Box::new(FlateCompressor::default()))
        } else {
            None
        };

    SYNC_LIST.lock().expect("Failed to lock SYNC_LIST").push(
        Client {
            write: writer,
            read: Box::new(stream),
            mode: init.mode,
            rt_comp: rt_comp,
        },
    );

    println!("Client connected!");

    Ok(())
}

fn send_to_client(client: &mut Client, buf: &[u8]) -> Result<(), io::Error> {
    if let Some(ref mut rt_comp) = client.rt_comp {
        let mut msg = unsafe { *(buf.as_ptr() as *const op_msg) };

        let encoded = rt_comp.encode(&buf[size_of::<op_msg>()..]);
        // FIXME this is extremely inefficient, I need to change compressor
        msg.op_length = (encoded.len() + size_of::<op_msg>()) as u32;

        let header_buf = unsafe {transmute::<op_msg, [u8; size_of::<op_msg>()]>(msg)};
        let mut nbuf = Vec::new();
        nbuf.extend_from_slice(&header_buf[..]);
        nbuf.extend_from_slice(&encoded);

        client.write.write_all(&nbuf)?;
    } else {
        client.write.write_all(&buf)?;
    }
    if client.mode == client_mode::MODE_SYNC {
        let mut ack_buf = [0; size_of::<ack_msg>()];
        client.read.read_exact(&mut ack_buf)?;
    }

    Ok(())
}

#[no_mangle]
pub extern "C" fn send_op(msg_data: *const c_void) -> i32 {

    let mut res = SYNC_LIST.lock().expect("Failed to lock SYNC_LIST");
    let list = res.deref_mut();
    let mut delete = Vec::new();

    let msg = unsafe { &mut *(msg_data as *mut op_msg) };
    let buf = unsafe { slice::from_raw_parts(msg_data as *const u8, msg.op_length as usize) };

    for (i, ref mut client) in list.into_iter().enumerate() {
        if send_to_client(client, &buf).is_err()  {
            println!("Failed sending op to client");
            delete.push(i);
            continue;
        }
    }
    for i in delete {
        list.remove(i);
    }
    unsafe { free(msg_data as *mut c_void) };
    0
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
            let new_path = "";
            let res = Command::new("mount")
                .arg("--move")
                .arg(matches.value_of("mount-path").unwrap())
                .arg(new_path)
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

    thread::spawn(move || for stream in listener.incoming() {
        handle_client(stream.expect("Failed client connection"), dont_check);
    });

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
