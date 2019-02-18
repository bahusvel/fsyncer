use common::{ffi::ToCString, link, VFSCall};
use journal::Journal;
use std::borrow::Cow;
use std::ffi::CString;
use std::fs;
use std::io::Error;
//use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

const FILESTORE_PATH: &str = ".fsyncer-deleted";

#[derive(Clone, Copy)]
enum DeletePolicy {
    JournalBefore,
    BeforeAffected,
    FilestoreEntry,
}

pub struct FileStore {
    current_size: u64,
    oldest_token: u64,
    current_token: u64,
    max_size: u64,
    vfsroot: PathBuf,
    policy: DeletePolicy,
}

impl FileStore {
    pub fn new(vfsroot: &Path, max_size: u64) -> Result<Self, Error> {
        let store_path = vfsroot.join(FILESTORE_PATH);

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

        let oldest_token = if let Some(e) = fs::read_dir(&store_path)?.next() {
            e?.file_name().to_str().unwrap().parse::<u64>().unwrap()
        } else {
            0
        };

        let current_token = if let Some(e) = fs::read_dir(&store_path)?.last() {
            e?.file_name().to_str().unwrap().parse::<u64>().unwrap()
        } else {
            0
        };

        Ok(FileStore {
            vfsroot: vfsroot.to_path_buf(),
            current_size,
            current_token,
            oldest_token,
            max_size,
            policy: DeletePolicy::FilestoreEntry,
        })
    }
    pub fn store(j: &mut Journal, path: &Path) -> Result<u64, Error> {
        //debug!(path);
        let this = j.fstore();

        let size = fs::symlink_metadata(&path)
            .expect("File should be here")
            .len();

        while this.current_size + size > this.max_size {
            // Need eliminate old entries from the journal
            match this.policy {
                DeletePolicy::FilestoreEntry => {
                    let t = this.oldest_token;
                    this.delete(t)?;
                }
                _ => panic!("Not implemented"),
            }
        }

        let token = this.current_token;
        this.current_token += 1;
        //println!("{}/.fsyncer-deleted/{}", self.vfsroot, token);
        fs::rename(
            &path,
            format!("{}/.fsyncer-deleted/{}", this.vfsroot.display(), token),
        )?;
        this.current_size += size;
        Ok(token)
    }

    pub fn recover(vfsroot: &Path, token: u64, path: &Path) -> Result<VFSCall<'static>, Error> {
        let rela_path = format!("/.fsyncer-deleted/{}", token);
        let stbuf =
            fs::symlink_metadata(format!("{}/.fsyncer-deleted/{}", vfsroot.display(), token))?;

        Ok(VFSCall::link(link {
            from: Cow::Owned(rela_path),
            to: Cow::Owned(path.to_path_buf()),
            uid: stbuf.uid(),
            gid: stbuf.gid(),
        }))
    }
    pub fn delete(&mut self, token: u64) -> Result<u64, Error> {
        let path = format!("{}/.fsyncer-deleted/{}", self.vfsroot.display(), token);
        let size = fs::symlink_metadata(&path)?.len();
        fs::remove_file(&path)?;
        self.current_size -= size;
        self.oldest_token += 1;
        Ok(size)
    }
}
