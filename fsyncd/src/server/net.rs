use net2::TcpStreamExt;
use std::fs::File;
use std::io::{Error, Read, Write};
use std::net::{TcpListener, TcpStream};

metablock!(cfg(target_family = "unix") {
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::os::unix::io::{AsRawFd, RawFd};
});

struct NagleFlush(TcpStream);

#[cfg(target_os = "linux")]
impl Write for NagleFlush {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        use libc::*;
        use std::os::unix::io::AsRawFd;
        let res = unsafe {
            send(
                self.0.as_raw_fd(),
                buf.as_ptr() as *const _,
                buf.len(),
                MSG_MORE,
            )
        };
        if res == -1 {
            return Err(Error::last_os_error());
        }
        Ok(res as usize)
    }
    fn flush(&mut self) -> Result<(), Error> {
        use libc::*;
        use std::mem;
        use std::os::unix::io::AsRawFd;
        let optval = 0;
        unsafe {
            setsockopt(
                self.0.as_raw_fd(),
                SOL_TCP,
                TCP_CORK,
                &optval as *const _ as *const _,
                mem::size_of::<i32>() as u32,
            )
        };
        Ok(())
    }
}

impl AsRawFd for NagleFlush {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

#[cfg(not(target_os = "linux"))]
impl Write for NagleFlush {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> Result<(), Error> {
        self.0.set_nodelay(true)?;
        self.0.set_nodelay(false)
    }
}

pub trait MyRead: AsRawFd + Read + Send {}
pub trait MyWrite: AsRawFd + Write + Send {}
impl MyRead for TcpStream {}
impl MyWrite for TcpStream {}
impl MyRead for UnixStream {}
impl MyWrite for UnixStream {}
impl MyWrite for NagleFlush {}
impl MyRead for File {}
impl MyWrite for File {}
//impl MyRead for Deref<Target = MyRead> {}

pub trait Listener: Send {
    fn accept(
        &self,
        buffer_size: usize,
    ) -> Result<(Box<MyRead>, Box<MyWrite>, String), Error>;
}

impl Listener for TcpListener {
    fn accept(
        &self,
        buffer_size: usize,
    ) -> Result<(Box<MyRead>, Box<MyWrite>, String), Error> {
        let (stream, addr) = self.accept()?;
        stream.set_send_buffer_size(buffer_size)?;
        Ok((
            Box::new(stream.try_clone()?) as _,
            Box::new(NagleFlush(stream)) as _,
            format!("{:?}", addr),
        ))
    }
}
#[cfg(target_family = "unix")]
impl Listener for UnixListener {
    fn accept(
        &self,
        _buffer_size: usize,
    ) -> Result<(Box<MyRead>, Box<MyWrite>, String), Error> {
        let (stream, addr) = self.accept()?;
        Ok((
            Box::new(stream.try_clone()?) as _,
            Box::new(stream) as _,
            format!("{:?}", addr),
        ))
    }
}
