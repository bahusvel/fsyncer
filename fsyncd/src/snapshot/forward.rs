use bincode::{deserialize_from, serialize_into, serialized_size};
use common::*;
use error::*;
use std::collections::{btree_map, hash_map, BTreeMap, HashMap};
use std::fs;
use std::io;
//use std::ops::Drop;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Block {
    location: u64,
    size: usize,
}

// impl Drop for Block {
//     fn drop(&mut self) {
//         panic!("Block leaked {:?}", self);
//     }
// }

#[derive(Serialize, Deserialize, Clone, Debug)]
struct DataList(BTreeMap<usize, Block>);

impl DataList {
    fn new() -> Self {
        DataList(BTreeMap::new())
    }
    fn shared(other: &DataList) -> Self {
        DataList(other.0.clone())
    }
    // fn find_gaps(&self) {
    //     let mut last_end = 0;
    //     for block in self.0.iter() {
    //         if *block.0 != last_end {
    //             eprintln!(
    //                 "Gap {}-{} ({} bytes)!",
    //                 last_end,
    //                 block.0,
    //                 block.0 - last_end
    //             );
    //         }
    //         last_end = block.0 + block.1.size;
    //     }
    // }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
enum FileType {
    Invalid,
    Opened,
    Directory,
    New(i32), // flags
    Special(u64),
    Symlink(PathBuf), /* Symlinks are resolved for any data operations,
                       * therefore it has no data, just metadata */
    Hardlink(PathBuf), /* A hardlink may refer to a file that is included
                        * in this snapshot, in which case as an
                        * optimisation, all data is to be stored there,
                        * otherwise these are the writes performed on that
                        * hardlink */
    Moved(PathBuf),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
enum FileSize {
    Unknown,
    MoreThan(u64),
    Exactly(u64),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct File {
    uid: Option<u32>,
    gid: Option<u32>,
    mode: Option<u32>,
    size: FileSize,
    xattrs: Option<HashMap<Vec<u8>, Option<Vec<u8>>>>,
    time: Option<[Timespec; 3]>,
    ty: FileType,
    data: Option<DataList>,
}

impl Default for File {
    fn default() -> Self {
        File {
            uid: None,
            gid: None,
            mode: None,
            size: FileSize::Unknown,
            xattrs: None,
            time: None,
            ty: FileType::Invalid,
            data: None,
        }
    }
}

#[derive(Serialize, Deserialize)]
enum SnapshotType {
    Forward,
    Bidirectional,
    Undo,
}

#[derive(Serialize, Deserialize)]
struct Header {
    ty: SnapshotType,
    ft_offset: u64,
    from: SystemTime,
    to: SystemTime,
    checksum: u32,
}

pub struct Snapshot {
    header: Header,
    files: BTreeMap<PathBuf, File>,
    serialised: fs::File,
    free_list: BTreeMap<u64, usize>,
}

macro_rules! set_fields {
    ($val:expr => { $(.$field:ident: $field_val:expr,)* }) => {
        {
            let mut v = $val;
            $(
                v.$field = $field_val;
            )*
            v
        }
    };
}

// #[cfg(target_os = "linux")]
// fn move_file_range(
//     file: &mut fs::File,
//     from: u64,
//     to: u64,
//     size: usize,
// ) -> Result<(), io::Error> {
//     use std::os::unix::io::AsRawFd;
//     let fd = file.as_raw_fd();
//     // Splice is not for files, use copy_file_range
//     let ret = unsafe {
//         libc::splice(
//             fd,
//             &from as *const _ as _,
//             fd,
//             &to as *const _ as _,
//             size,
//             0,
//         )
//     };
//     if ret == -1 {
//         return Err(io::Error::last_os_error());
//     }
//     assert!(ret == size as isize);
//     Ok(())
// }

fn hash_reader<R: io::Read>(mut reader: R) -> Result<u32, io::Error> {
    use crc::crc32::{update, IEEE_TABLE};
    let mut buffer = [0; 4096];
    let mut digest = 0;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest = update(digest, &IEEE_TABLE, &buffer[..count]);
    }
    Ok(digest)
}

impl Snapshot {
    pub fn new(to_file: fs::File) -> Self {
        let header = Header {
            ty: SnapshotType::Forward,
            ft_offset: 0,
            from: SystemTime::now(),
            to: SystemTime::now(),
            checksum: 0,
        };
        let mut free_list = BTreeMap::new();
        free_list.insert(serialized_size(&header).unwrap(), std::usize::MAX);
        Snapshot {
            header,
            files: BTreeMap::new(),
            serialised: to_file,
            free_list: free_list,
        }
    }
    pub fn open(mut from_file: fs::File) -> Result<Self, Error<io::Error>> {
        use std::io::{Seek, SeekFrom};
        trace!(from_file.seek(SeekFrom::Start(0)));
        let header: Header = trace!(deserialize_from(&mut from_file)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e)));

        trace!(from_file.seek(SeekFrom::Start(header.ft_offset)));
        let files = trace!(deserialize_from(&mut from_file)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e)));
        let mut free_list = BTreeMap::new();
        free_list.insert(header.ft_offset, std::usize::MAX);

        //debug!(files);

        Ok(Snapshot {
            header,
            serialised: from_file,
            free_list,
            files,
        })
    }
    fn get_or_open<'a>(
        files: &'a mut BTreeMap<PathBuf, File>,
        path: &Path,
    ) -> &'a mut File {
        files
            .entry(path.into())
            .or_insert(set_fields!(File::default() => {
            .ty: FileType::Opened,
            .data: Some(DataList::new()),
            }))
    }
    fn allocate(
        free_list: &mut BTreeMap<u64, usize>,
        size: usize,
        hint: Option<u64>,
    ) -> Block {
        let (location, entry_size) = hint
            .and_then(|hint| {
                free_list
                    .get(&hint)
                    .filter(|v| **v >= size)
                    .map(|v| (hint, *v))
            })
            .unwrap_or_else(|| {
                let otherwise = free_list
                    .iter()
                    .find(|(_, v)| **v >= size)
                    .map(|(k, v)| (*k, *v));
                if otherwise.is_none() {
                    debug!(free_list, size);
                }
                otherwise.unwrap()
            });
        free_list.remove(&location);
        if entry_size > size {
            free_list.insert(location + size as u64, entry_size - size);
        }
        Block { location, size }
    }
    fn deallocate(free_list: &mut BTreeMap<u64, usize>, block: Block) {
        let Block {
            location: mut offset,
            mut size,
        } = block;
        std::mem::forget(block);
        if let Some(prev) =
            free_list.range(..offset).next_back().map(|(k, v)| (*k, *v))
        {
            if prev.0 + prev.1 as u64 == offset {
                free_list.remove(&prev.0);
                offset = prev.0;
                size += prev.1;
            }
        }
        if let Some(next_size) = free_list.get(&(offset + size as u64)).copied()
        {
            free_list.remove(&(offset + size as u64));
            size += next_size;
        }
        free_list.insert(offset, size);
    }
    fn move_file_range(
        file: &mut fs::File,
        from: u64,
        to: u64,
        size: usize,
    ) -> Result<(), io::Error> {
        use std::cmp::min;
        const BUF_SIZE: usize = 16 * 1024;
        let mut buf = [0; BUF_SIZE];
        for offset in (0..size).step_by(BUF_SIZE) {
            let len = min(size - offset, BUF_SIZE);
            file.read_exact_at(&mut buf[..len], from + offset as u64)?;
            file.write_all_at(&buf[..len], to + offset as u64)?;
        }
        Ok(())
    }

    /*
    This is the most complex part of the algorithm.
    There are a couple of parameters to optimise for:
    1) Fragmentation of the file regions should be kept to a minimum, this is neccessary to produce reasonable recovery and snapshot generation speeds.
    2) Free space fragmentation should be kept to a minimum, this is to produce snapshots of small enough size. In other words fragmentation of the serialised snapshot.

    First defragmentation will be done here, during write, best effort will be made to find overlapping and adjacent blocks in terms of their file offset and place them contiguously in the serialised file.
    */
    fn encode_write(
        &mut self,
        path: &Path,
        offset: usize,
        buff: &[u8],
    ) -> Result<(), Error<io::Error>> {
        const MERGE_LIMIT: usize = 1024 * 1024;
        let data = &mut Snapshot::get_or_open(&mut self.files, &path)
            .data
            .as_mut()
            .expect("Cannot encode write when there is no data")
            .0;

        // Previous file data ranges that overlap this write, rust is very
        // elegant here.
        let mut overlaps: Vec<(usize, Block)> = data
            .range(..=offset + buff.len())
            .rev()
            .take_while(|(k, v)| *k + v.size >= offset)
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        overlaps.reverse();

        if overlaps.len() == 1 {
            let first = &overlaps[0];
            if first.0 <= offset
                && first.0 + first.1.size >= offset + buff.len()
            {
                // This write fits completely into another write, just write the
                // data there
                let location = first.1.location + (offset - first.0) as u64;
                // for overlap in overlaps {
                //     std::mem::forget(overlap)
                // }
                trace!(self
                    .serialised
                    .write_all_at(buff, location as u64));
                return Ok(());
            } else if first.0 < offset {
                /*This write is just after another write, possibly with some
                overlap, attempt to allocate next to it to merge with last
                block*/
                let overlap_size = first.0 + first.1.size - offset;
                let block = Snapshot::allocate(
                    &mut self.free_list,
                    buff.len() - overlap_size,
                    Some(first.1.location + first.1.size as u64),
                );
                if block.location == first.1.location + first.1.size as u64 {
                    //debug!(path, first, block, offset, buff.len());
                    trace!(self.serialised.write_all_at(
                        buff,
                        block.location - overlap_size as u64
                    ));
                    data.get_mut(&first.0).unwrap().size += block.size;
                    std::mem::forget(block);
                    return Ok(());
                } else {
                    Snapshot::deallocate(&mut self.free_list, block);
                }
            }
        }

        // Don't do front and back merges if they are too big
        if let Some(first) = overlaps.first() {
            if first.1.size > MERGE_LIMIT && first.0 + first.1.size == offset {
                overlaps.remove(0);
            }
        }
        if let Some(last) = overlaps.last() {
            if last.1.size > MERGE_LIMIT && last.0 == offset + buff.len() {
                overlaps.remove(overlaps.len() - 1);
            }
        }

        if overlaps.is_empty() {
            let block =
                Snapshot::allocate(&mut self.free_list, buff.len(), None);
            trace!(self.serialised.write_all_at(buff, block.location));
            data.insert(offset, block);
            return Ok(());
        }

        let first = overlaps.first().unwrap();
        let last = overlaps.last().unwrap();
        // Need to defragment, defragmentation strategy is to copy and
        // deallocate overlaps
        let first_exclusive = if offset > first.0 {
            offset - first.0
        } else {
            0
        };
        let last_exclusive = if last.0 + last.1.size > offset + buff.len() {
            (last.0 + last.1.size) - (offset + buff.len())
        } else {
            0
        };

        let need_space = first_exclusive + buff.len() + last_exclusive;
        // Allocation must happen first to avoid overwriting blocks if they are
        // out of order
        let block = Snapshot::allocate(&mut self.free_list, need_space, None);
        if first_exclusive != 0 {
            trace!(Snapshot::move_file_range(
                &mut self.serialised,
                first.1.location,
                block.location,
                first_exclusive,
            ));
        }
        trace!(self
            .serialised
            .write_all_at(buff, block.location + first_exclusive as u64));

        if last_exclusive != 0 {
            trace!(Snapshot::move_file_range(
                &mut self.serialised,
                last.1.location + (last.1.size - last_exclusive) as u64,
                block.location + (first_exclusive + buff.len()) as u64,
                last_exclusive,
            ));
        }
        let logical_offset = std::cmp::min(first.0, offset);
        for overlap in overlaps {
            Snapshot::deallocate(
                &mut self.free_list,
                data.remove(&overlap.0).unwrap(),
            );
            std::mem::forget(overlap);
        }

        data.insert(logical_offset, block);

        Ok(())
    }
    pub fn merge_from<'a, I: Iterator<Item = VFSCall<'a>>>(
        &mut self,
        iter: I,
    ) -> Result<(), Error<io::Error>> {
        for call in iter {
            match call {
                VFSCall::rename { from, to, .. } => {
                    let from_file = self
                        .files
                        .remove(&from as &Path)
                        .unwrap_or(set_fields!(File::default() => {
                            .ty: FileType::Moved(from.into_owned()),
                            .data: Some(DataList::new()),
                        }));
                    self.files
                        .insert(to.into_owned(), from_file)
                        .and_then(|f| f.data)
                        .map_or((), |d| {
                            for (_, block) in d.0 {
                                Snapshot::deallocate(
                                    &mut self.free_list,
                                    block,
                                );
                            }
                        });
                }
                VFSCall::mknod {
                    path,
                    mode,
                    rdev,
                    security: FileSecurity::Unix { uid, gid },
                } => {
                    assert!(self
                        .files
                        .insert(
                            path.into_owned(),
                            set_fields!(File::default() => {
                                .uid: Some(uid),
                                .gid: Some(gid),
                                .mode: Some(mode),
                                .ty: FileType::Special(rdev),
                                .data: Some(DataList::new()),
                            }),
                        )
                        .is_none());
                }
                VFSCall::mkdir {
                    path,
                    mode,
                    security: FileSecurity::Unix { uid, gid },
                } => {
                    assert!(self
                        .files
                        .insert(
                            path.into_owned(),
                            set_fields!(File::default() => {
                                .uid: Some(uid),
                                .gid: Some(gid),
                                .mode: Some(mode),
                                .ty: FileType::Directory,
                            }),
                        )
                        .is_none());
                }
                VFSCall::symlink {
                    from,
                    to,
                    security: FileSecurity::Unix { uid, gid },
                } => {
                    assert!(self
                        .files
                        .insert(
                            to.into_owned(),
                            set_fields!(File::default() => {
                                .uid: Some(uid),
                                .gid: Some(gid),
                                .ty: FileType::Symlink(from.into_owned()),
                            }),
                        )
                        .is_none());
                }
                VFSCall::link {
                    from,
                    to,
                    security: FileSecurity::Unix { uid, gid },
                } => {
                    let data = self
                        .files
                        .get(&from as &Path)
                        .map(|f| {
                            DataList::shared(f.data.as_ref().expect(
                                "Hardlink from is not a regualar file?",
                            ))
                        })
                        .unwrap_or_else(DataList::new);
                    assert!(self
                        .files
                        .insert(
                            to.into_owned(),
                            set_fields!(File::default() => {
                                .uid: Some(uid),
                                .gid: Some(gid),
                                .ty: FileType::Hardlink(from.into_owned()),
                                .data: Some(data),
                            }),
                        )
                        .is_none());
                }
                VFSCall::create {
                    path,
                    mode,
                    flags,
                    security: FileSecurity::Unix { uid, gid },
                } => {
                    self.files.insert(
                        path.into_owned(),
                        set_fields!(File::default() => {
                            .uid: Some(uid),
                            .gid: Some(gid),
                            .mode: Some(mode),
                            .ty: FileType::New(flags),
                            .data: Some(DataList::new()),
                        }),
                    );
                }
                VFSCall::unlink { path } | VFSCall::rmdir { path } => {
                    let file = self.files.remove(&path as &Path);
                    file.and_then(|f| f.data).map_or((), |d| {
                        for (_, block) in d.0 {
                            Snapshot::deallocate(&mut self.free_list, block);
                        }
                    });
                }
                VFSCall::security {
                    path,
                    security: FileSecurity::Unix { uid, gid },
                } => {
                    let file = Snapshot::get_or_open(&mut self.files, &path);
                    file.uid = Some(uid);
                    file.gid = Some(gid);
                }
                VFSCall::chmod { path, mode } => {
                    Snapshot::get_or_open(&mut self.files, &path).mode =
                        Some(mode);
                }
                VFSCall::truncate { path, size } => {
                    //eprintln!("There was a truncate {}", size);
                    let list = &mut self.free_list;
                    let file = Snapshot::get_or_open(&mut self.files, &path);
                    file.size = FileSize::Exactly(size as u64);
                    if let Some(ref mut d) = file.data.as_mut() {
                        let delete: Vec<usize> =
                            d.0.range(size as usize..)
                                .map(|(k, _)| *k)
                                .collect();
                        for offset in delete {
                            let block = d.0.remove(&offset).unwrap();
                            Snapshot::deallocate(list, block);
                        }
                        d.0.iter_mut().next_back().map_or(
                            (),
                            |(offset, block)| {
                                if offset + block.size > size as usize {
                                    let dealloc_size =
                                        offset + block.size - size as usize;
                                    Snapshot::deallocate(
                                        list,
                                        Block {
                                            location: block.location
                                                + (block.size - dealloc_size)
                                                    as u64,
                                            size: dealloc_size,
                                        },
                                    );
                                    block.size -= dealloc_size;
                                }
                            },
                        );
                    }
                }
                VFSCall::fallocate {
                    path,
                    offset,
                    length,
                    ..
                } => {
                    let size =
                        &mut Snapshot::get_or_open(&mut self.files, &path).size;
                    let nsize = (offset + length) as u64;
                    // This is only valid for posix_fallocate which basically
                    // only extends the file.
                    match size {
                        FileSize::Exactly(s) if *s < nsize => {
                            *size = FileSize::Exactly(nsize)
                        }
                        FileSize::MoreThan(s) if *s < nsize => {
                            *size = FileSize::MoreThan(nsize)
                        }
                        FileSize::Unknown => *size = FileSize::MoreThan(nsize),
                        FileSize::Exactly(_) | FileSize::MoreThan(_) => {}
                    }
                }
                VFSCall::write { path, buf, offset }
                | VFSCall::diff_write { path, buf, offset } => {
                    self.encode_write(&path, offset as usize, &buf)?;
                }
                VFSCall::setxattr {
                    path, name, value, ..
                } => {
                    Snapshot::get_or_open(&mut self.files, &path)
                        .xattrs
                        .get_or_insert(HashMap::new())
                        .insert(
                            name.to_bytes().into(),
                            Some(value.into_owned()),
                        );
                }
                VFSCall::removexattr { path, name } => {
                    Snapshot::get_or_open(&mut self.files, &path)
                        .xattrs
                        .get_or_insert(HashMap::new())
                        .insert(name.to_bytes().into(), None);
                }
                VFSCall::utimens { path, timespec } => {
                    Snapshot::get_or_open(&mut self.files, &path).time =
                        Some(timespec);
                }
                VFSCall::fsync { .. } => {} // fsync is not snapshotted
                VFSCall::truncating_write { .. } => {
                    panic!("This is a bullshit vfscall")
                }
                _ => panic!("Not handled, maybe windows stuff?"),
            }
        }
        Ok(())
    }
    pub fn compact(&mut self) -> Result<(), Error<io::Error>> {
        for (_, file) in self.files.iter_mut() {
            if file.data.is_none() {
                continue;
            }
            let wasted_space: usize =
                self.free_list.iter().rev().skip(1).map(|(_, v)| v).sum();
            let end_of_data = *self.free_list.iter().next_back().unwrap().0;
            if (wasted_space as f64 / end_of_data as f64) < 0.05 {
                // Don't bother with compaction of wasted space is below 5%
                break;
            }
            //debug!(wasted_space, end_of_data);
            let mut merged_blocks = DataList::new();
            let mut run: (usize, Block) = (
                0,
                Block {
                    location: 0,
                    size: 0,
                },
            );
            for (offset, block) in file.data.take().unwrap().0 {
                if block.location > end_of_data - wasted_space as u64 {
                    let new_block = Snapshot::allocate(
                        &mut self.free_list,
                        block.size,
                        Some(run.1.location + run.1.size as u64),
                    );
                    //debug!(block.location, new_loc);
                    if new_block.location > block.location {
                        eprintln!("Entry would move up!");
                        // Don't let blocks move up
                        Snapshot::deallocate(&mut self.free_list, new_block);
                        merged_blocks.0.insert(offset, block);
                        continue;
                    }
                    trace!(Snapshot::move_file_range(
                        &mut self.serialised,
                        block.location,
                        new_block.location,
                        block.size
                    ));
                    if run.0 == 0 && run.1.location == 0 && run.1.size == 0 {
                        run = (offset, new_block);
                    } else if run.0 + run.1.size == offset
                        && run.1.location + run.1.size as u64
                            == new_block.location
                    {
                        run.1.size += block.size;
                    } else {
                        merged_blocks.0.insert(run.0, run.1);
                        run = (offset, new_block)
                    }
                    Snapshot::deallocate(&mut self.free_list, block);
                } else {
                    merged_blocks.0.insert(offset, block);
                }
            }
            if run.0 != 0 {
                merged_blocks.0.insert(run.0, run.1);
            }
            file.data = Some(merged_blocks);
        }
        Ok(())
    }
    pub fn finalize(mut self) -> Result<(), Error<io::Error>> {
        use std::io::{BufWriter, Seek, SeekFrom};

        // for file in self.files.iter() {
        //     if let Some(data) = &file.1.data {
        //         if data.0.iter().count() > 99 {
        //             eprintln!("Gaps for file {:?}", file);
        //             data.find_gaps();
        //         }
        //     }
        // }

        trace!(self.compact());
        // debug!(self.free_list);
        //debug!(self.files);

        for file in self.files.iter() {
            eprintln!(
                "{:?} {} blocks",
                file.0,
                file.1.data.as_ref().map_or(0, |d| d.0.iter().count())
            );
        }

        let end_of_data = *self.free_list.iter().next_back().unwrap().0;
        trace!(self.serialised.set_len(end_of_data));

        trace!(self.serialised.seek(SeekFrom::Start(end_of_data)));
        let writer = BufWriter::new(&mut self.serialised);
        trace!(serialize_into(writer, &self.files)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e)));

        trace!(self
            .serialised
            .seek(SeekFrom::Start(serialized_size(&self.header).unwrap())));
        self.header.checksum = trace!(hash_reader(&mut self.serialised));
        self.header.ft_offset = end_of_data;

        trace!(self.serialised.seek(SeekFrom::Start(0)));
        trace!(serialize_into(&mut self.serialised, &self.header)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e)));

        Ok(())
    }
    pub fn apply(&self) -> SnapshotApply {
        SnapshotApply {
            serialised: &self.serialised,
            file_iter: self.files.iter(),
            current_file: None,
            data_iter: None,
            xattr_iter: None,
        }
    }
}

pub struct SnapshotApply<'a> {
    serialised: &'a fs::File,
    file_iter: btree_map::Iter<'a, PathBuf, File>,
    current_file: Option<(&'a PathBuf, File)>,
    data_iter: Option<btree_map::Iter<'a, usize, Block>>,
    xattr_iter: Option<hash_map::Iter<'a, Vec<u8>, Option<Vec<u8>>>>,
}

impl<'a> Iterator for SnapshotApply<'a> {
    type Item = VFSCall<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        use std::borrow::Cow;
        if let Some(data_blocks) = &mut self.data_iter {
            if let Some(block) = data_blocks.next() {
                let (cp, _) = self.current_file.as_ref().unwrap();
                let mut buf = Vec::with_capacity(block.1.size);
                unsafe { buf.set_len(block.1.size) };
                self.serialised
                    .read_exact_at(&mut buf, block.1.location)
                    .expect("Failed to read snapshot data");
                return Some(VFSCall::write {
                    path: Cow::Borrowed(cp),
                    offset: *block.0 as i64,
                    buf: Cow::Owned(buf),
                });
            } else {
                self.data_iter = None;
            }
        }
        //debug!(self.current_file);
        if self.current_file.is_none() {
            let (path, file) = self.file_iter.next()?;
            self.xattr_iter = file.xattrs.as_ref().map(|x| x.iter());
            self.data_iter = file.data.as_ref().map(|d| d.0.iter());
            self.current_file = Some((path, file.clone()));
            let (cp, cf) = self.current_file.as_mut().unwrap();
            cf.data = None; // Take the data to avoid wasting memory
            cf.xattrs = None;
            match cf.ty.clone() {
                FileType::Invalid => panic!("Invalid file type"),
                FileType::Opened => {}
                FileType::Directory => {
                    return Some(VFSCall::mkdir {
                        mode: cf.mode.take().unwrap(),
                        path: Cow::Borrowed(cp),
                        security: FileSecurity::Unix {
                            uid: cf.uid.take().unwrap(),
                            gid: cf.gid.take().unwrap(),
                        },
                    })
                }
                FileType::New(flags) => {
                    return Some(VFSCall::create {
                        flags,
                        mode: cf.mode.take().unwrap(),
                        path: Cow::Borrowed(cp),
                        security: FileSecurity::Unix {
                            uid: cf.uid.take().unwrap(),
                            gid: cf.gid.take().unwrap(),
                        },
                    })
                }
                FileType::Special(rdev) => {
                    return Some(VFSCall::mknod {
                        mode: cf.mode.take().unwrap(),
                        path: Cow::Borrowed(cp),
                        security: FileSecurity::Unix {
                            uid: cf.uid.take().unwrap(),
                            gid: cf.gid.take().unwrap(),
                        },
                        rdev,
                    })
                }
                FileType::Symlink(from) => {
                    return Some(VFSCall::symlink {
                        from: Cow::Owned(from),
                        to: Cow::Borrowed(cp),
                        security: FileSecurity::Unix {
                            uid: cf.uid.take().unwrap(),
                            gid: cf.gid.take().unwrap(),
                        },
                    })
                }
                FileType::Hardlink(from) => {
                    return Some(VFSCall::link {
                        from: Cow::Owned(from),
                        to: Cow::Borrowed(cp),
                        security: FileSecurity::Unix {
                            uid: cf.uid.take().unwrap(),
                            gid: cf.gid.take().unwrap(),
                        },
                    })
                }
                FileType::Moved(from) => {
                    return Some(VFSCall::rename {
                        from: Cow::Owned(from),
                        to: Cow::Borrowed(cp),
                        flags: 0,
                    })
                }
            }
        }
        let (cp, cf) = self.current_file.as_mut().unwrap();
        if let (Some(uid), Some(gid)) = (cf.uid.take(), cf.gid.take()) {
            return Some(VFSCall::security {
                path: Cow::Borrowed(cp),
                security: FileSecurity::Unix { uid, gid },
            });
        }
        if let Some(mode) = cf.mode.take() {
            return Some(VFSCall::chmod {
                path: Cow::Borrowed(cp),
                mode,
            });
        }
        if let FileSize::Exactly(tsize) = cf.size {
            cf.size = FileSize::Unknown;
            return Some(VFSCall::truncate {
                path: Cow::Borrowed(cp),
                size: tsize as i64,
            });
        }
        if let FileSize::MoreThan(fsize) = cf.size {
            cf.size = FileSize::Unknown;
            return Some(VFSCall::fallocate {
                path: Cow::Borrowed(cp),
                length: fsize as i64,
                offset: 0,
                mode: 0,
            }); // HACK, I don't think this is good.
        }
        if let Some(timespec) = cf.time.take() {
            return Some(VFSCall::utimens {
                path: Cow::Borrowed(cp),
                timespec,
            });
        }
        if let Some(xattrs) = &mut self.xattr_iter {
            use std::ffi::CStr;
            if let Some((k, v)) = xattrs.next() {
                if let Some(value) = v {
                    return Some(VFSCall::setxattr {
                        path: Cow::Borrowed(cp),
                        name: Cow::Borrowed(
                            CStr::from_bytes_with_nul(&k).unwrap(),
                        ),
                        value: Cow::Borrowed(value),
                        flags: 0,
                    });
                } else {
                    return Some(VFSCall::removexattr {
                        path: Cow::Borrowed(cp),
                        name: Cow::Borrowed(
                            CStr::from_bytes_with_nul(&k).unwrap(),
                        ),
                    });
                }
            } else {
                self.xattr_iter = None;
            }
        }
        self.current_file = None;
        self.next()
    }
}
