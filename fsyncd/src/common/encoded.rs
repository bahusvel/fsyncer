#![allow(non_camel_case_types)]
#![allow(unused)]

use common::FileSecurity;
use libc::*;
use std::borrow::Cow;
use std::ffi::{CStr, CString, OsString};
use std::ops::BitXor;
use std::path::Path;

#[cfg(target_os = "windows")]
use common::FILETIME;

macro_rules! encoded_syscall {
    ($name:ident {$($field:ident: $ft:ty,)*}) => {
        #[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
        pub struct $name<'a> {
            $(
                pub $field: $ft,
            )*
        }
    };
    ($name:ident {$($field:ident: $ft:ty),*}) => {
        encoded_syscall!($name { $($field: $ft,)*});
    }
}

macro_rules! path_syscall {
    ($name:ident {$($field:ident: $ft:ty),*}) => {
         encoded_syscall!($name {path:  Cow<'a, Path>, $($field: $ft,)* });
    }
}

path_syscall!(mkdir {
    security: FileSecurity,
    mode: uint32_t // Attributes on windows
});

path_syscall!(unlink {});

path_syscall!(rmdir {});

path_syscall!(fsync { isdatasync: c_int });

encoded_syscall!(rename {
    from: Cow<'a, Path>,
    to: Cow<'a, Path>,
    flags: uint32_t,
});

encoded_syscall!(link {
    from: Cow<'a, Path>,
    to: Cow<'a, Path>,
    uid: uint32_t,
    gid: uint32_t
});

path_syscall!(truncate { size: int64_t });

path_syscall!(write {
    offset: int64_t,
    buf: Cow<'a, [u8]>
});

path_syscall!(create {
    flags: int32_t,
    security: FileSecurity,
    mode: uint32_t // Attributes on windows
});

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Hash)]
pub struct enc_timespec {
    pub high: int64_t,
    pub low: int64_t,
}

impl BitXor for enc_timespec {
    type Output = Self;

    fn bitxor(self, rhs: Self) -> Self {
        enc_timespec {
            high: self.high ^ rhs.high,
            low: self.low ^ rhs.low,
        }
    }
}

#[cfg(target_family = "unix")]
impl From<timespec> for enc_timespec {
    fn from(spec: timespec) -> Self {
        enc_timespec {
            high: spec.tv_sec,
            low: spec.tv_nsec,
        }
    }
}

#[cfg(target_family = "unix")]
impl Into<timespec> for enc_timespec {
    fn into(self) -> timespec {
        timespec {
            high: self.tv_sec,
            low: self.tv_nsec,
        }
    }
}

#[cfg(target_os = "windows")]
impl From<FILETIME> for enc_timespec {
    fn from(spec: FILETIME) -> Self {
        enc_timespec {
            high: spec.dwHighDateTime as i64,
            low: spec.dwLowDateTime as i64,
        }
    }
}

#[cfg(target_os = "windows")]
impl Into<FILETIME> for enc_timespec {
    fn into(self) -> FILETIME {
        FILETIME {
            dwHighDateTime: self.high as u32,
            dwLowDateTime: self.low as u32,
        }
    }
}

path_syscall!(utimens {
    timespec: [enc_timespec; 3] /* 2 on POSIX last is 0, 3 on Windows
                                 * (Created, Accessed, Written) */
});

path_syscall!(chmod { mode: uint32_t }); // On windows this represents attributes

// chown on Linux
path_syscall!(security {
    security: FileSecurity
});

// Linux Specific

encoded_syscall!(symlink {
    from: Cow<'a, Path>,
    to: Cow<'a, Path>,
    uid: uint32_t,
    gid: uint32_t
});

path_syscall!(mknod {
    mode: uint32_t,
    rdev: uint64_t,
    uid: uint32_t,
    gid: uint32_t
});

path_syscall!(fallocate {
    mode: int32_t,
    offset: int64_t,
    length: int64_t
});

path_syscall!(setxattr {
    name: Cow<'a, CStr>,
    value: Cow<'a, [u8]>,
    flags: int32_t
});

path_syscall!(removexattr { name: Cow<'a, CStr> });
