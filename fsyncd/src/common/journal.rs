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
    static ref HEADER_SIZE: u64 = serialized_size(&JournalHeader {
        tail: 0,
        head: 0,
        trans_ctr: 0
    }).unwrap();
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
struct JournalHeader {
    tail: u64,
    head: u64,
    trans_ctr: u32,
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

macro_rules! debug {
    ($($e:expr),+) => {
        $(
            print!(concat!(stringify!($e), "={} "), $e);
        )*
        println!();
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
        iter_try!(self.journal.advance_offset(&mut new_head));
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
    pub fn open(mut file: File, new: bool) -> Result<Self, Error> {
        let header = if new {
            JournalHeader {
                head: 0,
                tail: 0,
                trans_ctr: 0,
            }
        } else {
            file.seek(SeekFrom::Start(0))?;
            deserialize_from(&mut file).map_err(|e| Error::new(ErrorKind::Other, e))?
        };

        let mut j = Journal {
            header: header,
            size: file.metadata()?.len() - *HEADER_SIZE,
            file: file,
        };

        if new {
            return Ok(j);
        }

        println!("Traversing the journal {:?}", j.header);

        let mut tx_max = j.header.trans_ctr - 1; // Because the ctr has been advanced before flush
        let mut new_tail = j.header.tail;
        // Only traverse up to header flush frequency
        for _ in 0..HEADER_FLUSH_FREQUENCY {
            j.seek(new_tail + 4)?;
            let next_tx = j.file.read_u32::<LittleEndian>()?;
            println!("Next tx {} old tx {}", next_tx, tx_max);
            // Allows for overflow to happen
            if next_tx <= tx_max && next_tx != tx_max + 1 {
                break;
            }
            tx_max = next_tx;
            j.skip_entry(&mut new_tail)?;
        }

        j.header.tail = new_tail;
        j.header.trans_ctr = tx_max + 1;

        // TODO head recovery
        // Head must be somewhere not too far in front of the tail, or it is at the beggining of the file because journal hasn't looped yet. The problem is, there is no way to know how far in front of the tail it is. To resolve this I should write some kind of marker after the tail indiciating the distance to the head, or simply every time I allocate head space flush the header prior to writing the journal entry.

        //Yes lets do the last option.

        Ok(j)
    }

    pub fn write_entry<T: Serialize>(&mut self, entry: T) -> Result<(), Error> {
        const ZERO_SIZE: [u8; 4] = [0; 4];
        let mut e = JournalEntry {
            fsize: 0,
            trans_id: self.header.trans_ctr as u32,
            inner: entry,
            rsize: 0,
        };
        self.header.trans_ctr += 1;
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

        assert!(
            self.size > esize + pad,
            "Journal is too small for this entry"
        );

        while free_space < esize + pad {
            //println!("{} <= {} + {}", free_space, esize, pad);
            if self.header.head == self.header.tail {
                panic!("Ran into tail while freeing up space, but journal is large enough to fit the entry")
            }
            let mut new_head = self.header.head;
            self.skip_entry(&mut new_head)?;
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

    fn seek(&mut self, off: u64) -> Result<(), Error> {
        let o = self.file_off(off);
        self.file.seek(SeekFrom::Start(o))?;
        Ok(())
    }

    // Advances arbitrary pointer to buffer from the end of one entry to the beginning of the next
    fn advance_offset(&mut self, off: &mut u64) -> Result<(), Error> {
        let h2end = self.d2end(*off);
        if h2end < 4 {
            *off += h2end
        } else {
            let o = self.file_off(*off);
            self.file.seek(SeekFrom::Start(o))?;
            let s = self.file.read_u32::<LittleEndian>()?;
            if s == 0 {
                *off += h2end
            }
        };
        Ok(())
    }

    // Skips the next entry
    fn skip_entry(&mut self, off: &mut u64) -> Result<(), Error> {
        let h2end = self.d2end(*off);
        if h2end < 4 {
            *off += h2end;
        } else {
            let o = self.file_off(*off);
            self.file.seek(SeekFrom::Start(o))?;
            let s = self.file.read_u32::<LittleEndian>()?;
            if s == 0 {
                *off += h2end;
            } else {
                *off += s as u64;
                return Ok(());
            }
        };
        let o = self.file_off(*off);
        self.file.seek(SeekFrom::Start(o))?;
        *off += self.file.read_u32::<LittleEndian>()? as u64;
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
fn journal_rw() {
    fn inner() -> Result<(), Error> {
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open("test.fj")?;
        f.set_len(1024)?;
        let mut j = Journal::open(f, true)?;
        for _ in 0..100 {
            j.write_entry("Hello")?;
        }
        for e in j.read_reverse::<String>() {
            println!("{:?}", e?);
        }
        println!("Header {:?}", j.header);
        Ok(())
    }
    inner().unwrap();
}

#[test]
fn journal_recover() {
    fn inner() -> Result<(), Error> {
        let f = File::open("test.fj")?;
        let mut j = Journal::open(f, false)?;
        println!("{:?}", j.header);
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
        Journal::open(f, true)
    }
    let buf = [1; 4197];
    let mut j = inner().unwrap();
    b.iter(|| j.write_entry(&buf[..]).expect("Write failed"));
}
