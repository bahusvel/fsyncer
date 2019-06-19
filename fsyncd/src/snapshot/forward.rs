use bincode::{deserialize_from, serialize_into, serialized_size};
use common::*;
use std::collections::{btree_map, hash_map, BTreeMap, HashMap};
use std::fs;
use std::io::Error;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct Block {
    location: u64,
    size: usize,
}

#[derive(Serialize, Deserialize, Clone)]
struct DataList(BTreeMap<usize, Block>);

impl DataList {
    fn new() -> Self {
        DataList(BTreeMap::new())
    }
    fn shared(other: &DataList) -> Self {
        DataList(other.0.clone())
    }
}

#[derive(Serialize, Deserialize, Clone)]
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

#[derive(Serialize, Deserialize, Clone)]
enum FileSize {
    Unknown,
    MoreThan(u64),
    Exactly(u64),
}

#[derive(Serialize, Deserialize, Clone)]
struct File {
    uid: Option<u32>,
    gid: Option<u32>,
    mode: Option<u32>,
    size: FileSize,
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
}

pub struct Snapshot {
    header: Header,
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

fn move_file_range(
    file: &mut fs::File,
    from: u64,
    to: u64,
    size: usize,
) -> Result<(), Error> {
    use std::cmp::min;
    const BUF_SIZE: usize = 16 * 1024;
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
        let header = Header {
            ty: SnapshotType::Forward,
            ft_offset: 0,
        };
        let mut free_list = BTreeMap::new();
        free_list.insert(serialized_size(&header).unwrap(), std::usize::MAX);
        Snapshot {
            header,
            files: HashMap::new(),
            serialised: to_file,
            free_list: free_list,
        }
    }
    pub fn open(mut from_file: fs::File) -> Result<Self, Error> {
        use std::io::{ErrorKind, Seek, SeekFrom};
        from_file.seek(SeekFrom::Start(0))?;
        let header: Header = deserialize_from(&mut from_file)
            .map_err(|e| Error::new(ErrorKind::Other, e))?;

        from_file.seek(SeekFrom::Start(header.ft_offset))?;
        let files = deserialize_from(&mut from_file)
            .map_err(|e| Error::new(ErrorKind::Other, e))?;
        let mut free_list = BTreeMap::new();
        free_list.insert(header.ft_offset, std::usize::MAX);

        Ok(Snapshot {
            header,
            serialised: from_file,
            free_list,
            files,
        })
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
        let (offset, entry_size) = self
            .free_list
            .iter()
            .find(|(_, v)| **v >= size)
            .map(|(k, v)| (*k, *v))
            .unwrap();
        self.free_list.remove(&offset);
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
        let overlaps: Vec<(usize, Block)> = file
            .data
            .as_mut()
            .expect("Cannot encode write when there is no data")
            .0
            .range(..offset + buff.len())
            .rev()
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
            // This write fits completely into another write, just write the
            // data there
            let location = first.1.location + (offset - first.0) as u64;
            return self.serialised.write_all_at(buff, location as u64);
        }

        eprintln!("Doing write compaction");

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
                VFSCall::rename { from, to, .. } => {
                    let from_file = self
                        .files
                        .remove(&from as &Path)
                        .unwrap_or(set_fields!(File::default() => {
                            .ty: FileType::Moved(from.into_owned()),
                            .data: Some(DataList::new()),
                        }));
                    self.files.insert(to.into_owned(), from_file);
                }
                VFSCall::mknod {
                    path,
                    mode,
                    rdev,
                    security: FileSecurity::Unix { uid, gid },
                } => {
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
                VFSCall::mkdir {
                    path,
                    mode,
                    security: FileSecurity::Unix { uid, gid },
                } => {
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
                VFSCall::symlink {
                    from,
                    to,
                    security: FileSecurity::Unix { uid, gid },
                } => {
                    self.files.insert(
                        to.into_owned(),
                        set_fields!(File::default() => {
                            .uid: Some(uid),
                            .gid: Some(gid),
                            .ty: FileType::Symlink(from.into_owned()),
                        }),
                    );
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
                VFSCall::unlink { path }
                | VFSCall::rmdir { path } => {
                    let file = self.files.remove(&path as &Path);
                    file.and_then(|f| f.data).map_or({}, |d| {
                        for (_, block) in d.0 {
                            self.deallocate(block.location, block.size);
                        }
                    });
                }
                VFSCall::security{
                    path,
                    security: FileSecurity::Unix { uid, gid },
                } => {
                    let file = self.get_or_open(&path);
                    file.uid = Some(uid);
                    file.gid = Some(gid);
                }
                VFSCall::chmod { path, mode } => {
                    self.get_or_open(&path).mode = Some(mode);
                }
                VFSCall::truncate { path, size } => {
                    let file = self.get_or_open(&path);
                    file.size = FileSize::Exactly(size as u64);
                    //let t: Vec<(u64, usize)> = Vec::new();
                    let mut dealloc = Vec::new();
                    if let Some(ref mut d) = file.data {
                        let delete: Vec<usize> =
                            d.0.range(size as usize..)
                                .map(|(k, _)| *k)
                                .collect();
                        for offset in delete {
                            let block = d.0.remove(&offset).unwrap();
                            dealloc.push((block.location, block.size));
                        }
                        d.0.iter_mut().next_back().map_or(
                            {},
                            |(offset, block)| {
                                if offset + block.size > size as usize {
                                    let dealloc_size =
                                        offset + block.size - size as usize;
                                    dealloc.push((
                                        block.location
                                            + (block.size - dealloc_size)
                                                as u64,
                                        dealloc_size,
                                    ));
                                    block.size -= dealloc_size;
                                }
                            },
                        );
                    }
                    for (offset, size) in dealloc {
                        self.deallocate(offset, size);
                    }
                }
                VFSCall::fallocate {
                    path,
                    offset,
                    length,
                    ..
                } => {
                    let size = &mut self.get_or_open(&path).size;
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
                    self.get_or_open(&path)
                        .xattrs
                        .get_or_insert(HashMap::new())
                        .insert(
                            name.to_bytes().into(),
                            Some(value.into_owned()),
                        );
                }
                VFSCall::removexattr { path, name } => {
                    self.get_or_open(&path)
                        .xattrs
                        .get_or_insert(HashMap::new())
                        .insert(name.to_bytes().into(), None);
                }
                VFSCall::utimens { path, timespec } => {
                    self.get_or_open(&path).time = Some(timespec);
                }
                VFSCall::fsync{..} => {} // fsync is not snapshotted
                VFSCall::truncating_write { .. } => {
                    panic!("This is a bullshit vfscall")
                }
                _ => panic!("Not handled, maybe windows stuff?"),
            }
        }
        Ok(())
    }
    pub fn finalize(mut self) -> Result<(), Error> {
        use std::io::{ErrorKind, Seek, SeekFrom};
        let end_of_data = *self.free_list.iter().next_back().unwrap().0;
        self.header.ft_offset = end_of_data;
        self.serialised.seek(SeekFrom::Start(0))?;
        serialize_into(&mut self.serialised, &self.header)
            .map_err(|e| Error::new(ErrorKind::Other, e))?;
        self.serialised.seek(SeekFrom::Start(end_of_data))?;
        serialize_into(&mut self.serialised, &self.files)
            .map_err(|e| Error::new(ErrorKind::Other, e))?;
        Ok(())
    }
    pub fn apply(&self) -> SnapshotApply {
        SnapshotApply {
            snapshot: &self,
            file_iter: self.files.iter(),
            current_file: None,
            data_iter: None,
            xattr_iter: None,
        }
    }
}

pub struct SnapshotApply<'a> {
    snapshot: &'a Snapshot,
    file_iter: hash_map::Iter<'a, PathBuf, File>,
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
                self.snapshot
                    .serialised
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
                    return Some(VFSCall::symlink{
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
            return Some(VFSCall::chmod{
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
            return Some(VFSCall::fallocate{
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
        None
    }
}
