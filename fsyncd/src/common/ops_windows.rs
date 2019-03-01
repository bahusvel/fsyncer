#![allow(non_snake_case)]

pub use winapi::shared::{
    basetsd::*,
    minwindef::{BOOL, DWORD, FILETIME, LPCVOID, LPDWORD},
    ntdef::*,
    ntstatus::STATUS_SUCCESS,
};
pub use winapi::um::errhandlingapi::GetLastError;
use winapi::um::fileapi::{DeleteFileW, RemoveDirectoryW};
pub use winapi::um::winnt::{ACCESS_MASK, PSECURITY_DESCRIPTOR, PSECURITY_INFORMATION};

#[link(name = "fsyncer", kind = "static")]
extern "C" {
    pub fn OpCreateFile(
        path: LPCWSTR,
        security_descriptor: PSECURITY_DESCRIPTOR,
        access: ACCESS_MASK,
        attributes: DWORD,
        shared: ULONG,
        disposition: DWORD,
        handle: *mut HANDLE,
    ) -> NTSTATUS;
    pub fn OpCreateDirectory(
        path: LPCWSTR,
        security_descriptor: PSECURITY_DESCRIPTOR,
        access: ACCESS_MASK,
        attributes: DWORD,
        shared: ULONG,
        disposition: DWORD,
        handle: *mut HANDLE,
    ) -> NTSTATUS;
    pub fn OpWriteFile(
        path: LPCWSTR,
        buffer: LPCVOID,
        len: DWORD,
        bytes_written: LPDWORD,
        offset: LONGLONG,
        handle: HANDLE,
    ) -> NTSTATUS;
    pub fn OpSetFileAttributes(path: LPCWSTR, attributes: DWORD) -> NTSTATUS;
    pub fn OpSetFileTime(
        path: LPCWSTR,
        creation: *const FILETIME,
        access: *const FILETIME,
        write: *const FILETIME,
        handle: HANDLE,
    ) -> NTSTATUS;

    pub fn OpMoveFile(path: LPCWSTR, new_name: LPCWSTR, replace: BOOL, handle: HANDLE) -> NTSTATUS;
    pub fn OpSetEndOfFile(path: LPCWSTR, offset: LONGLONG, handle: HANDLE) -> NTSTATUS;
    pub fn OpSetAllocationSize(path: LPCWSTR, size: LONGLONG, handle: HANDLE) -> NTSTATUS;
    pub fn OpSetFileSecurity(
        path: LPCWSTR,
        security: PSECURITY_INFORMATION,
        descriptor: PSECURITY_DESCRIPTOR,
        handle: HANDLE,
    ) -> NTSTATUS;
    pub fn OpFlushFileBuffers(path: LPCWSTR, handle: HANDLE) -> NTSTATUS;
    pub static DOKAN_OPS_PTR: *mut VOID;
}

pub unsafe fn OpDeleteFile(path: LPCWSTR) -> NTSTATUS {
    if DeleteFileW(path) == 0 {
        //return GetLastError();
    }
    STATUS_SUCCESS
}
pub unsafe fn OpDeleteDirectory(path: LPCWSTR) -> NTSTATUS {
    if RemoveDirectoryW(path) == 0 {
        //return GetLastError();
    }
    STATUS_SUCCESS
}
