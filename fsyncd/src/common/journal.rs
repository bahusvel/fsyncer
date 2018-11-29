use bincode::{deserialize_from, serialize_into, serialized_size};
use byteorder::{LittleEndian, ReadBytesExt};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::fs::{File, OpenOptions};
use std::io::{Error, ErrorKind, Seek, SeekFrom, Write};
use std::marker::PhantomData;
use std::mem::size_of;
use std::sync::atomic::{AtomicUsize, Ordering};

static TRANS_CTR: AtomicUsize = AtomicUsize::new(0);
const ZERO_SIZE: [u8; 4096] = [0; 4096];

#[derive(Clone)]
struct JournaHeader {
    tail: u64,
    head: u64,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
struct JournalEntry<T> {
    fsize: u32,
    trans_id: u32,
    inner: T,
    rsize: u32,
}

pub struct Journal {
    header: JournaHeader,
    size: u64,
    file: File,
}

pub trait Direction {}
struct Forward;
impl Direction for Forward {}
struct Reverse;
impl Direction for Reverse {}

pub struct JournalIterator<'a, D: Direction, T> {
    direction: PhantomData<D>,
    inner_t: PhantomData<T>,
    header: JournaHeader,
    journal: &'a mut Journal,
}

macro_rules! iter_try {
    ($e:expr) => {
        match $e {
            Err(e) => return Some(Err(e)),
            Ok(e) => e,
        }
    };
}

#[inline(always)]
fn d2end(off: u64, end: u64) -> u64 {
    end - (off % end)
}

impl<'a, T: Debug> Iterator for JournalIterator<'a, Forward, T>
where
    for<'de> T: Deserialize<'de>,
{
    type Item = Result<T, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.header.head == self.header.tail {
            return None;
        }

        if d2end(self.header.head, self.journal.size) < 4 {
            // This is the right boundary of the buffer
            self.header.head += d2end(self.header.head, self.journal.size);
        } else {
            iter_try!(
                self.journal
                    .file
                    .seek(SeekFrom::Start((self.header.head) % self.journal.size))
            );

            let size = iter_try!(self.journal.file.read_u32::<LittleEndian>());
            if size == 0 {
                self.header.head += d2end(self.header.head, self.journal.size);
            }
        }

        iter_try!(
            self.journal
                .file
                .seek(SeekFrom::Start((self.header.head) % self.journal.size))
        );

        let entry: JournalEntry<T> = iter_try!(
            deserialize_from(&mut self.journal.file).map_err(|e| Error::new(ErrorKind::Other, e))
        );

        println!(
            "head {}, tail {}, entry {:?}",
            self.header.head, self.header.tail, entry
        );

        self.header.head += entry.fsize as u64;
        Some(Ok(entry.inner))
    }
}

impl<'a, T: Debug> Iterator for JournalIterator<'a, Reverse, T>
where
    for<'de> T: Deserialize<'de>,
{
    type Item = Result<T, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.header.head == self.header.tail {
            return None;
        }

        iter_try!(
            self.journal
                .file
                .seek(SeekFrom::Start((self.header.tail - 4) % self.journal.size))
        );

        let rsize = iter_try!(self.journal.file.read_u32::<LittleEndian>());
        iter_try!(self.journal.file.seek(SeekFrom::Current(-(rsize as i64))));

        let entry: JournalEntry<T> = iter_try!(
            deserialize_from(&mut self.journal.file).map_err(|e| Error::new(ErrorKind::Other, e))
        );

        println!(
            "head {}, tail {}, entry {:?}",
            self.header.head, self.header.tail, entry
        );

        self.header.tail -= rsize as u64;
        Some(Ok(entry.inner))
    }
}

impl Journal {
    fn new(file: File) -> Result<Self, Error> {
        Ok(Journal {
            header: JournaHeader { head: 0, tail: 0 },
            size: file.metadata()?.len(),
            file: file,
        })
    }

    fn write_entry<T: Serialize>(&mut self, entry: T) -> Result<(), Error> {
        let mut e = JournalEntry {
            fsize: 0,
            trans_id: TRANS_CTR.fetch_add(1, Ordering::Relaxed) as u32,
            inner: entry,
            rsize: 0,
        };
        let esize = serialized_size(&e).map_err(|e| Error::new(ErrorKind::Other, e))?;
        e.fsize = esize as u32;
        e.rsize = esize as u32;
        println!(
            "esize {}, tail {}, head {}",
            esize, self.header.tail, self.header.head
        );
        let pad = if d2end(self.header.tail, self.size) < esize {
            // The write would overlap the right boundary of ring buffer.
            d2end(self.header.tail, self.size)
        } else {
            0
        };
        let mut free_space = self.size - (self.header.tail - self.header.head);

        while free_space < esize + pad {
            println!("{} <= {} + {}", free_space, esize, pad);
            if self.header.head == self.header.tail {
                return Err(Error::new(
                    ErrorKind::UnexpectedEof,
                    "Journal is too small for this entry",
                ));
            }
            let fsize = if d2end(self.header.head, self.size) < 4 {
                d2end(self.header.head, self.size)
            } else {
                self.file
                    .seek(SeekFrom::Start(self.header.head % self.size))?;
                let s = self.file.read_u32::<LittleEndian>()?;
                if s == 0 {
                    d2end(self.header.head, self.size)
                } else {
                    s as u64
                }
            };
            println!("fsize {}, head {}", fsize, self.header.head);
            self.header.head += fsize as u64;
            free_space += fsize as u64;
        }

        self.file
            .seek(SeekFrom::Start(self.header.tail % self.size))?;
        if pad != 0 {
            let mut pad_left = pad as usize;
            while pad_left > 0 {
                self.file
                    .write_all(&ZERO_SIZE[..if pad_left >= 4096 { 4096 } else { pad_left }])?;
                pad_left -= 4096;
            }
            self.file
                .seek(SeekFrom::Start((self.header.tail + pad) % self.size))?;
        }

        serialize_into(&mut self.file, &e).map_err(|e| Error::new(ErrorKind::Other, e))?;
        self.header.tail += esize + pad;
        Ok(())
    }
    fn flush(&mut self) -> Result<(), Error> {
        self.file.flush()
    }
    fn flush_header() {}

    fn read_forward<T>(&mut self) -> JournalIterator<Forward, T> {
        JournalIterator {
            direction: PhantomData,
            inner_t: PhantomData,
            header: self.header.clone(),
            journal: self,
        }
    }
    fn read_reverse<T>(&mut self) -> JournalIterator<Reverse, T> {
        JournalIterator {
            direction: PhantomData,
            inner_t: PhantomData,
            header: self.header.clone(),
            journal: self,
        }
    }
}

#[test]
fn journal_test() {
    fn inner() -> Result<(), Error> {
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open("test.fj")?;
        f.set_len(1024)?;
        let mut j = Journal::new(f)?;
        for _ in 0..100 {
            j.write_entry("Hello")?;
        }

        for e in j.read_reverse::<String>() {
            println!("{:?}", e?);
        }

        Ok(())
    }
    inner().unwrap();
}
