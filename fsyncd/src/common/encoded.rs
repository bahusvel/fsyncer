#![allow(non_camel_case_types)]
#![allow(unused)]

use libc::*;
use std::borrow::Cow;
use std::ffi::{CStr, CString};

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
         encoded_syscall!($name {path:  Cow<'a, CStr>, $($field: $ft,)* });
    }
}

path_syscall!(mknod {
    mode: uint32_t,
    rdev: uint64_t,
    uid: uint32_t,
    gid: uint32_t
});

path_syscall!(mkdir {
    mode: uint32_t,
    uid: uint32_t,
    gid: uint32_t
});

path_syscall!(unlink {});

path_syscall!(rmdir {});

path_syscall!(fsync { isdatasync: c_int });

encoded_syscall!(symlink {
    from: Cow<'a, CStr>,
    to: Cow<'a, CStr>,
    uid: uint32_t,
    gid: uint32_t
});

encoded_syscall!(rename {
    from: Cow<'a, CStr>,
    to: Cow<'a, CStr>,
    flags: uint32_t,
});

encoded_syscall!(link {
    from: Cow<'a, CStr>,
    to: Cow<'a, CStr>,
    uid: uint32_t,
    gid: uint32_t
});

path_syscall!(chmod { mode: uint32_t });

path_syscall!(chown {
    uid: uint32_t,
    gid: uint32_t
});

path_syscall!(truncate { size: int64_t });

path_syscall!(write {
    offset: int64_t,
    buf: Cow<'a, [u8]>
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

path_syscall!(create {
    mode: uint32_t,
    flags: int32_t,
    uid: uint32_t,
    gid: uint32_t
});

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct enc_timespec {
    pub tv_sec: int64_t,
    pub tv_nsec: int64_t,
}

impl enc_timespec {
    pub fn xor(&self, other: &Self) -> Self {
        enc_timespec {
            tv_sec: self.tv_sec ^ other.tv_sec,
            tv_nsec: self.tv_nsec ^ other.tv_nsec,
        }
    }
}

impl From<timespec> for enc_timespec {
    fn from(spec: timespec) -> Self {
        enc_timespec {
            tv_sec: spec.tv_sec,
            tv_nsec: spec.tv_nsec,
        }
    }
}

impl Into<timespec> for enc_timespec {
    fn into(self) -> timespec {
        timespec {
            tv_sec: self.tv_sec,
            tv_nsec: self.tv_nsec,
        }
    }
}

path_syscall!(utimens {
    timespec: [enc_timespec; 2]
});
