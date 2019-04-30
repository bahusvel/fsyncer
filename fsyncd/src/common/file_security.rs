#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
use error::{Error, FromError};
use std::io;
use std::path::Path;

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Hash)]
pub enum FileSecurity {
    Windows {
        str_desc: String,
        info: Option<u32>,
        /* control: u32, may need to replicate control bits instead of
         * deriving them on client side */
    },
    Unix {
        uid: u32,
        gid: u32,
    },
    Portable {
        owner: Option<String>,
        group: Option<String>,
    },
}

#[cfg(target_os = "windows")]
include!("security_windows.rs");

#[cfg(target_family = "unix")]
include!("security_unix.rs");
