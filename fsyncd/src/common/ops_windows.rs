extern crate winapi;
pub use self::winapi::shared::{
    basetsd::*,
    minwindef::{BOOL, DWORD, FILETIME, LPCVOID, LPDWORD},
    ntdef::*,
};
pub use self::winapi::um::winnt::{ACCESS_MASK, PSECURITY_DESCRIPTOR, PSECURITY_INFORMATION};

extern "C" {
    pub fn MirrorCreateFile(
        path: LPCWSTR,
        access: ACCESS_MASK,
        shared: ULONG,
        security_descriptor: PSECURITY_DESCRIPTOR,
        disposition: ULONG,
        attributes: ULONG,
        options: ULONG,
        handle: *mut HANDLE,
    ) -> NTSTATUS;
    pub fn MirrorWriteFile(
        path: LPCWSTR,
        buffer: LPCVOID,
        len: DWORD,
        bytes_written: LPDWORD,
        offset: LONGLONG,
        handle: HANDLE,
    ) -> NTSTATUS;
    pub fn MirrorSetFileAttributes(path: LPCWSTR, attributes: DWORD) -> NTSTATUS;
    pub fn MirrorSetFileTime(
        path: LPCWSTR,
        creation: *const FILETIME,
        access: *const FILETIME,
        write: *const FILETIME,
        handle: HANDLE,
    ) -> NTSTATUS;
    pub fn MirrorDeleteFile(path: LPCWSTR) -> NTSTATUS;
    pub fn MirrorDeleteDirectory(path: LPCWSTR) -> NTSTATUS;
    pub fn MirrorMoveFile(
        path: LPCWSTR,
        new_name: LPCWSTR,
        replace: BOOL,
        handle: HANDLE,
    ) -> NTSTATUS;
    pub fn MirrorSetEndOfFile(path: LPCWSTR, offset: LONGLONG, handle: HANDLE) -> NTSTATUS;
    pub fn MirrorSetAllocationSize(path: LPCWSTR, size: LONGLONG, handle: HANDLE) -> NTSTATUS;
    pub fn MirrorSetFileSecurity(
        path: LPCWSTR,
        security: PSECURITY_INFORMATION,
        descriptor: PSECURITY_DESCRIPTOR,
        handle: HANDLE,
    ) -> NTSTATUS;
    pub fn MirrorFlushFileBuffers(path: LPCWSTR, handle: HANDLE) -> NTSTATUS;
}
