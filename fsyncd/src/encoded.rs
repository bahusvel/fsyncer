#![allow(non_camel_case_types)]
#![allow(unused)]

use libc::*;
use std::ffi::CString;

macro_rules! encoded_syscall {
    ($name:ident {$($field:ident: $ft:ty),*}) => {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        pub struct $name {
            $(
                pub $field: $ft,
            )*
        }
    };
}

macro_rules! path_syscall {
    ($name:ident {$($field:ident: $ft:ty),*}) => {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        pub struct $name {
            pub path: CString,
            $(
                pub $field: $ft,
            )*
        }
    }
}

path_syscall!(mknod {
    mode: uint32_t,
    rdev: uint64_t
});

path_syscall!(mkdir { mode: uint32_t });

path_syscall!(unlink {});

path_syscall!(rmdir {});

encoded_syscall!(symlink {
    from: CString,
    to: CString
});

encoded_syscall!(rename {
    from: CString,
    to: CString,
    flags: uint32_t
});

encoded_syscall!(link {
    from: CString,
    to: CString
});

path_syscall!(chmod { mode: uint32_t });

path_syscall!(chown {
    uid: uint32_t,
    gid: uint32_t
});

path_syscall!(truncate { size: int64_t });

path_syscall!(write {
    offset: int64_t,
    buf: Vec<u8>
});

path_syscall!(fallocate {
    mode: int32_t,
    offset: int64_t,
    length: int64_t
});

path_syscall!(setxattr {
    name: CString,
    value: Vec<u8>,
    flags: int32_t
});

path_syscall!(removexattr { name: CString });

path_syscall!(create {
    mode: uint32_t,
    flags: int32_t
});

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct enc_timespec {
    pub tv_sec: int64_t,
    pub tv_nsec: int64_t,
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
