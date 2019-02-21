#pragma warning(push)
#pragma warning(disable : 4820)
#include "dokan/dokan.h"
#pragma warning(pop)

NTSTATUS DOKAN_CALLBACK MirrorCloseFile(LPCWSTR FileName,
										PDOKAN_FILE_INFO DokanFileInfo);

NTSTATUS DOKAN_CALLBACK MirrorReadFile(LPCWSTR FileName, LPVOID Buffer,
									   DWORD BufferLength, LPDWORD ReadLength,
									   LONGLONG Offset,
									   PDOKAN_FILE_INFO DokanFileInfo);

NTSTATUS DOKAN_CALLBACK
MirrorGetFileInformation(LPCWSTR FileName, LPBY_HANDLE_FILE_INFORMATION Buffer,
						 PDOKAN_FILE_INFO DokanFileInfo);

NTSTATUS DOKAN_CALLBACK MirrorFindFiles(LPCWSTR FileName,
										PFillFindData FillFindData,
										PDOKAN_FILE_INFO DokanFileInfo);

NTSTATUS DOKAN_CALLBACK MirrorFindFilesWithPattern(
	LPCWSTR PathName, LPCWSTR SearchPattern, PFillFindData FillFindData,
	PDOKAN_FILE_INFO DokanFileInfo);

NTSTATUS DOKAN_CALLBACK MirrorGetFileSecurity(
	LPCWSTR FileName, PSECURITY_INFORMATION SecurityInformation,
	PSECURITY_DESCRIPTOR SecurityDescriptor, ULONG BufferLength,
	PULONG LengthNeeded, PDOKAN_FILE_INFO DokanFileInfo);

NTSTATUS DOKAN_CALLBACK MirrorGetDiskFreeSpace(
	PULONGLONG FreeBytesAvailable, PULONGLONG TotalNumberOfBytes,
	PULONGLONG TotalNumberOfFreeBytes, PDOKAN_FILE_INFO DokanFileInfo);

NTSTATUS DOKAN_CALLBACK MirrorGetVolumeInformation(
	LPWSTR VolumeNameBuffer, DWORD VolumeNameSize, LPDWORD VolumeSerialNumber,
	LPDWORD MaximumComponentLength, LPDWORD FileSystemFlags,
	LPWSTR FileSystemNameBuffer, DWORD FileSystemNameSize,
	PDOKAN_FILE_INFO DokanFileInfo);

NTSTATUS DOKAN_CALLBACK
MirrorFindStreams(LPCWSTR FileName, PFillFindStreamData FillFindStreamData,
				  PDOKAN_FILE_INFO DokanFileInfo);
NTSTATUS DOKAN_CALLBACK
MirrorCreateFile(LPCWSTR FileName, PDOKAN_IO_SECURITY_CONTEXT SecurityContext,
				 ACCESS_MASK DesiredAccess, ULONG FileAttributes,
				 ULONG ShareAccess, ULONG CreateDisposition,
				 ULONG CreateOptions, PDOKAN_FILE_INFO DokanFileInfo);

DOKAN_CALLBACK MirrorCleanup(
	LPCWSTR FileName,
	PDOKAN_FILE_INFO DokanFileInfo); // Has some funky delete on close behaviour

NTSTATUS DOKAN_CALLBACK MirrorWriteFile(LPCWSTR FileName, LPCVOID Buffer,
										DWORD NumberOfBytesToWrite,
										LPDWORD NumberOfBytesWritten,
										LONGLONG Offset,
										PDOKAN_FILE_INFO DokanFileInfo);

NTSTATUS DOKAN_CALLBACK MirrorSetFileAttributes(LPCWSTR FileName,
												DWORD FileAttributes,
												PDOKAN_FILE_INFO DokanFileInfo);

NTSTATUS DOKAN_CALLBACK MirrorSetFileTime(LPCWSTR FileName,
										  CONST FILETIME *CreationTime,
										  CONST FILETIME *LastAccessTime,
										  CONST FILETIME *LastWriteTime,
										  PDOKAN_FILE_INFO DokanFileInfo);

NTSTATUS DOKAN_CALLBACK MirrorDeleteFile(LPCWSTR FileName,
										 PDOKAN_FILE_INFO DokanFileInfo);

NTSTATUS DOKAN_CALLBACK MirrorDeleteDirectory(LPCWSTR FileName,
											  PDOKAN_FILE_INFO DokanFileInfo);

NTSTATUS DOKAN_CALLBACK MirrorMoveFile(LPCWSTR FileName, LPCWSTR NewFileName,
									   BOOL ReplaceIfExisting,
									   PDOKAN_FILE_INFO DokanFileInfo);

NTSTATUS DOKAN_CALLBACK MirrorSetEndOfFile(LPCWSTR FileName,
										   LONGLONG ByteOffset,
										   PDOKAN_FILE_INFO DokanFileInfo);

NTSTATUS DOKAN_CALLBACK MirrorSetAllocationSize(LPCWSTR FileName,
												LONGLONG AllocSize,
												PDOKAN_FILE_INFO DokanFileInfo);

NTSTATUS DOKAN_CALLBACK MirrorSetFileSecurity(
	LPCWSTR FileName, PSECURITY_INFORMATION SecurityInformation,
	PSECURITY_DESCRIPTOR SecurityDescriptor, ULONG BufferLength,
	PDOKAN_FILE_INFO DokanFileInfo);
NTSTATUS DOKAN_CALLBACK MirrorFlushFileBuffers(LPCWSTR FileName,
											   PDOKAN_FILE_INFO DokanFileInfo);

NTSTATUS DOKAN_CALLBACK MirrorLockFile(LPCWSTR FileName, LONGLONG ByteOffset,
									   LONGLONG Length,
									   PDOKAN_FILE_INFO DokanFileInfo);
NTSTATUS DOKAN_CALLBACK MirrorUnlockFile(LPCWSTR FileName, LONGLONG ByteOffset,
										 LONGLONG Length,
										 PDOKAN_FILE_INFO DokanFileInfo);

NTSTATUS DOKAN_CALLBACK MirrorMounted(PDOKAN_FILE_INFO DokanFileInfo);
NTSTATUS DOKAN_CALLBACK MirrorUnmounted(PDOKAN_FILE_INFO DokanFileInfo);

const DOKAN_OPERATIONS DOKAN_OPS = {
	.ZwCreateFile = MirrorCreateFile,
	.Cleanup = MirrorCleanup,
	.CloseFile = MirrorCloseFile,
	.ReadFile = MirrorReadFile,
	.WriteFile = MirrorWriteFile,
	.FlushFileBuffers = MirrorFlushFileBuffers,
	.GetFileInformation = MirrorGetFileInformation,
	.FindFiles = MirrorFindFiles,
	.FindFilesWithPattern = NULL,
	.SetFileAttributes = MirrorSetFileAttributes,
	.SetFileTime = MirrorSetFileTime,
	.DeleteFile = MirrorDeleteFile,
	.DeleteDirectory = MirrorDeleteDirectory,
	.MoveFile = MirrorMoveFile,
	.SetEndOfFile = MirrorSetEndOfFile,
	.SetAllocationSize = MirrorSetAllocationSize,
	.LockFile = MirrorLockFile,
	.UnlockFile = MirrorUnlockFile,
	.GetFileSecurity = MirrorGetFileSecurity,
	.SetFileSecurity = MirrorSetFileSecurity,
	.GetDiskFreeSpace = NULL, // MirrorDokanGetDiskFreeSpace;
	.GetVolumeInformation = MirrorGetVolumeInformation,
	.Unmounted = MirrorUnmounted,
	.FindStreams = MirrorFindStreams,
	.Mounted = MirrorMounted,
};
const PDOKAN_OPERATIONS DOKAN_OPS_PTR = &DOKAN_OPS;