use common::{link, VFSCall};
use std::borrow::Cow;
use std::ffi::{CStr, CString};
use std::fs;
use std::io::Error;
use std::os::unix::fs::MetadataExt;

const FILESTORE_PATH: &str = ".fsyncer-deleted/";

pub struct FileStore {
    current_size: u64,
    current_token: u64,
    path: String,
}

impl FileStore {
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn new(mut vfsroot: String) -> Result<Self, Error> {
        vfsroot.push_str(FILESTORE_PATH);
        let sizes = fs::read_dir(&vfsroot)?.map(|f| f?.metadata().map(|m| m.len()));
        let mut current_size = 0;
        for size in sizes {
            current_size += size?;
        }

        let current_token = if let Some(e) = fs::read_dir(&vfsroot)?.last() {
            e?.file_name().to_str().unwrap().parse::<u64>().unwrap()
        } else {
            0
        };

        Ok(FileStore {
            path: vfsroot,
            current_size,
            current_token,
        })
    }
    pub fn store(&mut self, path: String) -> Result<u64, Error> {
        let size = fs::metadata(&path)?.len();
        let token = self.current_token;
        self.current_token += 1;
        fs::rename(&path, format!("{}{}", self.path, token))?;
        self.current_size += size;
        Ok(token)
    }
    pub fn recover<'a>(&self, token: u64, path: &'a CStr) -> Result<VFSCall<'a>, Error> {
        let rela_path = format!("{}{}", FILESTORE_PATH, token);
        let stbuf = fs::metadata(format!("{}{}", self.path, token))?;

        Ok(VFSCall::link(link {
            from: Cow::Owned(CString::new(rela_path).unwrap()),
            to: Cow::Borrowed(path),
            uid: stbuf.uid(),
            gid: stbuf.gid(),
        }))
        //fs::hard_link(format!("{}{}", self.path, token), &path)
    }
    pub fn delete(&mut self, path: String) -> Result<u64, Error> {
        let size = fs::metadata(&path)?.len();
        fs::remove_file(&path)?;
        self.current_size -= size;
        Ok(size)
    }
    pub fn current_size(&self) -> u64 {
        self.current_size
    }
}
