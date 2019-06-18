use bincode::{deserialize_from, serialize_into, serialized_size};
use common::*;
use intrusive_collections::{
    RBTree, RBTreeLink, LinkedList, LinkedListLink, KeyAdapter, Adapter, rbtree, Bound,
};
use serde::{Serialize, Deserialize, Serializer, Deserializer, ser::SerializeSeq, de::Visitor, de::SeqAccess};
use std::collections::{hash_map, HashMap};
use std::fs;
use std::io::Error;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::fmt;

#[derive(Debug, Clone)]
enum BlockStatus {
    Free(LinkedListLink),
    Allocated(RBTreeLink, u64),
}

impl Default for BlockStatus {
    fn default() -> Self {
        BlockStatus::Free(LinkedListLink::default())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Block {
    #[serde(skip)]
    status: BlockStatus,
    location: u64,
    size: usize,
}

impl Block {
    fn new(location: u64, size: usize) -> Box<Self> {
        Box::new(Block {
            status: BlockStatus::Free(LinkedListLink::default()),
            location, size
        })
    }
    fn file_offset(&self) -> Option<u64> {
        if let BlockStatus::Allocated(_, offset) = self.status {
            Some(offset)
        } else {
            None
        }
    }
}


struct FreeListAdapter; 
unsafe impl Adapter for FreeListAdapter{
    type Link = LinkedListLink;
    type Value = Block;
    type Pointer = Box<Block>;
    #[inline]
    unsafe fn get_value(&self, link: *const Self::Link) -> *const Self::Value {
        container_of!(link, Block, status) // It's inside enum, so probably -usize or something like that.
    }
    #[inline]
    unsafe fn get_link(&self, value: *const Self::Value) -> *const Self::Link {
        if let BlockStatus::Free(link) = &(*value).status {
            link as *const _
        } else {
            panic!("This block is not free");
        }
    }
}

struct FileBlockAdapter;
unsafe impl Adapter for FileBlockAdapter{
    type Link = RBTreeLink;
    type Value = Block;
    type Pointer = Box<Block>;
    #[inline]
    unsafe fn get_value(&self, link: *const Self::Link) -> *const Self::Value {
        container_of!(link, Block, status)
    }
    #[inline]
    unsafe fn get_link(&self, value: *const Self::Value) -> *const Self::Link {
        if let BlockStatus::Allocated(link, _ ) = &(*value).status {
            link as *const _
        } else {
            panic!("This block is not allocated");
        }
    }
}


// intrusive_adapter!(FreeListAdapter = Rc<Block>: Block { free_list_link: SinglyLinkedListLink });
// intrusive_adapter!(FileBlockAdapter = Rc<Block>: Block { file_block_link: RBTreeLink });
impl<'a> KeyAdapter<'a> for FileBlockAdapter {
    type Key = u64;
    fn get_key(&self, x: &'a Block) -> Self::Key { if let BlockStatus::Allocated(_, offset) = x.status {
            offset
        } else {
            panic!("This block is not allocated");
        } }
}

struct DataList(RBTree<FileBlockAdapter>);

impl Clone for DataList {
    fn clone(&self) -> Self {
        let mut list = RBTree::new(FileBlockAdapter);
        let mut cursor = list.front_mut();
        for entry in self.0.iter() {
            cursor.insert_before(Box::new(entry.clone()));
        }
        DataList(list)
    }
}

impl Serialize for DataList {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.0.iter().count()))?;
        for element in self.0.iter() {
            seq.serialize_element(element)?;
        }
        seq.end()
    }
}

struct DataListVisitor;

impl<'de> Visitor<'de> for DataListVisitor
{
    // The type that our Visitor is going to produce.
    type Value = DataList;

    // Format a message stating what data this Visitor expects to receive.
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a data list")
    }

    // Deserialize MyMap from an abstract "map" provided by the
    // Deserializer. The MapAccess input is a callback provided by
    // the Deserializer to let us see each entry in the map.
    fn visit_seq<M>(self, mut access: M) -> Result<Self::Value, M::Error>
    where
        M: SeqAccess<'de>,
    {
        let mut list = RBTree::new(FileBlockAdapter);
        let mut cursor = list.front_mut();

        // While there are entries remaining in the input, add them
        // into our map.
        while let Some(block) = access.next_element()? {
            cursor.insert_before(Box::new(block))
        }

        Ok(DataList(list))
    }
}



impl<'de> Deserialize<'de> for DataList {
    fn deserialize<D>(deserializer: D) -> Result<DataList, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(DataListVisitor)
    }
}

impl DataList {
    fn new() -> Self {
        DataList(RBTree::new(FileBlockAdapter))
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
    free_list: LinkedList<FreeListAdapter>,
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
        let mut free_list = LinkedList::new(FreeListAdapter);
        free_list.push_front(Block::new(serialized_size(&header).unwrap(), std::usize::MAX));
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


        let mut free_list = LinkedList::new(FreeListAdapter);
        free_list.push_front(Block::new(header.ft_offset, std::usize::MAX));

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
    fn allocate(&mut self, size: usize) -> Box<Block> {
        let mut cursor = self
            .free_list
            .cursor_mut();
        loop {
            let (val_offset, val_size) = {
                let val = cursor.get().unwrap();
                (val.location, val.size)
            };
            if val_size > size {
                return cursor.replace_with(Block::new(val_offset + size as u64, val_size - size)).unwrap();
            } else if val_size == size {
                return cursor.remove().unwrap();
            }
            cursor.move_next();
        }
    }
    fn deallocate(&mut self, block: Box<Block>) {
        let mut cursor = self
            .free_list
            .cursor_mut();

        loop {
            let val = cursor.get();
            if val.is_none()  {
                panic!("End of list disappeared");
            }
            let val = val.unwrap();
            if val.location == block.location + block.size as u64 {
                // Back merge
                block.size += val.size;
                cursor.replace_with(block);
                return;
            }
            if val.location > block.location {
                cursor.insert_before(block);
                return;
            }
            if val.location + val.size as u64 == block.location {
                // Front merge
                block.location = val.location;
                block.size += val.size;
                cursor.remove();
            }
            cursor.move_next();
        }
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
        let overlaps = Vec::new();

        let bound = (offset + buff.len()) as u64;
        
        let mut cursor = file
            .data
            .as_mut()
            .expect("Cannot encode write when there is no data")
            .0
            .upper_bound_mut(Bound::Included(&bound));

        loop {
            let block = cursor.get();
            if block.is_none() {
                break;
            }
            if block.unwrap().file_offset().unwrap() + block.unwrap().size as u64 > offset as u64 {
                overlaps.push(cursor.remove().unwrap());
            }
            cursor.move_prev();
        }


        if overlaps.len() == 0 {
            // No overlap simply allocate from free_list and write
            let mut block = self.allocate(buff.len());
            block.status = BlockStatus::Allocated(RBTreeLink::default(), offset as u64);
            self.get_or_open(path).data.as_mut().unwrap().0.insert(block);
            return self.serialised.write_all_at(buff, block.location);
        }


        let first = overlaps.first().unwrap();
        let last = overlaps.last().unwrap();
        if overlaps.len() == 1
            && first.file_offset().unwrap() <= offset as u64
            && first.size >= buff.len()
        {
            // This write fits completely into another write, just write the
            // data there
            let location = first.location + offset as u64 - first.file_offset().unwrap();
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
        let mut block = self.allocate(need_space);
        if first_exclusive != 0 {
            move_file_range(
                &mut self.serialised,
                first.1.location,
                block.location,
                first_exclusive,
            )?;
        }
        self.serialised
            .write_all_at(buff, block.location + first_exclusive as u64)?;
        if last_exclusive != 0 {
            move_file_range(
                &mut self.serialised,
                last.1.location + (last.1.size - last_exclusive) as u64,
                block.location + (first_exclusive + buff.len()) as u64,
                last_exclusive,
            )?;
        }
        let data_blocks = &mut self.get_or_open(path).data.as_mut().unwrap().0;
        for overlap in overlaps.iter() {
            data_blocks.remove(&overlap.0);
        }
        block.status = BlockStatus::Allocated(RBTreeLink::default(), first.0 as u64);
        data_blocks.insert(
            block,
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
                            None
                        })
                        .unwrap_or(Some(DataList::new()));
                    self.files.insert(
                        to.into_owned(),
                        set_fields!(File::default() => {
                            .uid: Some(uid),
                            .gid: Some(gid),
                            .ty: FileType::Hardlink(from.into_owned()),
                            .data: data,
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
                        for block in d.0 {
                            self.deallocate(block);
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
                    let file = self.get_or_open(&path);
                    file.size = FileSize::Exactly(size as u64);
                    if let Some(ref mut d) = file.data {
                        let mut cursor = d.0.lower_bound_mut(Bound::Included(&(size as u64)));
                        while let Some(block) = cursor.remove() {
                            self.deallocate(block);
                        }
                        cursor.move_prev();
                        if let Some(block) = cursor.get() {
                            if block.file_offset().unwrap() + block.size as u64 > size as u64 {
                                let dealloc_size =
                                        block.file_offset().unwrap() + block.size as u64 - size as u64;
                                block.size -= dealloc_size as usize;
                                self.deallocate(Block::new(block.location
                                            + (block.size as u64 - dealloc_size)
                                                as u64, dealloc_size as usize));
           
                            }
                        }
                    }
                }
                VFSCall::fallocate(fallocate {
                    path,
                    offset,
                    length,
                    ..
                }) => {
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
    pub fn finalize(mut self) -> Result<(), Error> {
        use std::io::{ErrorKind, Seek, SeekFrom};
        let end_of_data = self.free_list.iter().last().unwrap().location;
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

struct SnapshotApply<'a> {
    snapshot: &'a Snapshot,
    file_iter: hash_map::Iter<'a, PathBuf, File>,
    current_file: Option<(&'a PathBuf, File)>,
    data_iter: Option<rbtree::Iter<'a, FileBlockAdapter>>,
    xattr_iter: Option<hash_map::Iter<'a, Vec<u8>, Option<Vec<u8>>>>,
}

impl<'a> Iterator for SnapshotApply<'a> {
    type Item = VFSCall<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        use std::borrow::Cow;
        if let Some(data_blocks) = &mut self.data_iter {
            if let Some(block) = data_blocks.next() {
                let (cp, _) = self.current_file.as_ref().unwrap();
                let mut buf = Vec::with_capacity(block.size);
                unsafe { buf.set_len(block.size) };
                self.snapshot
                    .serialised
                    .read_exact_at(&mut buf, block.location)
                    .expect("Failed to read snapshot data");
                return Some(VFSCall::write(write {
                    path: Cow::Borrowed(cp),
                    offset: block.file_offset().unwrap() as i64,
                    buf: Cow::Owned(buf),
                }));
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
                    return Some(VFSCall::mkdir(mkdir {
                        mode: cf.mode.take().unwrap(),
                        path: Cow::Borrowed(cp),
                        security: FileSecurity::Unix {
                            uid: cf.uid.take().unwrap(),
                            gid: cf.gid.take().unwrap(),
                        },
                    }))
                }
                FileType::New(flags) => {
                    return Some(VFSCall::create(create {
                        flags,
                        mode: cf.mode.take().unwrap(),
                        path: Cow::Borrowed(cp),
                        security: FileSecurity::Unix {
                            uid: cf.uid.take().unwrap(),
                            gid: cf.gid.take().unwrap(),
                        },
                    }))
                }
                FileType::Special(rdev) => {
                    return Some(VFSCall::mknod(mknod {
                        mode: cf.mode.take().unwrap(),
                        path: Cow::Borrowed(cp),
                        security: FileSecurity::Unix {
                            uid: cf.uid.take().unwrap(),
                            gid: cf.gid.take().unwrap(),
                        },
                        rdev,
                    }))
                }
                FileType::Symlink(from) => {
                    return Some(VFSCall::symlink(symlink {
                        from: Cow::Owned(from),
                        to: Cow::Borrowed(cp),
                        security: FileSecurity::Unix {
                            uid: cf.uid.take().unwrap(),
                            gid: cf.gid.take().unwrap(),
                        },
                    }))
                }
                FileType::Hardlink(from) => {
                    return Some(VFSCall::link(link {
                        from: Cow::Owned(from),
                        to: Cow::Borrowed(cp),
                        security: FileSecurity::Unix {
                            uid: cf.uid.take().unwrap(),
                            gid: cf.gid.take().unwrap(),
                        },
                    }))
                }
                FileType::Moved(from) => {
                    return Some(VFSCall::rename(rename {
                        from: Cow::Owned(from),
                        to: Cow::Borrowed(cp),
                        flags: 0,
                    }))
                }
            }
        }
        let (cp, cf) = self.current_file.as_mut().unwrap();
        if let (Some(uid), Some(gid)) = (cf.uid.take(), cf.gid.take()) {
            return Some(VFSCall::security(security {
                path: Cow::Borrowed(cp),
                security: FileSecurity::Unix { uid, gid },
            }));
        }
        if let Some(mode) = cf.mode.take() {
            return Some(VFSCall::chmod(chmod {
                path: Cow::Borrowed(cp),
                mode,
            }));
        }
        if let FileSize::Exactly(tsize) = cf.size {
            cf.size = FileSize::Unknown;
            return Some(VFSCall::truncate(truncate {
                path: Cow::Borrowed(cp),
                size: tsize as i64,
            }));
        }
        if let FileSize::MoreThan(fsize) = cf.size {
            cf.size = FileSize::Unknown;
            return Some(VFSCall::fallocate(fallocate {
                path: Cow::Borrowed(cp),
                length: fsize as i64,
                offset: 0,
                mode: 0,
            })); // HACK, I don't think this is good.
        }
        if let Some(timespec) = cf.time.take() {
            return Some(VFSCall::utimens(utimens {
                path: Cow::Borrowed(cp),
                timespec,
            }));
        }
        if let Some(xattrs) = &mut self.xattr_iter {
            use std::ffi::CStr;
            if let Some((k, v)) = xattrs.next() {
                if let Some(value) = v {
                    return Some(VFSCall::setxattr(setxattr {
                        path: Cow::Borrowed(cp),
                        name: Cow::Borrowed(
                            CStr::from_bytes_with_nul(&k).unwrap(),
                        ),
                        value: Cow::Borrowed(value),
                        flags: 0,
                    }));
                } else {
                    return Some(VFSCall::removexattr(removexattr {
                        path: Cow::Borrowed(cp),
                        name: Cow::Borrowed(
                            CStr::from_bytes_with_nul(&k).unwrap(),
                        ),
                    }));
                }
            } else {
                self.xattr_iter = None;
            }
        }
        None
    }
}
