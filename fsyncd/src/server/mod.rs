metablock!(cfg(target_family = "unix") {
    use fuse_hl::start_fuse;
    use journal::{BilogEntry, Journal, JournalConfig, JournalType};
    use std::env;
    use std::fs::OpenOptions;
    static mut JOURNAL: Option<Mutex<Journal>> = None;
    static mut JOURNAL_TYPE: JournalType = JournalType::Invalid;
    use std::os::unix::net::UnixListener;
});

metablock!(cfg(target_os = "windows") {
    macro_rules! trans_ppath {
        ($path:expr) => {
            trans_wstr($path, &SERVER_PATH.as_ref().unwrap())
        };
    }
    extern crate dokan;
    #[no_mangle]
    pub unsafe extern "C" fn win_translate_path(buf: LPWSTR, path_len: ULONG, path: LPCWSTR) {
        use std::slice;
        let real_path = trans_ppath!(path);
        assert!(real_path.len() < path_len as usize);
        slice::from_raw_parts_mut(buf, path_len as usize)[..real_path.len()].copy_from_slice(&real_path)
    }
    use self::dokan::AddPrivileges;
    pub static mut TRANSLATE_SIDS: bool = true;
});

mod client;
pub mod net;

use self::client::{Client, ClientResponse, ClientStatus};
use clap::ArgMatches;
use common::file_security::copy_security;
use common::*;
use error::{Error, FromError};
use libc::c_int;
use std::fs;
use std::io::{self, ErrorKind};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::{
    borrow::Cow, mem::transmute, ops::Deref, process::Command, thread,
    time::Duration,
};

pub static mut SERVER_PATH: Option<PathBuf> = None;
pub static mut DIFF_WRITES: bool = false;

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
                eprintln!("Failed to flush to client");
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
    eprintln!("Corking");
    *CORK.lock().unwrap() = true;
    // Cork the individual clients
    let list = SYNC_LIST.read().expect("Failed to lock SYNC_LIST");
    for client in list.deref() {
        if let Err(e) = client.cork() {
            eprintln!("Failed to cork client {}", e);
        }
    }
    eprintln!("Cork done");
}

pub fn uncork_server() {
    eprintln!("Uncorking");
    let list = SYNC_LIST.read().expect("Failed to lock SYNC_LIST");
    for client in list.deref() {
        if let Err(e) = client.uncork() {
            eprintln!("Failed to uncork client {}", e);
        }
    }
    drop(list);
    *CORK.lock().unwrap() = false;
    CORK_VAR.notify_all();
    eprintln!("Uncork done");
}

pub struct OpRef {
    pub ret: Option<c_int>,
    waits: Vec<Arc<ClientResponse<ClientAck>>>,
}

pub fn pre_op(call: &VFSCall) -> OpRef {
    // THIS MAY NO LONGER BE CORRECT
    let mut corked = CORK.lock().unwrap();
    while *corked {
        corked = CORK_VAR.wait(corked).unwrap();
    }
    let mut opref = OpRef {
        ret: None,
        waits: Vec::new(),
    };

    let tid =
        unsafe { transmute::<thread::ThreadId, u64>(thread::current().id()) };
    let list = SYNC_LIST.read().expect("Failed to lock SYNC_LIST");
    for client in list.deref() {
        if client.mode == ClientMode::MODE_CONTROL
            || (is_variant!(&*call, VFSCall::fsync, struct)
                && (client.mode == ClientMode::MODE_ASYNC
                    || client.mode == ClientMode::MODE_SEMISYNC))
        {
            // Don't send anything to control, don't send flushes to
            // asynchronous client.
            continue;
        }
        let (msg, sync) = if client.mode == ClientMode::MODE_SYNC
            || client.mode == ClientMode::MODE_SEMISYNC
            || (client.mode == ClientMode::MODE_FLUSHSYNC
                && is_variant!(&*call, VFSCall::fsync, struct))
        {
            (FsyncerMsg::SyncOp(Cow::Borrowed(call), tid), true)
        } else {
            (FsyncerMsg::AsyncOp(Cow::Borrowed(call)), false)
        };
        match client.response_msg(msg, sync, sync) {
            Ok(None) => {}
            Ok(Some(response)) => opref.waits.push(response),
            Err(e) => eprintln!("Failed sending message to client {}", e),
        }
    }

    /* Cork lock is held until here, it is used to make sure that any pending
     * operations get sent over the network, the flush operation will force
     * them to the other side */
    drop(corked);

    #[cfg(target_family = "unix")]
    {
        // This is safe, journal is only initialized once.
        if unsafe { JOURNAL.is_none() } {
            return opref;
        }

        //eprintln!("writing journal event {:?}", call);

        use std::convert::TryFrom;
        match unsafe { JOURNAL_TYPE } {
            JournalType::Bilog => {
                let bilog = BilogEntry::try_from((call, unsafe {
                    &SERVER_PATH.as_ref().unwrap() as &Path
                }))
                .expect("Failed to generate journal entry from vfscall");
                if is_variant!(bilog, BilogEntry::filestore, struct) {
                    // Bypass real unlink when using filestore
                    opref.ret = Some(0);
                }
                {
                    // Reduce the time journal lock is held
                    let mut j =
                        unsafe { JOURNAL.as_ref().unwrap() }.lock().unwrap();
                    j.write_entry(&bilog)
                        .expect("Failed to write journal entry");
                }
            }
            JournalType::Forward => {
                {
                    // Reduce the time journal lock is held
                    let mut j =
                        unsafe { JOURNAL.as_ref().unwrap() }.lock().unwrap();
                    j.write_entry(call).expect("Failed to write journal entry");
                }
            }
            _ => panic!("Cannot generate entries of type {:?}", unsafe {
                JOURNAL_TYPE
            }),
        }
    }
    opref
}

pub fn post_op(opref: OpRef, ret: i32) -> i32 {
    for wait in opref.waits {
        let client_ret = wait.wait();
        if client_ret.is_none() {
            eprintln!("Client did not respond");
            continue;
        }
        let client_ret = client_ret.unwrap();
        match client_ret {
            ClientAck::Dead => {
                eprintln!("Client died before acknowledging write")
            }
            ClientAck::RetCode(code) if code != ret => eprintln!(
                "Response from client {} does not match server {}",
                code, ret
            ),
            _ => {}
        }
    }
    ret
}

fn check_mount(path: &str) -> Result<bool, Error<io::Error>> {
    Ok(
        trace!(trace!(Command::new("mountpoint").arg(path).spawn()).wait())
            .success(),
    )
}

fn figure_out_paths(
    matches: &ArgMatches,
) -> Result<(PathBuf, PathBuf), Error<io::Error>> {
    let mount_path = trace!(canonize_path(Path::new(
        matches.value_of("mount-path").unwrap()
    )));

    debug!(mount_path);

    #[allow(unused_mut)]
    let mut mount_exists = mount_path.exists();

    #[cfg(target_os = "windows")]
    {
        if !mount_exists {
            // On windows mount_path may exists, but may be mounted to
            // previously crashed dokan file system. So another check is
            // neccessary to figure out if it exists.
            if let Some(parent) = mount_path.parent() {
                mount_exists = trace!(parent.read_dir())
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
            trace!(Err(io::Error::new(
                ErrorKind::NotFound,
                "Backing path does not exist",
            )));
        }
        trace!(PathBuf::from(matches.value_of("backing-store").unwrap())
            .canonicalize())
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
            trace!(fs::create_dir_all(&mount_path));
            let res = trace!(trace!(Command::new("mount")
                .arg("--move")
                .arg(matches.value_of("mount-path").unwrap())
                .arg(backing_store.to_str().unwrap())
                .spawn())
            .wait());
            if !res.success() {
                trace!(Err(io::Error::new(
                    ErrorKind::Other,
                    "Failed to move old mountpoint",
                )));
            }
        } else {
            trace!(fs::rename(&mount_path, &backing_store));
        }
    }

    if backing_store.exists() && !mount_exists {
        trace!(fs::create_dir_all(&mount_path));
        trace!(copy_security(&backing_store, &mount_path));
    } else if !backing_store.exists() && !mount_exists {
        trace!(Err(io::Error::new(
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
                    eprintln!("Mount path is busy, attempting to recreate it");
                    trace!(fs::remove_dir(&mount_path));
                    trace!(fs::create_dir(&mount_path));
                    trace!(copy_security(&backing_store, &mount_path));
                    recreated = true
                } else {
                    trace!(Err(io::Error::new(
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
    let f = trace!(OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path));

    if exists {
        if f.metadata()
            .expect("Failed to retrieve file metadata")
            .len()
            < c.journal_size
        {
            panic!("Refusing to shrink journal size")
        }
        trace!(f.set_len(c.journal_size));
        Journal::open(f, c).map_err(|e| trace_err!(e))
    } else {
        trace!(f.set_len(c.journal_size));
        Journal::new(f, c).map_err(|e| trace_err!(e))
    }
}

pub fn server_main(matches: ArgMatches) -> Result<(), Error<io::Error>> {
    let server_matches = matches.subcommand_matches("server").unwrap();
    use url::Url;
    // Parse args
    //debug!(server_matches.value_of("url").unwrap());
    let url = Url::parse(server_matches.value_of("url").unwrap())
        .expect("Invalid url specified");
    #[cfg(target_os = "windows")]
    unsafe {
        if AddPrivileges() == 0 {
            panic!(
                "Failed to add security priviledge, make sure you run as \
                 Administrator"
            );
        }
    }
    let (mount_path, backing_store) = trace!(figure_out_paths(&server_matches));
    debug!(mount_path, backing_store);
    unsafe {
        SERVER_PATH = Some(backing_store.clone());
    }

    let interval = server_matches
        .value_of("flush-interval")
        .map(|v| v.parse::<u64>().expect("Invalid format for flush interval"))
        .unwrap();

    if server_matches.is_present("diff-writes") {
        unsafe {
            DIFF_WRITES = true;
        }
    }

    let dont_check = server_matches.is_present("dont-check");
    let buffer_size =
        parse_human_size(server_matches.value_of("buffer").unwrap())
            .expect("Buffer format incorrect");

    // Network

    if interval != 0 {
        thread::spawn(move || flush_thread(interval));
    }

    thread::spawn(harvester_thread);

    use self::net::Listener;

    match url.scheme() {
        "tcp" | "unix" => {
            let listener: Box<dyn Listener> = if url.scheme() == "tcp" {
                let mut url = url.clone();
                if url.port().is_none() {
                    url.set_port(Some(2323)).unwrap();
                }
                Box::new(trace!(TcpListener::bind(url))) as _
            } else {
                if Path::new(url.path()).exists() {
                    trace!(fs::remove_file(url.path()));
                }
                Box::new(trace!(UnixListener::bind(url.path()))) as _
            };
            thread::spawn(move || {
                while let Ok((netin, netout, addr)) =
                    listener.accept(buffer_size)
                {
                    eprintln!("Received connection from client {:?}", addr);
                    let client = Client::from_stream(netin, netout, dont_check);
                    match client {
                        Ok(client) => SYNC_LIST.write().unwrap().push(client),
                        Err(e) => eprintln!("Failed handling client {:?}", e),
                    }
                }
            });
        }
        "stdio" => {
            use std::fs::File;
            use std::os::unix::io::FromRawFd;
            thread::spawn(move || {
                Client::from_stream(
                    Box::new(unsafe { File::from_raw_fd(0) }) as _,
                    Box::new(unsafe { File::from_raw_fd(1) }) as _,
                    dont_check,
                )
            });
        }
        otherwise => panic!("Scheme {} is not supported", otherwise),
    };

    // Journal

    #[cfg(target_family = "unix")]
    {
        let journal_size =
            parse_human_size(server_matches.value_of("journal-size").unwrap())
                .expect("Invalid format for journal-size");
        let journal_sync = server_matches.is_present("journal-sync");

        if server_matches.value_of("journal").unwrap() != "off" {
            let journal_path = server_matches
                .value_of("journal-path")
                .expect("Journal path must be set if journaling is enabled");

            let journal_type = match server_matches.value_of("journal").unwrap()
            {
                "bilog" => JournalType::Bilog,
                "forward" => JournalType::Forward,
                "undo" => JournalType::Undo,
                _ => unreachable!(),
            };

            unsafe { JOURNAL_TYPE = journal_type };

            let c = JournalConfig {
                journal_size: journal_size as u64,
                sync: journal_sync,
                journal_type,
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
    }

    // Fs proxying

    #[cfg(target_family = "unix")]
    {
        start_fuse(&mount_path, env::args().skip_while(|v| v != "--").skip(1));
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        use self::dokan::*;
        let mut options = DOKAN_OPTIONS::zero();
        let wstr_mount_path = path_to_wstr(&mount_path);

        if server_matches.is_present("send-sids") {
            unsafe { TRANSLATE_SIDS = false };
        }

        options.MountPoint = wstr_mount_path.as_ptr();
        //debug!(wstr_mount_path)
        options.Options |= DOKAN_OPTION_ALT_STREAM;

        if matches.is_present("debug") {
            options.Options |= DOKAN_OPTION_DEBUG | DOKAN_OPTION_STDERR;
        }

        let res = unsafe { dokan_main(options, DOKAN_OPS_PTR) };
        match res {
            Ok(DokanResult::Success) => {
                eprintln!("Dokan exited {:?}", res);
                Ok(())
            }
            e => panic!("Dokan error {:?}", e),
        }
    }
}
