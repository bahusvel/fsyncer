#[cfg(test)]
extern crate test;

#[cfg(test)]
use self::test::Bencher;
use bincode::{deserialize_from, serialize_into, serialized_size};
use byteorder::{LittleEndian, ReadBytesExt};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::fs::{File, OpenOptions};
use std::io::{Error, ErrorKind, Read, Seek, SeekFrom, Write};
use std::marker::PhantomData;
use std::os::unix::fs::FileExt;
use std::sync::atomic::{AtomicUsize, Ordering};

const HEADER_FLUSH_FREQUENCY: usize = 1000;

lazy_static! {
    static ref HEADER_SIZE: u64 = serialized_size(&JournalHeader { tail: 0, head: 0 }).unwrap();
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
struct JournalHeader {
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
    header: JournalHeader,
    trans_ctr: usize,
    size: u64,
    file: File,
}

pub trait Direction {}
pub struct Forward;
impl Direction for Forward {}
pub struct Reverse;
impl Direction for Reverse {}

pub struct JournalIterator<'a, D: Direction, T> {
    direction: PhantomData<D>,
    inner_t: PhantomData<T>,
    header: JournalHeader,
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

impl<'a, T: Debug> Iterator for JournalIterator<'a, Forward, T>
where
    for<'de> T: Deserialize<'de>,
{
    type Item = Result<T, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.header.head == self.header.tail {
            return None;
        }

        let mut new_head = self.header.head;
        self.journal.advance_offset(&mut new_head);
        self.header.head = new_head;

        let off = self.journal.file_off(self.header.head);
        iter_try!(self.journal.file.seek(SeekFrom::Start(off)));

        let entry: JournalEntry<T> = iter_try!(
            deserialize_from(&mut self.journal.file).map_err(|e| Error::new(ErrorKind::Other, e))
        );

        // println!(
        //     "head {}, tail {}, entry {:?}",
        //     self.header.head, self.header.tail, entry
        // );

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

        let off = self.journal.file_off(self.header.tail - 4);
        iter_try!(self.journal.file.seek(SeekFrom::Start(off)));

        let rsize = iter_try!(self.journal.file.read_u32::<LittleEndian>());
        //println!("rsize {}", rsize);

        let last_entry = self.header.tail - rsize as u64;

        let pad_size = if self.journal.d2end(last_entry) < 4 {
            self.journal.d2end(last_entry)
        } else {
            let off = self.journal.file_off(last_entry);
            iter_try!(self.journal.file.seek(SeekFrom::Start(off)));
            let size = iter_try!(self.journal.file.read_u32::<LittleEndian>());
            if size == 0 {
                self.journal.d2end(last_entry)
            } else {
                0
            }
        };

        let off = self.journal.file_off(last_entry + pad_size);

        iter_try!(self.journal.file.seek(SeekFrom::Start(off)));

        let entry: JournalEntry<T> = iter_try!(
            deserialize_from(&mut self.journal.file).map_err(|e| Error::new(ErrorKind::Other, e))
        );

        // println!(
        // "head {}, tail {}, entry {:?}",
        // self.header.head, self.header.tail, entry
        // );

        self.header.tail -= rsize as u64;
        Some(Ok(entry.inner))
    }
}

impl Journal {
    pub fn open(mut file: File, recover: bool) -> Result<Self, Error> {
        file.seek(SeekFrom::Start(0))?;
        let header = deserialize_from(&mut file).map_err(|e| Error::new(ErrorKind::Other, e))?;

        let mut j = Journal {
            header: header,
            trans_ctr: 0,
            size: file.metadata()?.len() - *HEADER_SIZE,
            file: file,
        };

        let off = j.file_off(j.header.tail + 4);
        let mut tx_max = j.file.read_u32::<LittleEndian>()?;
        loop {
            let off = j.file_off(j.header.tail + 4);
            j.file.seek(SeekFrom::Start(off))?;
            let next_tx = j.file.read_u32::<LittleEndian>()?;
            if tx_max + 1 != next_tx {
                break;
            }
            tx_max = next_tx;
            let mut new_tail = j.header.tail;
            j.advance_offset(&mut new_tail)?;
        }

        Ok(j)
    }
    pub fn write_entry<T: Serialize>(&mut self, entry: T) -> Result<(), Error> {
        const ZERO_SIZE: [u8; 4] = [0; 4];
        let mut e = JournalEntry {
            fsize: 0,
            trans_id: self.trans_ctr as u32,
            inner: entry,
            rsize: 0,
        };
        self.trans_ctr += 1;
        let esize = serialized_size(&e).map_err(|e| Error::new(ErrorKind::Other, e))?;
        e.fsize = esize as u32;
        // println!(
        // "esize {}, tail {}, head {}",
        // esize, self.header.tail, self.header.head
        // );
        let pad = if self.d2end(self.header.tail) < esize {
            // The write would overlap the right boundary of ring buffer.
            self.d2end(self.header.tail)
        } else {
            0
        };
        let mut free_space = self.size - (self.header.tail - self.header.head);

        while free_space < esize + pad {
            //println!("{} <= {} + {}", free_space, esize, pad);
            if self.header.head == self.header.tail {
                return Err(Error::new(
                    ErrorKind::UnexpectedEof,
                    "Journal is too small for this entry",
                ));
            }
            let mut new_head = self.header.head;
            self.advance_offset(&mut new_head)?;
            free_space += new_head - self.header.head;
            self.header.head = new_head;
        }
        e.rsize = (esize + pad) as u32;

        if pad != 0 {
            if pad >= 4 {
                self.file
                    .write_all_at(&ZERO_SIZE, self.file_off(self.header.tail))?;
            }
        }

        let mut buf = Vec::with_capacity(esize as usize);
        serialize_into(&mut buf, &e).map_err(|e| Error::new(ErrorKind::Other, e))?;
        self.file
            .write_all_at(&buf[..], self.file_off(self.header.tail + pad))?;
        self.header.tail += esize + pad;

        if e.trans_id as usize % HEADER_FLUSH_FREQUENCY == 0 {
            self.write_header()?;
        }

        Ok(())
    }

    // Advances arbitrary pointer to buffer by one entry, pointer must be aligned to the end of the previous entry or start of the file
    fn advance_offset(&mut self, off: &mut u64) -> Result<(), Error> {
        let h2end = self.d2end(*off);
        let fsize = if h2end < 4 {
            h2end
        } else {
            let off = self.file_off(*off);
            self.file.seek(SeekFrom::Start(off))?;
            let s = self.file.read_u32::<LittleEndian>()?;
            if s == 0 {
                h2end
            } else {
                s as u64
            }
        };
        *off += fsize;
        Ok(())
    }

    #[inline(always)]
    fn d2end(&self, off: u64) -> u64 {
        self.size - (off % self.size)
    }

    #[inline(always)]
    fn file_off(&self, off: u64) -> u64 {
        *HEADER_SIZE + (off % self.size)
    }

    pub fn flush(&mut self) -> Result<(), Error> {
        self.file.flush()
    }

    fn write_header(&mut self) -> Result<(), Error> {
        let mut buf = Vec::with_capacity(*HEADER_SIZE as usize);
        serialize_into(&mut buf, &self.header).map_err(|e| Error::new(ErrorKind::Other, e))?;
        self.file.write_all_at(&buf[..], 0)
    }

    pub fn read_forward<T>(&mut self) -> JournalIterator<Forward, T> {
        JournalIterator {
            direction: PhantomData,
            inner_t: PhantomData,
            header: self.header.clone(),
            journal: self,
        }
    }
    pub fn read_reverse<T>(&mut self) -> JournalIterator<Reverse, T> {
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

#[bench]
fn journal_write(b: &mut Bencher) {
    fn inner() -> Result<Journal, Error> {
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open("test.fj")?;
        f.set_len(1024 * 1024 * 1024)?;
        Journal::new(f)
    }
    let buf = [1; 4197];
    let mut j = inner().unwrap();
    b.iter(|| j.write_entry(&buf[..]).expect("Write failed"));
}
