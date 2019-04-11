#![cfg(target_os = "windows")]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![feature(try_from)]
extern crate winapi;
use std::convert::TryFrom;
use std::ptr;
use winapi::shared::{
    basetsd::*,
    minwindef::{BOOL, DWORD, FILETIME, LPCVOID, LPDWORD},
    ntdef::*,
};
use winapi::um::consoleapi::SetConsoleCtrlHandler;
use winapi::um::wincon::{
    CTRL_BREAK_EVENT, CTRL_CLOSE_EVENT, CTRL_C_EVENT, CTRL_LOGOFF_EVENT,
    CTRL_SHUTDOWN_EVENT,
};
use winapi::um::winnt::{
    ACCESS_MASK, PSECURITY_DESCRIPTOR, PSECURITY_INFORMATION,
};

pub const FILE_NON_DIRECTORY_FILE: DWORD = 0x00000040;

pub const DOKAN_OPTION_DEBUG: ULONG = 1;
pub const DOKAN_OPTION_STDERR: ULONG = 2;
pub const DOKAN_OPTION_ALT_STREAM: ULONG = 4;
pub const DOKAN_OPTION_WRITE_PROTECT: ULONG = 8;
pub const DOKAN_OPTION_REMOVABLE: ULONG = 32;
pub const DOKAN_OPTION_MOUNT_MANAGER: ULONG = 64;
pub const DOKAN_OPTION_CURRENT_SESSION: ULONG = 128;
pub const DOKAN_OPTION_FILELOCK_USER_MODE: ULONG = 256;

use std::mem;

#[repr(C)]
pub struct DOKAN_OPTIONS {
    pub Version: USHORT,
    pub ThreadCount: USHORT,
    pub Options: ULONG,
    pub GlobalContext: ULONG64,
    pub MountPoint: LPCWSTR,
    pub UNCName: LPCWSTR,
    pub Timeout: ULONG,
    pub AllocationUnitSize: ULONG,
    pub SectorSize: ULONG,
}

impl DOKAN_OPTIONS {
    pub fn zero() -> DOKAN_OPTIONS {
        let mut res: DOKAN_OPTIONS = unsafe { mem::zeroed() };
        res.Version = unsafe { CONST_DOKAN_VERSION };
        res
    }
}

pub type PDOKAN_OPTIONS = *mut DOKAN_OPTIONS;

#[repr(C)]
pub struct DOKAN_FILE_INFO {
    pub Context: ULONG64,
    pub DokanContext: ULONG64,
    pub DokanOptions: PDOKAN_OPTIONS,
    pub ProcessId: ULONG,
    pub IsDirectory: UCHAR,
    pub DeleteOnClose: UCHAR,
    pub PagingIo: UCHAR,
    pub SynchronousIo: UCHAR,
    pub Nocache: UCHAR,
    pub WriteToEndOfFile: UCHAR,
}

pub type PDOKAN_FILE_INFO = *mut DOKAN_FILE_INFO;

#[repr(C)]
pub struct DOKAN_ACCESS_STATE {
    pub SecurityEvaluated: BOOLEAN,
    pub GenerateAudit: BOOLEAN,
    pub GenerateOnClose: BOOLEAN,
    pub AuditPrivileges: BOOLEAN,
    pub Flags: ULONG,
    pub RemainingDesiredAccess: ACCESS_MASK,
    pub PreviouslyGrantedAccess: ACCESS_MASK,
    pub OriginalDesiredAccess: ACCESS_MASK,
    pub SecurityDescriptor: PSECURITY_DESCRIPTOR,
    pub ObjectName: UNICODE_STRING,
    pub ObjectType: UNICODE_STRING,
}

#[repr(C)]
pub struct DOKAN_IO_SECURITY_CONTEXT {
    pub AccessState: DOKAN_ACCESS_STATE,
    pub DesiredAccess: ACCESS_MASK,
}

pub type PDOKAN_IO_SECURITY_CONTEXT = *mut DOKAN_IO_SECURITY_CONTEXT;

pub type PDOKAN_OPERATIONS = *mut VOID;

#[derive(Debug)]
pub enum DokanResult {
    Success = 0,
    Error = -1,
    DriveLetterError = -2,
    DriverInstallError = -3,
    StartError = -4,
    MountError = -5,
    MountPointError = -6,
    VersionError = -7,
}

impl TryFrom<i32> for DokanResult {
    type Error = i32;
    fn try_from(value: i32) -> Result<Self, <Self as TryFrom<i32>>::Error> {
        match value {
            0 => Ok(DokanResult::Success),
            -1 => Ok(DokanResult::Success),
            -2 => Ok(DokanResult::Success),
            -3 => Ok(DokanResult::Success),
            -4 => Ok(DokanResult::Success),
            -5 => Ok(DokanResult::Success),
            -6 => Ok(DokanResult::Success),
            -7 => Ok(DokanResult::Success),
            i => Err(i),
        }
    }
}

trait DokanWrite {
    fn zw_create_file(
        path: LPCWSTR,
        context: PDOKAN_IO_SECURITY_CONTEXT,
        access: ACCESS_MASK,
        attributes: ULONG,
        shared: ULONG,
        disposition: ULONG,
        options: ULONG,
        info: PDOKAN_FILE_INFO,
    ) -> NTSTATUS;
    fn cleanup(path: LPCWSTR, info: PDOKAN_FILE_INFO);
    fn write_file(
        path: LPCWSTR,
        buffer: LPCVOID,
        len: DWORD,
        bytes_written: LPDWORD,
        offset: LONGLONG,
        info: PDOKAN_FILE_INFO,
    ) -> NTSTATUS;
    fn set_file_attributes(
        path: LPCWSTR,
        attributes: DWORD,
        info: PDOKAN_FILE_INFO,
    ) -> NTSTATUS;
    fn set_file_time(
        path: LPCWSTR,
        creation: *const FILETIME,
        access: *const FILETIME,
        write: *const FILETIME,
        info: PDOKAN_FILE_INFO,
    ) -> NTSTATUS;
    fn delete_file(path: LPCWSTR, info: PDOKAN_FILE_INFO) -> NTSTATUS;
    fn delete_directory(path: LPCWSTR, info: PDOKAN_FILE_INFO) -> NTSTATUS;
    fn move_file(
        path: LPCWSTR,
        new_name: LPCWSTR,
        replace: BOOL,
        info: PDOKAN_FILE_INFO,
    ) -> NTSTATUS;
    fn set_end_of_file(
        path: LPCWSTR,
        offset: LONGLONG,
        info: PDOKAN_FILE_INFO,
    ) -> NTSTATUS;
    fn set_allocation_size(
        path: LPCWSTR,
        size: LONGLONG,
        info: PDOKAN_FILE_INFO,
    ) -> NTSTATUS;
    fn set_file_security(
        path: LPCWSTR,
        security: PSECURITY_INFORMATION,
        descriptor: PSECURITY_DESCRIPTOR,
        length: ULONG,
        info: PDOKAN_FILE_INFO,
    ) -> NTSTATUS;
    fn flush_file_buffers(path: LPCWSTR, info: PDOKAN_FILE_INFO) -> NTSTATUS;
}

#[link(name = "dokan1")]
extern "stdcall" {
    pub fn DokanMapKernelToUserCreateFileFlags(
        DesiredAccess: ACCESS_MASK,
        FileAttributes: ULONG,
        CreateOptions: ULONG,
        CreateDisposition: ULONG,
        outDesiredAccess: *mut ACCESS_MASK,
        outFileAttributesAndFlags: *mut DWORD,
        outCreationDisposition: *mut DWORD,
    );
    pub fn DokanOpenRequestorToken(info: PDOKAN_FILE_INFO) -> HANDLE;
    pub fn DokanNtStatusFromWin32(error: DWORD) -> NTSTATUS;
    pub fn DokanMain(
        DokanOptions: PDOKAN_OPTIONS,
        DokanOperations: PDOKAN_OPERATIONS,
    ) -> i32;
    pub fn DokanRemoveMountPoint(MountPoint: LPCWSTR) -> BOOL;
}

#[link(name = "helper", kind = "static")]
extern "stdcall" {
    static CONST_DOKAN_VERSION: u16;
    pub fn AddPrivileges() -> BOOL;
}

static mut MOUNT_POINT: LPCWSTR = ptr::null();

unsafe extern "system" fn handler(ctrl_type: DWORD) -> BOOL {
    match ctrl_type {
        CTRL_C_EVENT | CTRL_BREAK_EVENT | CTRL_CLOSE_EVENT
        | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT => {
            println!("Handler fired");
            SetConsoleCtrlHandler(Some(handler), 0);
            DokanRemoveMountPoint(MOUNT_POINT);
            1
        }
        _ => 0,
    }
}

pub unsafe fn dokan_main(
    mut options: DOKAN_OPTIONS,
    ops: PDOKAN_OPERATIONS,
) -> Result<DokanResult, i32> {
    MOUNT_POINT = options.MountPoint;
    if SetConsoleCtrlHandler(Some(handler), 1) == 0 {
        panic!("Failed to set dokan exit handler");
    }
    let res = DokanMain(&mut options as *mut _, ops);
    DokanResult::try_from(res)
}
