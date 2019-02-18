extern crate dokan;

use self::dokan::*;
use common::*;
use libc::{uint32_t, wcslen};
use server::{post_op, pre_op};
use std::borrow::Cow;
use std::path::PathBuf;
use std::slice;

fn wstr_to_path<'a>(path: LPCWSTR) -> PathBuf {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    let len = wcslen(path);
    PathBuf::from(OsString::from_wide(slice::from_raw_parts(path, len)))
}

pub unsafe extern "stdcall" fn zw_create_file(
    path: LPCWSTR,
    context: PDOKAN_IO_SECURITY_CONTEXT,
    access: ACCESS_MASK,
    attributes: ULONG,
    shared: ULONG,
    disposition: ULONG,
    options: ULONG,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {

}
pub unsafe extern "stdcall" fn cleanup(path: LPCWSTR, info: PDOKAN_FILE_INFO) {}
pub unsafe extern "stdcall" fn write_file(
    path: LPCWSTR,
    buffer: LPCVOID,
    len: DWORD,
    bytes_written: LPDWORD,
    offset: LONGLONG,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let call = VFSCall::write(write {
        path: Cow::Owned(wstr_to_path(path)),
        buf: Cow::Borrowed(slice::from_raw_parts(buffer as *const u8, len as usize)),
        offset,
    });
    if let Some(r) = pre_op(&call) {
        return r;
    }
    let status = MirrorWriteFile(path, buffer, len, bytes_written, offset, info);
    post_op(&call, status)
}
pub unsafe extern "stdcall" fn set_file_attributes(
    path: LPCWSTR,
    attributes: DWORD,
    _: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let call = VFSCall::chmod(chmod {
        path: Cow::Owned(wstr_to_path(path)),
        mode: attributes,
    });
    if let Some(r) = pre_op(&call) {
        return r;
    }
    let status = MirrorSetFileAttributes(path, attributes);
    post_op(&call, status)
}
pub unsafe extern "stdcall" fn set_file_time(
    path: LPCWSTR,
    creation: *const FILETIME,
    access: *const FILETIME,
    write: *const FILETIME,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let call = VFSCall::utimens(utimens {
        path: Cow::Owned(wstr_to_path(path)),
        timespec: [
            enc_timespec::from(*creation),
            enc_timespec::from(*access),
            enc_timespec::from(*write),
        ],
    });
    if let Some(r) = pre_op(&call) {
        return r;
    }
    let status = MirrorSetFileTime(path, creation, access, write, (*info).Context as HANDLE);
    post_op(&call, status)
}
pub unsafe extern "stdcall" fn move_file(
    path: LPCWSTR,
    new_name: LPCWSTR,
    replace: BOOL,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let call = VFSCall::rename(rename {
        from: Cow::Owned(wstr_to_path(path)),
        to: Cow::Owned(wstr_to_path(new_name)),
        flags: replace as uint32_t,
    });
    if let Some(r) = pre_op(&call) {
        return r;
    }
    let status = MirrorMoveFile(path, new_name, replace, (*info).Context as HANDLE);
    post_op(&call, status)
}
pub unsafe extern "stdcall" fn set_end_of_file(
    path: LPCWSTR,
    offset: LONGLONG,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let call = VFSCall::truncate(truncate {
        path: Cow::Owned(wstr_to_path(path)),
        size: offset,
    });
    if let Some(r) = pre_op(&call) {
        return r;
    }
    let status = MirrorSetEndOfFile(path, offset, (*info).Context as HANDLE);
    post_op(&call, status)
}
pub unsafe extern "stdcall" fn set_allocation_size(
    path: LPCWSTR,
    size: LONGLONG,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let call = VFSCall::allocation_size(allocation_size {
        path: Cow::Owned(wstr_to_path(path)),
        size,
    });
    if let Some(r) = pre_op(&call) {
        return r;
    }
    let status = MirrorSetAllocationSize(path, size, (*info).Context as HANDLE);
    post_op(&call, status)
}
pub unsafe extern "stdcall" fn set_file_security(
    path: LPCWSTR,
    security: PSECURITY_INFORMATION,
    descriptor: PSECURITY_DESCRIPTOR,
    length: ULONG,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
}
pub unsafe extern "stdcall" fn flush_file_buffers(
    path: LPCWSTR,
    info: PDOKAN_FILE_INFO,
) -> NTSTATUS {
    let call = VFSCall::fsync(fsync {
        path: Cow::Owned(wstr_to_path(path)),
        isdatasync: 0,
    });
    if let Some(r) = pre_op(&call) {
        return r;
    }
    let status = MirrorFlushFileBuffers(path, (*info).Context as HANDLE);
    post_op(&call, status)
}
