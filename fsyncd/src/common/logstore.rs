use std::fs::File;

struct JournaHeader {
    tail: isize,
    head: isize,
}

struct JournalEntry {
    tid: u32,
    entry: Vec<u8>,
    size: u32,
}

pub struct Journal {
    header: JournaHeader,
    file: File,
}

impl Journal {
    fn write_entry() {}
    fn read_entry() {}
    fn seek(n_entries: isize) {}
    fn flush() {}
    fn flush_header() {}
}
