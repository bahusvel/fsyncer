use std::cmp::{max, min};
use std::io::{Error, ErrorKind, Result, Write};
use std::thread::sleep;
use std::time::{Duration, Instant};

pub struct LimitWriter<W> {
    inner: W,
    bytes_left: usize,
    bps: usize,
    last_updated: Instant,
    blocking: bool,
    partial_writes: bool,
}

impl<W> LimitWriter<W> {
    pub fn new(writer: W, bps: usize) -> Self {
        LimitWriter {
            inner: writer,
            bytes_left: bps,
            bps,
            last_updated: Instant::now(),
            partial_writes: false,
            blocking: true,
        }
    }
    pub fn set_partial_writes(&mut self, partial_writes: bool) {
        self.partial_writes = partial_writes;
    }
    pub fn set_blocking(&mut self, blocking: bool) {
        self.blocking = blocking;
    }
}

// Guarantees wait
fn sleep_until(i: Instant) -> Instant {
    let mut now = Instant::now();
    loop {
        if i < now {
            return now;
        }
        let d = i - now;
        sleep(d);
        now = Instant::now();
    }
}

impl<W: Write> Write for LimitWriter<W> {
    fn write(&mut self, mut buf: &[u8]) -> Result<usize> {
        loop {
            if self.bps == 0 {
                // Unlimited
                break;
            }

            if self.bytes_left >= buf.len() {
                // Can write full buffer
                break;
            }

            if self.partial_writes && self.bytes_left != 0 {
                buf = &buf[..min(self.bytes_left, buf.len())];
                break;
            }

            let now = Instant::now();
            // Check the time
            self.bytes_left += min(
                now.duration_since(self.last_updated).as_millis() as usize
                    * (self.bps / 1000),
                max(buf.len() - self.bytes_left, self.bps),
            );
            self.last_updated = now;

            if self.bytes_left >= buf.len() {
                // Can write full buffer
                break;
            }

            if self.blocking {
                // Block for the time neccessary to fit the quota
                let wait_for = Duration::from_millis(
                    ((buf.len() - self.bytes_left) / (self.bps / 1000)) as u64,
                );
                self.last_updated = sleep_until(now + wait_for);
                break;
            } else {
                return Err(Error::new(
                    ErrorKind::WouldBlock,
                    "IO limit exceeded",
                ));
            }
        }

        let written = self.inner.write(buf)?;

        self.bytes_left -= min(written, self.bytes_left);
        Ok(written)
    }
    fn flush(&mut self) -> Result<()> {
        self.inner.flush()
    }
}
