use common::{link, VFSCall};
use std::borrow::Cow;
use std::ffi::{CStr, CString};
use std::fs;
use std::io::Error;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

const FILESTORE_PATH: &str = "/.fsyncer-deleted";

pub struct FileStore {
    current_size: u64,
    current_token: u64,
    vfsroot: String,
}

impl FileStore {
    pub fn vfsroot(&self) -> &str {
        &self.vfsroot
    }
    pub fn new(vfsroot: String) -> Result<Self, Error> {
        let mut store_path = vfsroot.clone();
        store_path.push_str(FILESTORE_PATH);

        if !Path::new(&store_path).exists() {
            //println!(vfsroot);
            fs::create_dir(&store_path)?;
        }

        debug!(store_path);

        let sizes = fs::read_dir(&store_path)?.map(|f| f?.metadata().map(|m| m.len()));
        let mut current_size = 0;
        for size in sizes {
            current_size += size?;
        }

        let current_token = if let Some(e) = fs::read_dir(&store_path)?.last() {
            e?.file_name().to_str().unwrap().parse::<u64>().unwrap()
        } else {
            0
        };

        Ok(FileStore {
            vfsroot,
            current_size,
            current_token,
        })
    }
    pub fn store(&mut self, path: String) -> Result<u64, Error> {
        //debug!(path);
        let size = fs::symlink_metadata(&path)
            .expect("File should be here")
            .len();
        let token = self.current_token;
        self.current_token += 1;
        //println!("{}/.fsyncer-deleted/{}", self.vfsroot, token);
        fs::rename(
            &path,
            format!("{}/.fsyncer-deleted/{}", self.vfsroot, token),
        )?;
        self.current_size += size;
        Ok(token)
    }
    pub fn recover<'a>(&self, token: u64, path: &'a CStr) -> Result<VFSCall<'a>, Error> {
        let rela_path = format!("/.fsyncer-deleted/{}", token);
        let stbuf = fs::symlink_metadata(format!("{}/.fsyncer-deleted/{}", self.vfsroot, token))?;

        Ok(VFSCall::link(link {
            from: Cow::Owned(CString::new(rela_path).unwrap()),
            to: Cow::Borrowed(path),
            uid: stbuf.uid(),
            gid: stbuf.gid(),
        }))
        //fs::hard_link(format!("{}{}", self.path, token), &path)
    }
    pub fn delete(&mut self, path: String) -> Result<u64, Error> {
        let size = fs::symlink_metadata(&path)?.len();
        fs::remove_file(&path)?;
        self.current_size -= size;
        Ok(size)
    }
    pub fn current_size(&self) -> u64 {
        self.current_size
    }
}
