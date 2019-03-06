#![allow(non_snake_case)]

use std::{mem, ptr};
pub use winapi::shared::{
    basetsd::*,
    minwindef::{BOOL, DWORD, FILETIME, LPCVOID, LPDWORD},
    ntdef::*,
    ntstatus::STATUS_SUCCESS,
    winerror::ERROR_SUCCESS,
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
    ) -> DWORD;
    pub fn OpCreateDirectory(
        path: LPCWSTR,
        security_descriptor: PSECURITY_DESCRIPTOR,
        access: ACCESS_MASK,
        attributes: DWORD,
        shared: ULONG,
        disposition: DWORD,
        handle: *mut HANDLE,
    ) -> DWORD;
    pub fn OpMoveFile(new_name: LPCWSTR, replace: BOOL, handle: HANDLE) -> DWORD;
    pub static DOKAN_OPS_PTR: *mut VOID;
}

pub unsafe fn OpFlushFileBuffers(handle: HANDLE) -> DWORD {
    use winapi::um::fileapi::FlushFileBuffers;
    if FlushFileBuffers(handle) == 0 {
        return GetLastError();
    }
    return ERROR_SUCCESS;
}

pub unsafe fn OpDeleteFile(path: LPCWSTR) -> DWORD {
    if DeleteFileW(path) == 0 {
        return GetLastError();
    }
    return ERROR_SUCCESS;
}
pub unsafe fn OpDeleteDirectory(path: LPCWSTR) -> DWORD {
    if RemoveDirectoryW(path) == 0 {
        return GetLastError();
    }
    return ERROR_SUCCESS;
}

pub unsafe fn OpSetFileSecurity(
    security: PSECURITY_INFORMATION,
    descriptor: PSECURITY_DESCRIPTOR,
    handle: HANDLE,
) -> DWORD {
    use winapi::um::winuser::SetUserObjectSecurity;
    if SetUserObjectSecurity(handle, security, descriptor) == 0 {
        return GetLastError();
    }
    return ERROR_SUCCESS;
}

pub unsafe fn OpSetFileTime(
    creation: *const FILETIME,
    access: *const FILETIME,
    write: *const FILETIME,
    handle: HANDLE,
) -> DWORD {
    use winapi::um::fileapi::SetFileTime;
    if SetFileTime(handle, creation, access, write) == 0 {
        return GetLastError();
    }
    return ERROR_SUCCESS;
}

pub unsafe fn OpSetFileAttributes(path: LPCWSTR, attributes: DWORD) -> DWORD {
    use winapi::um::fileapi::SetFileAttributesW;
    if SetFileAttributesW(path, attributes) == 0 {
        return GetLastError();
    }
    return ERROR_SUCCESS;
}

pub unsafe fn OpSetEndOfFile(byte_offset: LONGLONG, handle: HANDLE) -> DWORD {
    use winapi::um::fileapi::{SetEndOfFile, SetFilePointerEx};
    use winapi::um::winbase::FILE_BEGIN;
    use winapi::um::winnt::LARGE_INTEGER;

    let mut offset: LARGE_INTEGER = mem::zeroed();
    *offset.QuadPart_mut() = byte_offset;

    if SetFilePointerEx(handle, offset, ptr::null_mut(), FILE_BEGIN) == 0 {
        return GetLastError();
    }

    if SetEndOfFile(handle) == 0 {
        return GetLastError();
    }
    return ERROR_SUCCESS;
}

pub unsafe fn OpWriteFile(
    buffer: LPCVOID,
    len: DWORD,
    bytes_written: LPDWORD,
    byte_offset: LONGLONG,
    handle: HANDLE,
) -> DWORD {
    use winapi::um::fileapi::{SetFilePointerEx, WriteFile};
    use winapi::um::winbase::FILE_BEGIN;
    use winapi::um::winnt::LARGE_INTEGER;

    let mut offset: LARGE_INTEGER = mem::zeroed();
    *offset.QuadPart_mut() = byte_offset;

    if SetFilePointerEx(handle, offset, ptr::null_mut(), FILE_BEGIN) == 0 {
        return GetLastError();
    }

    if WriteFile(handle, buffer, len, bytes_written, ptr::null_mut()) == 0 {
        return GetLastError();
    }
    return ERROR_SUCCESS;
}
