metablock!(cfg(target_family = "unix") {
    macro_rules! trans_ppath {
        ($path:expr) => {
            translate_path(CStr::from_ptr($path), &SERVER_PATH.as_ref().unwrap())
        };
    }
    mod fusemain;
    mod fuseops;
    mod read_unix;
    mod write_unix;
    use self::fusemain::fuse_main;
    use journal::{BilogEntry, Journal, JournalConfig, JournalEntry};
    use std::{env, ffi::CString};
    use std::fs::OpenOptions;
    use libc::c_char;
    static mut JOURNAL: Option<Mutex<Journal>> = None;
});

metablock!(cfg(target_os = "windows") {
    macro_rules! trans_ppath {
        ($path:expr) => {
            trans_wstr($path, &SERVER_PATH.as_ref().unwrap())
        };
    }
    extern crate dokan;
    pub mod write_windows;
    #[no_mangle]
    pub unsafe extern "C" fn win_translate_path(buf: LPWSTR, path_len: ULONG, path: LPCWSTR) {
        use std::slice;
        let real_path = trans_ppath!(path);
        assert!(real_path.len() < path_len as usize);
        slice::from_raw_parts_mut(buf, path_len as usize)[..real_path.len()].copy_from_slice(&real_path)
    }
    use self::dokan::AddPrivileges;
});

mod client;
use self::client::{Client, ClientStatus};
use clap::ArgMatches;
use common::*;
use error::Error;
use libc::c_int;
use std::fs;
use std::io::{self, ErrorKind};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, RwLock};
use std::{
    borrow::Cow, mem::transmute, ops::Deref, process::Command, thread,
    time::Duration,
};

pub static mut SERVER_PATH: Option<PathBuf> = None;

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

fn send_call<'a>(
    call: Cow<'a, VFSCall<'a>>,
    client: &Client,
    ret: i32,
) -> Result<(), Error<io::Error>> {
    match client.mode {
        ClientMode::MODE_SYNC
        | ClientMode::MODE_SEMISYNC
        | ClientMode::MODE_FLUSHSYNC => {
            // In flushsync mode all ops except for fsync are sent async
            if client.mode == ClientMode::MODE_FLUSHSYNC
                && !is_variant!(&*call, VFSCall::fsync)
            {
                return client.send_msg(FsyncerMsg::AsyncOp(call), false);
            }

            let tid = unsafe {
                transmute::<thread::ThreadId, u64>(thread::current().id())
            };
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
        ClientMode::MODE_ASYNC => {
            client.send_msg(FsyncerMsg::AsyncOp(call), false)
        }
        ClientMode::MODE_CONTROL => Ok(()), // Don't send control anything
    }
}

pub fn pre_op(_call: &VFSCall) -> Option<c_int> {
    // This is safe, journal is only initialized once.
    #[cfg(target_family = "unix")]
    {
        if unsafe { JOURNAL.is_none() } {
            return None;
        }
        //println!("writing journal event {:?}", call);
        let bilog = BilogEntry::from_vfscall(_call, unsafe {
            &SERVER_PATH.as_ref().unwrap()
        })
        .expect("Failed to generate journal entry from vfscall");
        {
            // Reduce the time journal lock is held
            let mut j = unsafe { JOURNAL.as_ref().unwrap() }.lock().unwrap();
            j.write_entry(&bilog)
                .expect("Failed to write journal entry");
        }

        if is_variant!(bilog, BilogEntry::filestore, struct) {
            // Bypass real unlink when using filestore
            return Some(0);
        }
    }

    None
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
    /* Cork lock is held until here, it is used to make sure that any pending
     * operations get sent over the network, the flush operation will force
     * them to the other side */
}

#[cfg(target_family = "unix")]
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

fn check_mount(path: &str) -> Result<bool, Error<io::Error>> {
    Ok(Command::new("mountpoint")
        .arg(path)
        .spawn()
        .map_err(|e| make_err!(e))?
        .wait()
        .map_err(|e| make_err!(e))?
        .success())
}

#[cfg(target_os = "windows")]
fn copy_security(src: &Path, dst: &Path) -> Result<(), Error<io::Error>> {
    use common::with_file;
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;
    use winapi::um::winbase::FILE_FLAG_BACKUP_SEMANTICS;
    use winapi::um::winnt::{
        ACCESS_SYSTEM_SECURITY, DACL_SECURITY_INFORMATION, GENERIC_READ,
        GENERIC_WRITE, GROUP_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
        READ_CONTROL, SACL_SECURITY_INFORMATION, WRITE_DAC,
    };
    use winapi::um::winuser::{GetUserObjectSecurity, SetUserObjectSecurity};
    const DESC_LENGTH: usize = 4096;
    let info = OWNER_SECURITY_INFORMATION
        | GROUP_SECURITY_INFORMATION
        | DACL_SECURITY_INFORMATION
        | SACL_SECURITY_INFORMATION;
    let mut desc: [u8; DESC_LENGTH] = [0; DESC_LENGTH];
    let mut needed: u32 = 0;
    with_file(
        src,
        OpenOptions::new()
            .access_mode(ACCESS_SYSTEM_SECURITY | GENERIC_READ | READ_CONTROL)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS),
        |handle| {
            if unsafe {
                GetUserObjectSecurity(
                    handle,
                    &info as *const _ as *mut _,
                    &mut desc as *mut _ as *mut _,
                    DESC_LENGTH as u32,
                    &mut needed as *mut _,
                )
            } == 0
            {
                Err(make_err!(io::Error::last_os_error()))
            } else {
                Ok(())
            }
        },
    )
    .map_err(|e| make_err!(e))?
    .map_err(|e| trace_err!(e))?;
    if needed as usize > DESC_LENGTH {
        panic!("Failed to copy really large descriptor, Denis is lazy.");
    }
    with_file(
        dst,
        OpenOptions::new()
            .access_mode(ACCESS_SYSTEM_SECURITY | GENERIC_WRITE | WRITE_DAC)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS),
        |handle| {
            if unsafe {
                SetUserObjectSecurity(
                    handle,
                    &info as *const _ as *mut _,
                    &desc as *const _ as *mut _,
                )
            } == 0
            {
                Err(make_err!(io::Error::last_os_error()))
            } else {
                Ok(())
            }
        },
    )
    .map_err(|e| make_err!(e))?
    .map_err(|e| trace_err!(e))?;

    Ok(())
}

fn figure_out_paths(
    matches: &ArgMatches,
) -> Result<(PathBuf, PathBuf), Error<io::Error>> {
    let mount_path =
        canonize_path(Path::new(matches.value_of("mount-path").unwrap()))
            .map_err(|e| make_err!(e))?;

    debug!(mount_path);

    let mut mount_exists = mount_path.exists();

    #[cfg(target_os = "windows")]
    {
        if !mount_exists {
            // On windows mount_path may exists, but may be mounted to
            // previously crashed dokan file system. So another check is
            // neccessary to figure out if it exists.
            if let Some(parent) = mount_path.parent() {
                mount_exists = parent
                    .read_dir()
                    .map_err(|e| make_err!(e))?
                    .filter(|e| {
                        if let Ok(entry) = e {
                            entry.path() == mount_path
                        } else {
                            false
                        }
                    })
                    .next()
                    .is_some();
            }
        }
    }

    let backing_store = if matches.is_present("backing-store") {
        // Backing store specified
        let path = Path::new(matches.value_of("backing-store").unwrap());
        if !path.exists() {
            return Err(make_err!(io::Error::new(
                ErrorKind::NotFound,
                "Backing path does not exist",
            )));
        }
        PathBuf::from(matches.value_of("backing-store").unwrap())
            .canonicalize()
            .map_err(|e| make_err!(e))?
    } else {
        // Implictly inferring backing store
        mount_path.with_file_name(format!(
            ".fsyncer-{}",
            mount_path
                .file_name()
                .expect("You specified a weird file path")
                .to_str()
                .unwrap()
        ))
    };

    if !backing_store.exists() && mount_exists {
        // TODO figure out how to move mountpoints on windows
        if !cfg!(target_os = "windows")
            && check_mount(mount_path.to_str().unwrap())?
        {
            fs::create_dir_all(&mount_path).map_err(|e| make_err!(e))?;
            let res = Command::new("mount")
                .arg("--move")
                .arg(matches.value_of("mount-path").unwrap())
                .arg(backing_store.to_str().unwrap())
                .spawn()
                .map_err(|e| make_err!(e))?
                .wait()
                .map_err(|e| make_err!(e))?;
            if !res.success() {
                return Err(make_err!(io::Error::new(
                    ErrorKind::Other,
                    "Failed to move old mountpoint",
                )));
            }
        } else {
            fs::rename(&mount_path, &backing_store)
                .map_err(|e| make_err!(e))?;
        }
    }

    if backing_store.exists() && !mount_exists {
        fs::create_dir_all(&mount_path).map_err(|e| make_err!(e))?;
        copy_security(&backing_store, &mount_path)?;
    } else if !backing_store.exists() && !mount_exists {
        return Err(make_err!(io::Error::new(
            ErrorKind::NotFound,
            "Mount path does not exist",
        )));
    }

    #[cfg(target_os = "windows")]
    {
        // On windows mount path may exist, but cannot be used by dokan because
        // some other process is already accessing it.
        use std::os::windows::fs::OpenOptionsExt;
        let mut recreated = false;
        loop {
            let err = fs::OpenOptions::new()
                .write(true)
                .custom_flags(winapi::um::winbase::FILE_FLAG_BACKUP_SEMANTICS)
                .share_mode(0)
                .open(&mount_path);
            if err.is_err()
                && err
                    .unwrap_err()
                    .raw_os_error()
                    .expect("Failed to get OS error code")
                    as u32
                    == winapi::shared::winerror::ERROR_SHARING_VIOLATION
            {
                if !recreated {
                    println!("Mount path is busy, attempting to recreate it");
                    fs::remove_dir(&mount_path).map_err(|e| make_err!(e))?;
                    fs::create_dir(&mount_path).map_err(|e| make_err!(e))?;
                    copy_security(&backing_store, &mount_path)?;
                    recreated = true
                } else {
                    return Err(make_err!(io::Error::new(
                        ErrorKind::Other,
                        "Failed to establish ownership of the mount path",
                    )));
                }
            } else {
                break;
            }
        }
    }

    Ok((mount_path.to_path_buf(), backing_store))
}

#[cfg(target_family = "unix")]
fn open_journal(
    path: &str,
    c: JournalConfig,
) -> Result<Journal, Error<io::Error>> {
    let exists = Path::new(path).exists();
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)
        .map_err(|e| make_err!(e))?;

    if exists {
        if f.metadata()
            .expect("Failed to retrieve file metadata")
            .len()
            < c.journal_size
        {
            panic!("Refusing to shrink journal size")
        }
        f.set_len(c.journal_size)?;
        Journal::open(f, c)
    } else {
        f.set_len(c.journal_size)?;
        Journal::new(f, c)
    }
}

pub fn server_main(matches: ArgMatches) -> Result<(), Error<io::Error>> {
    let server_matches = matches.subcommand_matches("server").unwrap();
    #[cfg(target_os = "windows")]
    unsafe {
        if AddPrivileges() == 0 {
            panic!(
                "Failed to add security priviledge, make sure you run as \
                 Administrator"
            );
        }
    }
    let (mount_path, backing_store) =
        figure_out_paths(&server_matches).map_err(|e| trace_err!(e))?;
    println!("{:?}, {:?}", mount_path, backing_store);
    unsafe {
        SERVER_PATH = Some(backing_store.clone());
    }

    // FIXME use net2::TcpBuilder to set SO_REUSEADDR
    let listener = TcpListener::bind(format!(
        "0.0.0.0:{}",
        matches
            .value_of("port")
            .map(|v| v.parse::<i32>().expect("Invalid format for port"))
            .unwrap()
    ))
    .map_err(|e| make_err!(e))?;

    let dont_check = server_matches.is_present("dont-check");
    let buffer_size = parse_human_size(matches.value_of("buffer").unwrap())
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

    #[cfg(target_family = "unix")]
    {
        let journal_size =
            parse_human_size(server_matches.value_of("journal-size").unwrap())
                .expect("Invalid format for journal-size");
        let journal_sync = server_matches.is_present("journal-sync");

        match server_matches.value_of("journal").unwrap() {
            "bilog" => {
                let journal_path = server_matches
                    .value_of("journal-path")
                    .expect("Journal path must be set in bilog mode");

                let c = JournalConfig {
                    journal_size: journal_size as u64,
                    sync: journal_sync,
                    vfsroot: unsafe { SERVER_PATH.as_ref().unwrap().clone() },
                    filestore_size: 1024 * 1024 * 1024,
                };
                unsafe {
                    JOURNAL = Some(Mutex::new(
                        open_journal(journal_path, c)
                            .expect("Failed to open journal"),
                    ))
                }
            }
            "off" => {}
            _ => panic!("Unknown journal type"),
        }

        // Fuse args parsing
        let args = vec!["fsyncd".to_string(), mount_path]
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
    #[cfg(target_os = "windows")]
    {
        use self::dokan::*;
        let mut options = DOKAN_OPTIONS::zero();
        let wstr_mount_path = path_to_wstr(&mount_path);

        options.MountPoint = wstr_mount_path.as_ptr();
        //debug!(wstr_mount_path)
        options.Options |= DOKAN_OPTION_ALT_STREAM;

        if matches.is_present("debug") {
            options.Options |= DOKAN_OPTION_DEBUG | DOKAN_OPTION_STDERR;
        }

        let res = unsafe { dokan_main(options, DOKAN_OPS_PTR) };
        match res {
            Ok(DokanResult::Success) => {
                println!("Dokan exited {:?}", res);
                Ok(())
            }
            e => panic!("Dokan error {:?}", e),
        }
    }
}
