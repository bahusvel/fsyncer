use bincode::{deserialize_from, serialize_into, serialized_size};
use byteorder::{LittleEndian, ReadBytesExt};
use std::fs::File;
use std::io::{Error, ErrorKind, Seek, SeekFrom, Write};
use std::marker::PhantomData;
use std::mem::size_of;
use std::sync::atomic::AtomicUsize;

static TRANS_CTR: AtomicUsize = AtomicUsize::new(0);
const ZERO_SIZE: [u8; 4] = [0; 4];

#[derive(Clone)]
struct JournaHeader {
    tail: u64,
    head: u64,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
struct JournalEntry {
    fsize: u32,
    trans_id: u32,
    entry: Vec<u8>,
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

pub struct JournalIterator<'a, D: Direction> {
    phantom: PhantomData<D>,
    header: JournaHeader,
    journal: &'a mut Journal,
}

impl<'a> Iterator for JournalIterator<'a, Forward> {
    type Item = Result<JournalEntry, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.header.head == self.header.tail {
            return None;
        }
        let entry: Result<JournalEntry, _> =
            deserialize_from(&mut self.journal.file).map_err(|e| Error::new(ErrorKind::Other, e));

        match entry {
            Ok(entry) => {
                self.header.head += entry.fsize as u64;
                Some(Ok(entry))
            }
            Err(e) => Some(Err(e)),
        }
    }
}

impl<'a> Iterator for JournalIterator<'a, Reverse> {
    type Item = Result<JournalEntry, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.header.head == self.header.tail {
            return None;
        }

        let err = self
            .journal
            .file
            .seek(SeekFrom::Start((self.header.tail - 4) % self.journal.size));
        if err.is_err() {
            return Some(Err(err.unwrap_err()));
        }

        let rsize = self.journal.file.read_u32::<LittleEndian>();
        if rsize.is_err() {
            return Some(Err(rsize.unwrap_err()));
        }
        let rsize = rsize.unwrap();

        let err = self.journal.file.seek(SeekFrom::Current(-(rsize as i64)));
        if err.is_err() {
            return Some(Err(err.unwrap_err()));
        }

        let entry: Result<JournalEntry, _> =
            deserialize_from(&mut self.journal.file).map_err(|e| Error::new(ErrorKind::Other, e));

        match entry {
            Ok(entry) => {
                self.header.tail -= rsize as u64;
                Some(Ok(entry))
            }
            Err(e) => Some(Err(e)),
        }
    }
}

impl Journal {
    fn write_entry(&mut self, e: &JournalEntry) -> Result<(), Error> {
        let esize = serialized_size(e).map_err(|e| Error::new(ErrorKind::Other, e))?;
        let pad = if self.size - (self.header.tail % self.size) < esize {
            // The write would overlap the right boundary of ring buffer.
            self.size - (self.header.tail % self.size)
        } else {
            0
        };
        let mut free_space = self.size - (self.header.tail - self.header.head);
        while free_space <= esize + pad {
            free_space += self.seek(1)?;
        }
        self.file
            .seek(SeekFrom::Start(self.header.tail % self.size))?;
        if pad != 0 {
            self.file
                .write(&ZERO_SIZE[..if pad < 4 { pad as usize } else { 4 }])?;
            self.file
                .seek(SeekFrom::Start((self.header.tail + pad) % self.size))?;
        }
        serialize_into(&mut self.file, e).map_err(|e| Error::new(ErrorKind::Other, e))?;
        self.header.tail += esize + pad;
        Ok(())
    }
    fn seek(&mut self, n_entries: isize) -> Result<u64, Error> {
        let old_head = self.header.head;
        for _ in 0..n_entries {
            if self.header.head == self.header.tail {
                return Err(Error::new(ErrorKind::UnexpectedEof, "Journal is empty"));
            }
            let fsize = self.file.read_u32::<LittleEndian>()?;
            self.header.head += fsize as u64;
            self.file.seek(SeekFrom::Start(
                (fsize as u64 - size_of::<u32>() as u64) % self.size,
            ))?;
        }
        Ok(self.header.head - old_head)
    }
    fn flush(&mut self) -> Result<(), Error> {
        self.file.flush()
    }
    fn flush_header() {}

    fn read_forward(&mut self) -> JournalIterator<Forward> {
        JournalIterator {
            phantom: PhantomData,
            header: self.header.clone(),
            journal: self,
        }
    }
    fn read_reverse(&mut self) -> JournalIterator<Reverse> {
        JournalIterator {
            phantom: PhantomData,
            header: self.header.clone(),
            journal: self,
        }
    }
}
