use error::{Error, FromError};
use libc::{fcntl, F_SETFD};
use std::io::{self, Read, Write};
use std::os::unix::io::RawFd;
use std::path::Path;
use std::process::{Command, Stdio};

/*
    This implementation sucks, the most sucky bit is termination of the channel and it works like this:
    1. Client side rsync will exit once it is finished syncing, which will cause the main thread loop in rsync_bridge to break; Sending a signal 0 length message to the other side.
    2. The second thread inside fakeshell (child process used to bridge rsync parent to channel) which will hit the terminate code path, send a signal back to the client and exit the (fakeshell) process.
    3. From the signal send by the fakeshell before dying, client second thread will see this message and return.
    4. Once the second thread on client side exits all threads belonging to rsync are terminated and normal communication may proceed over the TCP channel.
*/

pub fn server(
    netin: RawFd,
    netout: RawFd,
    src: &Path,
) -> Result<(), Error<io::Error>> {
    let netin = unsafe { libc::dup(netin) };
    let netout = unsafe { libc::dup(netout) };
    if unsafe { fcntl(netin, F_SETFD, 0) } == -1 {
        return Err(trace_err!(io::Error::last_os_error()));
    }
    if unsafe { fcntl(netout, F_SETFD, 0) } == -1 {
        return Err(trace_err!(io::Error::last_os_error()));
    }
    let mut fsyncd = trace!(std::env::current_exe())
        .to_str()
        .unwrap()
        .to_string();
    if fsyncd.ends_with(" (deleted)") {
        let nlen = fsyncd.len() - " (deleted)".len();
        fsyncd.truncate(nlen);
    }
    let mut src = src.to_path_buf();
    src.push("./");
    //debug!(fsyncd, src);
    trace!(trace!(Command::new("rsync")
        .args(&[
            //"rsync".into(),
            "-avhAX".into(),
            "--delete".into(),
            "-e".into(),
            std::ffi::OsString::from(format!(
                "{} fakeshell {} {}",
                fsyncd, netin, netout
            )),
            src.into_os_string(),
            ":.".into(),
        ])
        .stdin(Stdio::null())
        .spawn())
    .wait());
    Ok(())
}

#[derive(PartialEq)]
pub enum Direction {
    ToRsync,
    FromRsync,
}

pub fn rsync_bridge<
    NI: Read + Send + 'static,
    NO: Write + Send + 'static,
    RI: Write + Send + 'static,
    RO: Read + Send + 'static,
>(
    mut nin: NI,
    nout: NO,
    mut rin: RI,
    mut rout: RO,
    terminate: bool,
) -> Result<(NI, NO), io::Error> {
    use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
    use std::io::ErrorKind;
    use std::mem;
    use std::sync::{Arc, Mutex};
    use std::thread::{self, JoinHandle};
    let mut vec = Vec::with_capacity(4096);
    let netout = Arc::new(Mutex::new(nout));
    let netout_clone = netout.clone();
    let handle: JoinHandle<Result<NI, io::Error>> =
        thread::spawn(move || loop {
            let len = nin.read_u32::<BigEndian>()? as usize;
            if len == 0 {
                //eprintln!("fakeshell terminated");
                if terminate {
                    netout_clone.lock().unwrap().write_u32::<BigEndian>(0)?;
                    std::process::exit(0);
                } else {
                    return Ok(nin);
                }
            }
            if len > vec.len() {
                if len > vec.capacity() {
                    let res = len - vec.capacity();
                    vec.reserve(res);
                }
                unsafe {
                    vec.set_len(len);
                }
            }
            nin.read_exact(&mut vec[..len])?;
            //eprintln!("tcp->rsync {:?}", &vec[..len]);
            rin.write_all(&vec[..len])?;
        });
    loop {
        let mut buf: [u8; 4096] = unsafe { mem::zeroed() };
        let len = match rout.read(&mut buf) {
            Ok(0) => {
                netout.lock().unwrap().write_u32::<BigEndian>(0)?;
                break;
            }
            Ok(len) => len,
            Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        //eprintln!("rsync->tcp {:?}", &buf[..len]);
        netout.lock().unwrap().write_u32::<BigEndian>(len as u32)?;
        netout.lock().unwrap().write_all(&buf[..len])?;
    }
    Ok((
        handle.join().expect("thread panicked")?,
        if let Ok(lock) = Arc::try_unwrap(netout) {
            lock.into_inner().unwrap()
        } else {
            unreachable!();
        },
    ))
}

pub fn client<NI: Read + Send + 'static, NO: Write + Send + 'static>(
    nin: NI,
    nout: NO,
    dst: &Path,
) -> Result<(NI, NO), Error<io::Error>> {
    let child = trace!(Command::new("rsync")
        .args(&[
            //"rsync".into(),
            "--server".into(),
            "-vlogDtpAXre.iLsfxC".into(),
            "--delete".into(),
            ".".into(),
            dst.as_os_str().to_owned(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn());
    Ok(trace!(rsync_bridge(
        nin,
        nout,
        child.stdin.unwrap(),
        child.stdout.unwrap(),
        false,
    )))
}
