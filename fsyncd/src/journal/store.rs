#[cfg(test)]
extern crate test;

extern crate crc;

#[cfg(test)]
use self::test::Bencher;
#[cfg(test)]
use std::{fs::OpenOptions, io::Read};

use self::crc::crc32;
use bincode::{deserialize, deserialize_from, serialize_into, serialized_size};
use byteorder::{ByteOrder, LittleEndian, ReadBytesExt, WriteBytesExt};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::fs::File;
use std::io::{Error, ErrorKind, Seek, SeekFrom, Write};
use std::marker::PhantomData;
use std::os::unix::fs::FileExt;

const BLOCK_SIZE: u64 = 1024 * 128;

lazy_static! {
    static ref HEADER_SIZE: u64 = serialized_size(&JournalHeader {
        tail: 0,
        head: 0,
        trans_ctr: 0
    })
    .unwrap();
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
    crc32: u32,
}

pub struct Journal {
    header: JournalHeader,
    size: u64,
    file: File,
    sync: bool,
    sbuf: Vec<u8>,
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
    block_buffer: Vec<T>,
}

#[inline(always)]
fn align_up_always(off: u64, align: u64) -> u64 {
    (off / align + 1) * align
}

#[inline(always)]
fn align_down(off: u64, align: u64) -> u64 {
    off & !(align - 1)
}

impl<'a, T: Debug> Iterator for JournalIterator<'a, Forward, T>
where
    for<'de> T: Deserialize<'de>,
{
    type Item = Result<T, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        let mut buf: [u8; 4] = [0; 4];
        if self.header.head == self.header.tail {
            return None;
        }

        if align_up_always(self.header.head, BLOCK_SIZE) - self.header.head < 4 {
            self.header.head = align_up_always(self.header.head, BLOCK_SIZE);
        } else {
            iter_try!(self
                .journal
                .file
                .read_exact_at(&mut buf[..], self.journal.file_off(self.header.head)));
            if buf == [0; 4] {
                self.header.head = align_up_always(self.header.head, BLOCK_SIZE);
            }
        }

        iter_try!(self.journal.seek(self.header.head));

        let entry: JournalEntry<T> =
            iter_try!(deserialize_from(&mut self.journal.file)
                .map_err(|e| Error::new(ErrorKind::Other, e)));

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
        let mut buf: [u8; 4] = [0; 4];
        if self.block_buffer.len() == 0 {
            if self.header.head == self.header.tail || self.header.tail == 0 {
                return None;
            }

            //debug!(self.header.head, self.header.tail);

            //self.header.tail = align_down(self.header.tail, BLOCK_SIZE);
            let mut decode_tail = if self.header.tail % BLOCK_SIZE != 0 {
                align_down(self.header.tail, BLOCK_SIZE)
            } else {
                self.header.tail - BLOCK_SIZE
            };

            //debug!(decode_tail);
            loop {
                if self.header.tail - decode_tail < 4 {
                    break;
                }
                iter_try!(self
                    .journal
                    .file
                    .read_exact_at(&mut buf[..], self.journal.file_off(decode_tail)));
                if buf == [0; 4] {
                    break;
                }
                let fsize = LittleEndian::read_u32(&buf[..]) as u64;
                //debug!(fsize, decode_tail, align_up_always(decode_tail, BLOCK_SIZE));
                if fsize > align_up_always(decode_tail, BLOCK_SIZE) - decode_tail {
                    debug!(fsize, decode_tail, align_up_always(decode_tail, BLOCK_SIZE));
                    return Some(Err(Error::new(ErrorKind::Other, "Entry too large")));
                }
                iter_try!(self.journal.seek(decode_tail));
                let mut buf = Vec::with_capacity(fsize as usize);
                unsafe {
                    buf.set_len(fsize as usize);
                }
                iter_try!(self
                    .journal
                    .file
                    .read_exact_at(&mut buf[..], self.journal.file_off(decode_tail)));

                let crc_recorded = LittleEndian::read_u32(&buf[buf.len() - 4..]);
                let crc_computed = crc32::checksum_ieee(&buf[..buf.len() - 4]);

                if crc_recorded != crc_computed {
                    debug!(crc_recorded, crc_computed);
                    return Some(Err(Error::new(ErrorKind::Other, "Entry checksum mismatch")));
                }

                let entry: JournalEntry<T> =
                    iter_try!(deserialize(&buf).map_err(|e| Error::new(ErrorKind::Other, e)));

                decode_tail += fsize as u64;
                self.block_buffer.push(entry.inner);
            }

            if self.header.tail % BLOCK_SIZE != 0 {
                // Partial block read
                self.header.tail = align_down(self.header.tail, BLOCK_SIZE);
            } else {
                if self.header.tail != 0 && self.header.tail != self.header.head {
                    self.header.tail -= BLOCK_SIZE;
                }
            }
        }

        self.block_buffer.pop().map(|v| Ok(v))
    }
}

impl Journal {
    pub fn new(file: File, sync: bool) -> Result<Self, Error> {
        Ok(Journal {
            header: JournalHeader {
                head: 0,
                tail: 0,
                trans_ctr: 0,
            },
            size: align_down(file.metadata()?.len() - *HEADER_SIZE, BLOCK_SIZE),
            file: file,
            sbuf: Vec::new(),
            sync,
        })
    }
    pub fn open(mut file: File, sync: bool) -> Result<Self, Error> {
        file.seek(SeekFrom::Start(0))?;
        let header = deserialize_from(&mut file).map_err(|e| Error::new(ErrorKind::Other, e))?;

        let mut j = Journal {
            header: header,
            size: align_down(file.metadata()?.len() - *HEADER_SIZE, BLOCK_SIZE),
            file: file,
            sbuf: Vec::new(),
            sync,
        };

        println!("Traversing the journal {:?}", j.header);

        let mut tx_max = j.header.trans_ctr - 1; // Because the ctr has been advanced before flush
        let mut new_tail = j.header.tail;
        loop {
            if new_tail > align_up_always(j.header.tail, BLOCK_SIZE) {
                println!("Traversing past block boundary");
                break;
            }
            if align_up_always(new_tail, BLOCK_SIZE) - new_tail < 4 {
                break;
            }
            j.seek(new_tail)?;
            let fsize = j.file.read_u32::<LittleEndian>()?;
            if fsize == 0 {
                break; // last entry in the block
            }
            // FIXME next_tx is not neccessarily correct, it may be leftover data from the previous block, I need to validate this entry.
            let next_tx = j.file.read_u32::<LittleEndian>()?;
            //println!("Next tx {} old tx {}", next_tx, tx_max);
            // Allows for overflow to happen
            if next_tx != tx_max + 1 {
                break;
            }
            tx_max = next_tx;
            new_tail += fsize as u64;
        }

        j.header.tail = new_tail;
        j.header.trans_ctr = tx_max + 1;

        debug!(j.header);

        Ok(j)
    }

    pub fn write_entry<T: Serialize>(&mut self, entry: T) -> Result<(), Error> {
        const ZERO_SIZE: [u8; 4] = [0; 4];
        let mut e = JournalEntry {
            fsize: 0,
            trans_id: self.header.trans_ctr as u32,
            inner: entry,
            crc32: 0,
        };
        let esize = serialized_size(&e).map_err(|e| Error::new(ErrorKind::Other, e))?;

        //println!("Writing to journal {}", esize);

        assert!(
            esize < BLOCK_SIZE as u64,
            "Journal is too small for this entry"
        );

        e.fsize = esize as u32;
        // println!(
        // "esize {}, tail {}, head {}",
        // esize, self.header.tail, self.header.head
        // );

        //debug!(self.header.tail, align_up(self.header.tail, BLOCK_SIZE));

        let space_in_block = align_up_always(self.header.tail, BLOCK_SIZE) - self.header.tail;

        if space_in_block < esize {
            if space_in_block >= 4 {
                //println!("Writing zero {}", self.header.tail);
                self.file
                    .write_all_at(&ZERO_SIZE[..], self.file_off(self.header.tail))?;
            }
            self.header.tail = align_up_always(self.header.tail, BLOCK_SIZE as u64);
            // Journal is full, will move head
            if self.header.head + self.size == self.header.tail {
                self.header.head += BLOCK_SIZE as u64;
            }
            self.write_header()?;
        }

        if self.sbuf.capacity() < esize as usize {
            self.sbuf = Vec::with_capacity(esize as usize);
        }

        unsafe { self.sbuf.set_len(0) };

        serialize_into(&mut self.sbuf, &e).map_err(|e| Error::new(ErrorKind::Other, e))?;

        let nlen = self.sbuf.len() - 4;
        unsafe { self.sbuf.set_len(nlen) };

        let crc32 = crc32::checksum_ieee(&self.sbuf[..]);
        self.sbuf.write_u32::<LittleEndian>(crc32)?;

        self.file
            .write_all_at(&self.sbuf, self.file_off(self.header.tail))?;
        self.header.tail += esize;
        self.header.trans_ctr += 1;

        if self.sync {
            self.flush()?;
        }

        Ok(())
    }

    fn seek(&mut self, off: u64) -> Result<(), Error> {
        let o = self.file_off(off);
        self.file.seek(SeekFrom::Start(o))?;
        Ok(())
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
            block_buffer: Vec::new(),
        }
    }
    pub fn read_reverse<T>(&mut self) -> JournalIterator<Reverse, T> {
        JournalIterator {
            direction: PhantomData,
            inner_t: PhantomData,
            header: self.header.clone(),
            journal: self,
            block_buffer: Vec::new(),
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
        f.set_len(1024 + *HEADER_SIZE)?;
        let mut j = Journal::new(f, false)?;
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
        Journal::new(f, false)
    }
    let buf = [1; 4197];
    let mut j = inner().unwrap();
    b.iter(|| j.write_entry(&buf[..]).expect("Write failed"));
}
