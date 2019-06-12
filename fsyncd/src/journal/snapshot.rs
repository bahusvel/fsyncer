use common::*;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Error;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
struct Block {
    location: u64,
    size: usize,
}

struct DataList(BTreeMap<usize, Block>);
impl DataList {
    fn new() -> Self {
        DataList(BTreeMap::new())
    }
    fn shared(other: &DataList) -> Self {
        DataList(other.0.clone())
    }
}

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

struct File {
    uid: Option<u32>,
    gid: Option<u32>,
    mode: Option<u32>,
    size: Option<usize>,
    xattrs: Option<HashMap<Vec<u8>, Option<Vec<u8>>>>,
    time: Option<[enc_timespec; 3]>,
    ty: FileType,
    data: Option<DataList>,
}

impl Default for File {
    fn default() -> Self {
        File {
            uid: None,
            gid: None,
            mode: None,
            size: None,
            xattrs: None,
            time: None,
            ty: FileType::Invalid,
            data: None,
        }
    }
}

struct Snapshot {
    files: HashMap<PathBuf, File>,
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

struct ReverseIterator<T: DoubleEndedIterator>(T);
impl<T: DoubleEndedIterator> Iterator for ReverseIterator<T> {
    type Item = T::Item;
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next_back()
    }
}

fn move_file_range(
    file: &mut fs::File,
    from: u64,
    to: u64,
    size: usize,
) -> Result<(), Error> {
    use std::cmp::min;
    const BUF_SIZE: usize = 4096;
    let mut buf = [0; BUF_SIZE];
    for offset in (from..from + size as u64).step_by(BUF_SIZE) {
        let len = min(from as usize + size - offset as usize, BUF_SIZE);
        file.read_exact_at(&mut buf[..len], offset)?;
        file.write_all_at(&mut buf[..len], offset - from + to)?;
    }
    Ok(())
}

impl Snapshot {
    pub fn new(to_file: fs::File) -> Self {
        let mut free_list = BTreeMap::new();
        free_list.insert(0, std::usize::MAX);
        Snapshot {
            files: HashMap::new(),
            serialised: to_file,
            free_list: free_list,
        }
    }
    fn get_or_open(&mut self, path: &Path) -> &mut File {
        self.files.entry(path.into()).or_insert(
            set_fields!(File::default() => {
            .ty: FileType::Opened,
            .data: Some(DataList::new()),
            }),
        )
    }
    fn allocate(&mut self, size: usize) -> u64 {
        let offset = *self
            .free_list
            .iter()
            .find(|(_, v)| **v >= size)
            .take()
            .unwrap()
            .0;
        let entry_size = self.free_list.remove(&offset).unwrap();
        if entry_size > size {
            self.free_list
                .insert(offset + size as u64, entry_size - size);
        }
        offset
    }
    fn deallocate(&mut self, mut offset: u64, mut size: usize) {
        let prev = self
            .free_list
            .range(..offset)
            .next_back()
            .map(|(k, v)| (*k, *v));
        if prev.is_some() {
            let prev = prev.unwrap();
            if prev.0 + prev.1 as u64 == offset {
                self.free_list.remove(&prev.0);
                offset = prev.0;
                size += prev.1;
            }
        }
        let next = self
            .free_list
            .range(offset + size as u64..)
            .next()
            .map(|(k, v)| (*k, *v));
        if next.is_some() {
            let next = next.unwrap();
            if next.0 == offset + size as u64 {
                self.free_list.remove(&next.0);
                size += next.1;
            }
        }
        self.free_list.insert(offset, size);
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
    ) -> Result<(), Error> {
        let file = self.get_or_open(&path);
        //let data =

        // Previous file data ranges that overlap this write, rust is very
        // elegant here.
        let overlaps: Vec<(usize, Block)> = ReverseIterator(
            file.data
                .as_mut()
                .expect("Cannot encode write when there is no data")
                .0
                .range(..offset + buff.len()),
        )
        .take_while(|(k, v)| *k + v.size > offset)
        .map(|(k, v)| (*k, *v))
        .collect();

        if overlaps.len() == 0 {
            // No overlap simply allocate from free_list and write
            let location = self.allocate(buff.len());
            self.get_or_open(path).data.as_mut().unwrap().0.insert(
                offset,
                Block {
                    location,
                    size: buff.len(),
                },
            );
            return self.serialised.write_all_at(buff, location as u64);
        }
        let first = overlaps.first().unwrap();
        let last = overlaps.last().unwrap();
        if overlaps.len() == 1
            && first.0 <= offset
            && first.1.size >= buff.len()
        {
            let location = first.1.location + (offset - first.0) as u64;
            return self.serialised.write_all_at(buff, location as u64);
        }

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
        let location = self.allocate(need_space);
        if first_exclusive != 0 {
            move_file_range(
                &mut self.serialised,
                first.1.location,
                location,
                first_exclusive,
            )?;
        }
        self.serialised
            .write_all_at(buff, location + first_exclusive as u64)?;
        if last_exclusive != 0 {
            move_file_range(
                &mut self.serialised,
                last.1.location + (last.1.size - last_exclusive) as u64,
                location + (first_exclusive + buff.len()) as u64,
                last_exclusive,
            )?;
        }
        let data_blocks = &mut self.get_or_open(path).data.as_mut().unwrap().0;
        for overlap in overlaps.iter() {
            data_blocks.remove(&overlap.0);
        }
        data_blocks.insert(
            first.0,
            Block {
                location,
                size: need_space,
            },
        );
        for overlap in overlaps.iter() {
            self.deallocate(overlap.1.location, overlap.1.size);
        }
        Ok(())
    }
    pub fn merge_from<'a, I: Iterator<Item = VFSCall<'a>>>(
        &mut self,
        iter: I,
    ) -> Result<(), Error> {
        for call in iter {
            match call {
                VFSCall::rename(rename { from, to, .. }) => {
                    let from_file = self
                        .files
                        .remove(&from as &Path)
                        .unwrap_or(set_fields!(File::default() => {
                            .ty: FileType::Moved(from.into_owned()),
                            .data: Some(DataList::new()),
                        }));
                    self.files.insert(to.into_owned(), from_file);
                }
                VFSCall::mknod(mknod {
                    path,
                    mode,
                    rdev,
                    security: FileSecurity::Unix { uid, gid },
                }) => {
                    self.files.insert(
                        path.into_owned(),
                        set_fields!(File::default() => {
                            .uid: Some(uid),
                            .gid: Some(gid),
                            .mode: Some(mode),
                            .ty: FileType::Special(rdev),
                            .data: Some(DataList::new()),
                        }),
                    );
                }
                VFSCall::mkdir(mkdir {
                    path,
                    mode,
                    security: FileSecurity::Unix { uid, gid },
                }) => {
                    self.files.insert(
                        path.into_owned(),
                        set_fields!(File::default() => {
                            .uid: Some(uid),
                            .gid: Some(gid),
                            .mode: Some(mode),
                            .ty: FileType::Directory,
                        }),
                    );
                }
                VFSCall::symlink(symlink {
                    from,
                    to,
                    security: FileSecurity::Unix { uid, gid },
                }) => {
                    self.files.insert(
                        to.into_owned(),
                        set_fields!(File::default() => {
                            .uid: Some(uid),
                            .gid: Some(gid),
                            .ty: FileType::Symlink(from.into_owned()),
                        }),
                    );
                }
                VFSCall::link(link {
                    from,
                    to,
                    security: FileSecurity::Unix { uid, gid },
                }) => {
                    let data = self
                        .files
                        .get(&from as &Path)
                        .map(|f| {
                            DataList::shared(f.data.as_ref().expect(
                                "Hardlink from is not a regualar file?",
                            ))
                        })
                        .unwrap_or(DataList::new());
                    self.files.insert(
                        to.into_owned(),
                        set_fields!(File::default() => {
                            .uid: Some(uid),
                            .gid: Some(gid),
                            .ty: FileType::Hardlink(from.into_owned()),
                            .data: Some(data),
                        }),
                    );
                }
                VFSCall::create(create {
                    path,
                    mode,
                    flags,
                    security: FileSecurity::Unix { uid, gid },
                }) => {
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
                VFSCall::unlink(unlink { path })
                | VFSCall::rmdir(rmdir { path }) => {
                    let file = self.files.remove(&path as &Path);
                    file.and_then(|f| f.data).map_or({}, |d| {
                        for (_, block) in d.0 {
                            self.deallocate(block.location, block.size);
                        }
                    });
                }
                VFSCall::security(security {
                    path,
                    security: FileSecurity::Unix { uid, gid },
                }) => {
                    let file = self.get_or_open(&path);
                    file.uid = Some(uid);
                    file.gid = Some(gid);
                }
                VFSCall::chmod(chmod { path, mode }) => {
                    self.get_or_open(&path).mode = Some(mode);
                }
                VFSCall::truncate(truncate { path, size }) => {
                    self.get_or_open(&path).size = Some(size as usize);
                    // TODO delete any data past the size offset
                }
                VFSCall::fallocate(fallocate {
                    path,
                    mode,
                    offset,
                    length,
                }) => {
                    // TODO implement snapshot for fallocate
                }
                VFSCall::write(write { path, buf, offset })
                | VFSCall::diff_write(write { path, buf, offset }) => {
                    self.encode_write(&path, offset as usize, &buf)?;
                }
                VFSCall::setxattr(setxattr {
                    path, name, value, ..
                }) => {
                    self.get_or_open(&path)
                        .xattrs
                        .get_or_insert(HashMap::new())
                        .insert(
                            name.to_bytes().into(),
                            Some(value.into_owned()),
                        );
                }
                VFSCall::removexattr(removexattr { path, name }) => {
                    self.get_or_open(&path)
                        .xattrs
                        .get_or_insert(HashMap::new())
                        .insert(name.to_bytes().into(), None);
                }
                VFSCall::utimens(utimens { path, timespec }) => {
                    self.get_or_open(&path).time = Some(timespec);
                }
                VFSCall::fsync(_) => {} // fsync is not snapshotted
                VFSCall::truncating_write { .. } => {
                    panic!("This is a bullshit vfscall")
                }
                _ => panic!("Not handled, maybe windows stuff?"),
            }
        }
        Ok(())
    }
    pub fn finalize(self) -> Result<(), Error> {
        // Serialise the table to the end of the file
        // Write table offset to the header
        Ok(())
    }
}
