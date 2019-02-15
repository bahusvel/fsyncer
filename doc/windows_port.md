# Op categories

## Read

CloseFile (LPCWSTR FileName, PDOKAN_FILE_INFO DokanFileInfo);

ReadFile (LPCWSTR FileName, LPVOID Buffer, DWORD BufferLength, LPDWORD ReadLength, LONGLONG Offset, PDOKAN_FILE_INFO DokanFileInfo);

GetFileInformation (LPCWSTR FileName, LPBY_HANDLE_FILE_INFORMATION Buffer, PDOKAN_FILE_INFO DokanFileInfo);

FindFiles (LPCWSTR FileName, PFillFindData FillFindData, PDOKAN_FILE_INFO DokanFileInfo);

FindFilesWithPattern (LPCWSTR PathName, LPCWSTR SearchPattern, PFillFindData FillFindData, PDOKAN_FILE_INFO DokanFileInfo);

GetFileSecurity (LPCWSTR FileName, PSECURITY_INFORMATION SecurityInformation, PSECURITY_DESCRIPTOR SecurityDescriptor, ULONG BufferLength, PULONG LengthNeeded, PDOKAN_FILE_INFO DokanFileInfo);

GetDiskFreeSpace (PULONGLONG FreeBytesAvailable, PULONGLONG TotalNumberOfBytes, PULONGLONG TotalNumberOfFreeBytes, PDOKAN_FILE_INFO DokanFileInfo);

GetVolumeInformation (LPWSTR VolumeNameBuffer, DWORD VolumeNameSize, LPDWORD VolumeSerialNumber, LPDWORD MaximumComponentLength, LPDWORD FileSystemFlags, LPWSTR FileSystemNameBuffer, DWORD FileSystemNameSize, PDOKAN_FILE_INFO DokanFileInfo);

FindStreams (LPCWSTR FileName, PFillFindStreamData FillFindStreamData, PDOKAN_FILE_INFO DokanFileInfo);

## Write

ZwCreateFile (LPCWSTR FileName, PDOKAN_IO_SECURITY_CONTEXT SecurityContext, ACCESS_MASK DesiredAccess, ULONG FileAttributes, ULONG ShareAccess, ULONG CreateDisposition, ULONG CreateOptions, PDOKAN_FILE_INFO DokanFileInfo);

Cleanup (LPCWSTR FileName, PDOKAN_FILE_INFO DokanFileInfo); // Has some funky delete on close behaviour

WriteFile (LPCWSTR FileName, LPCVOID Buffer, DWORD NumberOfBytesToWrite, LPDWORD NumberOfBytesWritten, LONGLONG Offset, PDOKAN_FILE_INFO DokanFileInfo);

SetFileAttributes (LPCWSTR FileName, DWORD FileAttributes, PDOKAN_FILE_INFO DokanFileInfo);

SetFileTime (LPCWSTR FileName, CONST FILETIME *CreationTime, CONST FILETIME *LastAccessTime, CONST FILETIME *LastWriteTime, PDOKAN_FILE_INFO DokanFileInfo);

DeleteFile (LPCWSTR FileName, PDOKAN_FILE_INFO DokanFileInfo);

DeleteDirectory (LPCWSTR FileName, PDOKAN_FILE_INFO DokanFileInfo);

MoveFile (LPCWSTR FileName, LPCWSTR NewFileName, BOOL ReplaceIfExisting, PDOKAN_FILE_INFO DokanFileInfo);

SetEndOfFile (LPCWSTR FileName, LONGLONG ByteOffset, PDOKAN_FILE_INFO DokanFileInfo);

SetAllocationSize (LPCWSTR FileName, LONGLONG AllocSize, PDOKAN_FILE_INFO DokanFileInfo);

SetFileSecurity (LPCWSTR FileName, PSECURITY_INFORMATION SecurityInformation, PSECURITY_DESCRIPTOR SecurityDescriptor, ULONG BufferLength, PDOKAN_FILE_INFO DokanFileInfo);

## Write but not relevant for replication

FlushFileBuffers (LPCWSTR FileName, PDOKAN_FILE_INFO DokanFileInfo); 

// If these are handled by the kernel this is fine, fsyncer does not need to handle these
LockFile (LPCWSTR FileName, LONGLONG ByteOffset, LONGLONG Length, PDOKAN_FILE_INFO DokanFileInfo);
UnlockFile (LPCWSTR FileName, LONGLONG ByteOffset, LONGLONG Length, PDOKAN_FILE_INFO DokanFileInfo);

# Similarity to POSIX vfs ops

ZwCreateFile and SetFileSecurity will get their own vfs ops, the rest can be fitted into current ones, perhaps with one or two additional fields if neccessary.

## Similar but different

ZwCreateFile (LPCWSTR FileName, PDOKAN_IO_SECURITY_CONTEXT SecurityContext, ACCESS_MASK DesiredAccess, ULONG FileAttributes, ULONG ShareAccess, ULONG CreateDisposition, ULONG CreateOptions, PDOKAN_FILE_INFO DokanFileInfo); -> create, but contains stat information, Linux just assumes instead. Could be useful to extend Create, but Windows also has enough weird fields in there to be treated differently.

SetFileSecurity (LPCWSTR FileName, PSECURITY_INFORMATION SecurityInformation, PSECURITY_DESCRIPTOR SecurityDescriptor, ULONG BufferLength, PDOKAN_FILE_INFO DokanFileInfo); -> this is like chown and chmod combined, but with extra windows stuff, ultimately these are not compatible and need intelligent translation.

SetFileTime (LPCWSTR FileName, CONST FILETIME *CreationTime, CONST FILETIME *LastAccessTime, CONST FILETIME *LastWriteTime, PDOKAN_FILE_INFO DokanFileInfo) -> utimens, with the exception of creation time, that does not exist in Linux

SetFileAttributes, similar to chmod, so perhaps it could contain this information. Because mode on linux is also uin32_t/DWORD.

## Unique to Windows

SetAllocationSize

## Identical

cleanup -> release ??? This is questionable, I need to figure out what that delete on close behaviour is about
DeleteFile -> unlink
DeleteDirectory -> rmdir
MoveFile -> rename
WriteFile -> write
SetEndOfFile -> truncate

