

#[repr(C)]
#[derive(PartialEq, Clone, Copy)]
pub enum op_type {
    MKNOD,
    MKDIR,
    UNLINK,
    RMDIR,
    SYMLINK,
    RENAME,
    LINK,
    CHMOD,
    CHOWN,
    TRUNCATE,
    WRITE,
    FALLOCATE,
    SETXATTR,
    REMOVEXATTR,
    CREATE,
    UTIMENS,
}

#[repr(C)]
#[derive(PartialEq, Clone, Copy)]
#[allow(non_camel_case_types)]
pub enum client_mode {
    MODE_ASYNC,
    MODE_SYNC,
    MODE_CONTROL,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct op_msg {
    pub op_length: u32,
    pub op_type: op_type,
}

#[repr(C)]
pub struct init_msg {
    pub mode: client_mode,
    pub dsthash: u64,
    pub compress: CompMode,
}

#[repr(C)]
pub struct ack_msg {
    pub retcode: i32,
}

bitflags! {
    pub struct CompMode: u32 {
        const RT_DSSC_ZLIB = 0b001;
        const RT_DSSC_CHUNKED = 0b010;
        const STREAM_ZSTD = 0b100;
    }
}
