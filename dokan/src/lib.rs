extern crate winapi;
use winapi::shared::{
    basetsd::*,
    minwindef::{BOOL, DWORD, FILETIME, LPCVOID, LPDWORD},
    ntdef::*,
};
use winapi::um::winnt::{ACCESS_MASK, PSECURITY_DESCRIPTOR, PSECURITY_INFORMATION};

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
    fn set_file_attributes(path: LPCWSTR, attributes: DWORD, info: PDOKAN_FILE_INFO) -> NTSTATUS;
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
    fn set_end_of_file(path: LPCWSTR, offset: LONGLONG, info: PDOKAN_FILE_INFO) -> NTSTATUS;
    fn set_allocation_size(path: LPCWSTR, size: LONGLONG, info: PDOKAN_FILE_INFO) -> NTSTATUS;
    fn set_file_security(
        path: LPCWSTR,
        security: PSECURITY_INFORMATION,
        descriptor: PSECURITY_DESCRIPTOR,
        length: ULONG,
        info: PDOKAN_FILE_INFO,
    ) -> NTSTATUS;
    fn flush_file_buffers(path: LPCWSTR, info: PDOKAN_FILE_INFO) -> NTSTATUS;
}
