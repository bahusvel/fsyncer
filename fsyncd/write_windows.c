/*
  Dokan : user-mode file system library for Windows

  Copyright (C) 2015 - 2019 Adrien J. <liryna.stark@gmail.com> and Maxime C.
<maxime@islog.com> Copyright (C) 2007 - 2011 Hiroki Asakawa <info@dokan-dev.net>

  http://dokan-dev.github.io

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
*/

#include "dokan/dokan.h"
#include "dokan/fileinfo.h"
#include <malloc.h>
#include <stdio.h>
#include <stdlib.h>
#include <winbase.h>

//#define WIN10_ENABLE_LONG_PATH
#ifdef WIN10_ENABLE_LONG_PATH
// dirty but should be enough
#define DOKAN_MAX_PATH 32768
#else
#define DOKAN_MAX_PATH MAX_PATH
#endif // DEBUG

BOOL g_UseStdErr;
BOOL g_DebugMode;
BOOL g_HasSeSecurityPrivilege;
BOOL g_ImpersonateCallerUser;

static WCHAR RootDirectory[DOKAN_MAX_PATH] = L"C:";
static WCHAR UNCName[DOKAN_MAX_PATH] = L"";

static void DbgPrint(LPCWSTR format, ...) {
	if (g_DebugMode) {
		const WCHAR *outputString;
		WCHAR *buffer = NULL;
		size_t length;
		va_list argp;

		va_start(argp, format);
		length = _vscwprintf(format, argp) + 1;
		buffer = _malloca(length * sizeof(WCHAR));
		if (buffer) {
			vswprintf_s(buffer, length, format, argp);
			outputString = buffer;
		} else {
			outputString = format;
		}
		if (g_UseStdErr)
			fputws(outputString, stderr);
		else
			OutputDebugStringW(outputString);
		if (buffer)
			_freea(buffer);
		va_end(argp);
		if (g_UseStdErr)
			fflush(stderr);
	}
}

static void GetFilePath(PWCHAR filePath, ULONG numberOfElements,
						LPCWSTR FileName) {
	wcsncpy_s(filePath, numberOfElements, RootDirectory, wcslen(RootDirectory));
	size_t unclen = wcslen(UNCName);
	if (unclen > 0 && _wcsnicmp(FileName, UNCName, unclen) == 0) {
		if (_wcsnicmp(FileName + unclen, L".", 1) != 0) {
			wcsncat_s(filePath, numberOfElements, FileName + unclen,
					  wcslen(FileName) - unclen);
		}
	} else {
		wcsncat_s(filePath, numberOfElements, FileName, wcslen(FileName));
	}
}

NTSTATUS MirrorCreateFile(LPCWSTR FileName,
						  PSECURITY_DESCRIPTOR SecurityDescriptor,
						  ACCESS_MASK DesiredAccess, ULONG FileAttributes,
						  ULONG ShareAccess, ULONG CreateDisposition,
						  ULONG CreateOptions, PDOKAN_FILE_INFO DokanFileInfo) {
	WCHAR filePath[DOKAN_MAX_PATH];
	HANDLE handle;
	DWORD fileAttr;
	NTSTATUS status = STATUS_SUCCESS;
	DWORD creationDisposition;
	DWORD fileAttributesAndFlags;
	DWORD error = 0;
	SECURITY_ATTRIBUTES securityAttrib;
	ACCESS_MASK genericDesiredAccess;
	// userTokenHandle is for Impersonate Caller User Option
	HANDLE userTokenHandle = INVALID_HANDLE_VALUE;

	securityAttrib.nLength = sizeof(securityAttrib);
	securityAttrib.lpSecurityDescriptor = SecurityDescriptor;
	securityAttrib.bInheritHandle = FALSE;

	DokanMapKernelToUserCreateFileFlags(
		DesiredAccess, FileAttributes, CreateOptions, CreateDisposition,
		&genericDesiredAccess, &fileAttributesAndFlags, &creationDisposition);

	GetFilePath(filePath, DOKAN_MAX_PATH, FileName);

	/*
	if (ShareMode == 0 && AccessMode & FILE_WRITE_DATA)
			ShareMode = FILE_SHARE_WRITE;
	else if (ShareMode == 0)
			ShareMode = FILE_SHARE_READ;
	*/

	// When filePath is a directory, needs to change the flag so that the file
	// can be opened.
	fileAttr = GetFileAttributes(filePath);

	if (fileAttr != INVALID_FILE_ATTRIBUTES &&
		fileAttr & FILE_ATTRIBUTE_DIRECTORY) {
		if (!(CreateOptions & FILE_NON_DIRECTORY_FILE)) {
			DokanFileInfo->IsDirectory = TRUE;
			// Needed by FindFirstFile to list files in it
			// TODO: use ReOpenFile in MirrorFindFiles to set share read
			// temporary
			ShareAccess |= FILE_SHARE_READ;
		} else { // FILE_NON_DIRECTORY_FILE - Cannot open a dir as a file
			return STATUS_FILE_IS_A_DIRECTORY;
		}
	}

	if (g_ImpersonateCallerUser) {
		userTokenHandle = DokanOpenRequestorToken(DokanFileInfo);
		if (userTokenHandle == INVALID_HANDLE_VALUE) {
			// Should we return some error?
		}
	}

	if (DokanFileInfo->IsDirectory) {
		// It is a create directory request
		if (creationDisposition == CREATE_NEW ||
			creationDisposition == OPEN_ALWAYS) {

			if (g_ImpersonateCallerUser &&
				userTokenHandle != INVALID_HANDLE_VALUE) {
				// if g_ImpersonateCallerUser option is on, call the
				// ImpersonateLoggedOnUser function.
				if (!ImpersonateLoggedOnUser(userTokenHandle)) {
					// handle the error if failed to impersonate
				}
			}

			// We create folder
			if (!CreateDirectory(filePath, &securityAttrib)) {
				error = GetLastError();
				// Fail to create folder for OPEN_ALWAYS is not an error
				if (error != ERROR_ALREADY_EXISTS ||
					creationDisposition == CREATE_NEW) {

					status = DokanNtStatusFromWin32(error);
				}
			}

			if (g_ImpersonateCallerUser &&
				userTokenHandle != INVALID_HANDLE_VALUE) {
				// Clean Up operation for impersonate
				DWORD lastError = GetLastError();
				if (status !=
					STATUS_SUCCESS) // Keep the handle open for CreateFile
					CloseHandle(userTokenHandle);
				RevertToSelf();
				SetLastError(lastError);
			}
		}

		if (status == STATUS_SUCCESS) {

			// Check first if we're trying to open a file as a directory.
			if (fileAttr != INVALID_FILE_ATTRIBUTES &&
				!(fileAttr & FILE_ATTRIBUTE_DIRECTORY) &&
				(CreateOptions & FILE_DIRECTORY_FILE)) {
				return STATUS_NOT_A_DIRECTORY;
			}

			if (g_ImpersonateCallerUser &&
				userTokenHandle != INVALID_HANDLE_VALUE) {
				// if g_ImpersonateCallerUser option is on, call the
				// ImpersonateLoggedOnUser function.
				if (!ImpersonateLoggedOnUser(userTokenHandle)) {
					// handle the error if failed to impersonate
				}
			}

			// FILE_FLAG_BACKUP_SEMANTICS is required for opening directory
			// handles
			handle = CreateFile(
				filePath, genericDesiredAccess, ShareAccess, &securityAttrib,
				OPEN_EXISTING,
				fileAttributesAndFlags | FILE_FLAG_BACKUP_SEMANTICS, NULL);

			if (g_ImpersonateCallerUser &&
				userTokenHandle != INVALID_HANDLE_VALUE) {
				// Clean Up operation for impersonate
				DWORD lastError = GetLastError();
				CloseHandle(userTokenHandle);
				RevertToSelf();
				SetLastError(lastError);
			}

			if (handle == INVALID_HANDLE_VALUE) {
				error = GetLastError();

				status = DokanNtStatusFromWin32(error);
			} else {
				DokanFileInfo->Context =
					(ULONG64)handle; // save the file handle in Context

				// Open succeed but we need to inform the driver
				// that the dir open and not created by returning
				// STATUS_OBJECT_NAME_COLLISION
				if (creationDisposition == OPEN_ALWAYS &&
					fileAttr != INVALID_FILE_ATTRIBUTES)
					return STATUS_OBJECT_NAME_COLLISION;
			}
		}
	} else {
		// It is a create file request
		// Cannot overwrite a hidden or system file if flag not set
		if (fileAttr != INVALID_FILE_ATTRIBUTES &&
			((!(fileAttributesAndFlags & FILE_ATTRIBUTE_HIDDEN) &&
			  (fileAttr & FILE_ATTRIBUTE_HIDDEN)) ||
			 (!(fileAttributesAndFlags & FILE_ATTRIBUTE_SYSTEM) &&
			  (fileAttr & FILE_ATTRIBUTE_SYSTEM))) &&
			(creationDisposition == TRUNCATE_EXISTING ||
			 creationDisposition == CREATE_ALWAYS))
			return STATUS_ACCESS_DENIED;

		// Cannot delete a read only file
		if ((fileAttr != INVALID_FILE_ATTRIBUTES &&
				 (fileAttr & FILE_ATTRIBUTE_READONLY) ||
			 (fileAttributesAndFlags & FILE_ATTRIBUTE_READONLY)) &&
			(fileAttributesAndFlags & FILE_FLAG_DELETE_ON_CLOSE))
			return STATUS_CANNOT_DELETE;

		// Truncate should always be used with write access
		if (creationDisposition == TRUNCATE_EXISTING)
			genericDesiredAccess |= GENERIC_WRITE;

		if (g_ImpersonateCallerUser &&
			userTokenHandle != INVALID_HANDLE_VALUE) {
			// if g_ImpersonateCallerUser option is on, call the
			// ImpersonateLoggedOnUser function.
			if (!ImpersonateLoggedOnUser(userTokenHandle)) {
				// handle the error if failed to impersonate
			}
		}

		handle = CreateFile(
			filePath,
			genericDesiredAccess, // GENERIC_READ|GENERIC_WRITE|GENERIC_EXECUTE,
			ShareAccess,
			&securityAttrib, // security attribute
			creationDisposition,
			fileAttributesAndFlags, // |FILE_FLAG_NO_BUFFERING,
			NULL);					// template file handle

		if (g_ImpersonateCallerUser &&
			userTokenHandle != INVALID_HANDLE_VALUE) {
			// Clean Up operation for impersonate
			DWORD lastError = GetLastError();
			CloseHandle(userTokenHandle);
			RevertToSelf();
			SetLastError(lastError);
		}

		if (handle == INVALID_HANDLE_VALUE) {
			error = GetLastError();

			status = DokanNtStatusFromWin32(error);
		} else {
			// Need to update FileAttributes with previous when Overwrite file
			if (fileAttr != INVALID_FILE_ATTRIBUTES &&
				creationDisposition == TRUNCATE_EXISTING) {
				SetFileAttributes(filePath, fileAttributesAndFlags | fileAttr);
			}

			DokanFileInfo->Context =
				(ULONG64)handle; // save the file handle in Context

			if (creationDisposition == OPEN_ALWAYS ||
				creationDisposition == CREATE_ALWAYS) {
				error = GetLastError();
				if (error == ERROR_ALREADY_EXISTS) {

					// Open succeed but we need to inform the driver
					// that the file open and not created by returning
					// STATUS_OBJECT_NAME_COLLISION
					status = STATUS_OBJECT_NAME_COLLISION;
				}
			}
		}
	}

	return status;
}

#pragma warning(push)
#pragma warning(disable : 4305)

static void MirrorCleanup(LPCWSTR FileName, PDOKAN_FILE_INFO DokanFileInfo) {
	WCHAR filePath[DOKAN_MAX_PATH];
	GetFilePath(filePath, DOKAN_MAX_PATH, FileName);

	if (DokanFileInfo->Context) {
		CloseHandle((HANDLE)(DokanFileInfo->Context));
		DokanFileInfo->Context = 0;
	}
	if (DokanFileInfo->DeleteOnClose) {
		// Should already be deleted by CloseHandle
		// if open with FILE_FLAG_DELETE_ON_CLOSE
		if (DokanFileInfo->IsDirectory) {
			if (!RemoveDirectory(filePath)) {
				DbgPrint(L"error code = %d\n\n", GetLastError());
			}
		} else {
			if (DeleteFile(filePath) == 0) {
				DbgPrint(L" error code = %d\n\n", GetLastError());
			}
		}
	}
}

NTSTATUS MirrorWriteFile(LPCWSTR FileName, LPCVOID Buffer,
						 DWORD NumberOfBytesToWrite,
						 LPDWORD NumberOfBytesWritten, LONGLONG Offset,
						 PDOKAN_FILE_INFO DokanFileInfo) {
	WCHAR filePath[DOKAN_MAX_PATH];
	HANDLE handle = (HANDLE)DokanFileInfo->Context;
	BOOL opened = FALSE;

	GetFilePath(filePath, DOKAN_MAX_PATH, FileName);

	// reopen the file
	if (!handle || handle == INVALID_HANDLE_VALUE) {
		handle = CreateFile(filePath, GENERIC_WRITE, FILE_SHARE_WRITE, NULL,
							OPEN_EXISTING, 0, NULL);
		if (handle == INVALID_HANDLE_VALUE) {
			DWORD error = GetLastError();

			return DokanNtStatusFromWin32(error);
		}
		opened = TRUE;
	}

	UINT64 fileSize = 0;
	DWORD fileSizeLow = 0;
	DWORD fileSizeHigh = 0;
	fileSizeLow = GetFileSize(handle, &fileSizeHigh);
	if (fileSizeLow == INVALID_FILE_SIZE) {
		DWORD error = GetLastError();

		if (opened)
			CloseHandle(handle);
		return DokanNtStatusFromWin32(error);
	}

	fileSize = ((UINT64)fileSizeHigh << 32) | fileSizeLow;

	LARGE_INTEGER distanceToMove;
	if (DokanFileInfo->WriteToEndOfFile) {
		LARGE_INTEGER z;
		z.QuadPart = 0;
		if (!SetFilePointerEx(handle, z, NULL, FILE_END)) {
			DWORD error = GetLastError();

			if (opened)
				CloseHandle(handle);
			return DokanNtStatusFromWin32(error);
		}
	} else {
		// Paging IO cannot write after allocate file size.
		if (DokanFileInfo->PagingIo) {
			if ((UINT64)Offset >= fileSize) {
				*NumberOfBytesWritten = 0;
				if (opened)
					CloseHandle(handle);
				return STATUS_SUCCESS;
			}

			if (((UINT64)Offset + NumberOfBytesToWrite) > fileSize) {
				UINT64 bytes = fileSize - Offset;
				if (bytes >> 32) {
					NumberOfBytesToWrite = (DWORD)(bytes & 0xFFFFFFFFUL);
				} else {
					NumberOfBytesToWrite = (DWORD)bytes;
				}
			}
		}

		if ((UINT64)Offset > fileSize) {
			// In the mirror sample helperZeroFileData is not necessary. NTFS
			// will zero a hole. But if user's file system is different from
			// NTFS( or other Windows's file systems ) then  users will have to
			// zero the hole themselves.
		}

		distanceToMove.QuadPart = Offset;
		if (!SetFilePointerEx(handle, distanceToMove, NULL, FILE_BEGIN)) {
			DWORD error = GetLastError();
			if (opened)
				CloseHandle(handle);
			return DokanNtStatusFromWin32(error);
		}
	}

	if (!WriteFile(handle, Buffer, NumberOfBytesToWrite, NumberOfBytesWritten,
				   NULL)) {
		DWORD error = GetLastError();
		if (opened)
			CloseHandle(handle);
		return DokanNtStatusFromWin32(error);
	}

	// close the file when it is reopened
	if (opened)
		CloseHandle(handle);

	return STATUS_SUCCESS;
}

NTSTATUS MirrorFlushFileBuffers(LPCWSTR FileName, HANDLE handle) {
	WCHAR filePath[DOKAN_MAX_PATH];

	GetFilePath(filePath, DOKAN_MAX_PATH, FileName);

	if (!handle || handle == INVALID_HANDLE_VALUE) {
		return STATUS_SUCCESS;
	}

	if (FlushFileBuffers(handle)) {
		return STATUS_SUCCESS;
	} else {
		DWORD error = GetLastError();
		return DokanNtStatusFromWin32(error);
	}
}

NTSTATUS MirrorDeleteFile(LPCWSTR FileName, PDOKAN_FILE_INFO DokanFileInfo) {
	WCHAR filePath[DOKAN_MAX_PATH];
	HANDLE handle = (HANDLE)DokanFileInfo->Context;

	GetFilePath(filePath, DOKAN_MAX_PATH, FileName);

	DWORD dwAttrib = GetFileAttributes(filePath);

	if (dwAttrib != INVALID_FILE_ATTRIBUTES &&
		(dwAttrib & FILE_ATTRIBUTE_DIRECTORY))
		return STATUS_ACCESS_DENIED;

	if (handle && handle != INVALID_HANDLE_VALUE) {
		FILE_DISPOSITION_INFO fdi;
		fdi.DeleteFile = DokanFileInfo->DeleteOnClose;
		if (!SetFileInformationByHandle(handle, FileDispositionInfo, &fdi,
										sizeof(FILE_DISPOSITION_INFO)))
			return DokanNtStatusFromWin32(GetLastError());
	}

	return STATUS_SUCCESS;
}

NTSTATUS MirrorDeleteDirectory(LPCWSTR FileName,
							   PDOKAN_FILE_INFO DokanFileInfo) {
	WCHAR filePath[DOKAN_MAX_PATH];
	// HANDLE	handle = (HANDLE)DokanFileInfo->Context;
	HANDLE hFind;
	WIN32_FIND_DATAW findData;
	size_t fileLen;

	ZeroMemory(filePath, sizeof(filePath));
	GetFilePath(filePath, DOKAN_MAX_PATH, FileName);

	if (!DokanFileInfo->DeleteOnClose)
		// Dokan notify that the file is requested not to be deleted.
		return STATUS_SUCCESS;

	fileLen = wcslen(filePath);
	if (filePath[fileLen - 1] != L'\\') {
		filePath[fileLen++] = L'\\';
	}
	filePath[fileLen] = L'*';
	filePath[fileLen + 1] = L'\0';

	hFind = FindFirstFile(filePath, &findData);

	if (hFind == INVALID_HANDLE_VALUE) {
		DWORD error = GetLastError();
		return DokanNtStatusFromWin32(error);
	}

	do {
		if (wcscmp(findData.cFileName, L"..") != 0 &&
			wcscmp(findData.cFileName, L".") != 0) {
			FindClose(hFind);
			return STATUS_DIRECTORY_NOT_EMPTY;
		}
	} while (FindNextFile(hFind, &findData) != 0);

	DWORD error = GetLastError();

	FindClose(hFind);

	if (error != ERROR_NO_MORE_FILES) {
		return DokanNtStatusFromWin32(error);
	}

	return STATUS_SUCCESS;
}

NTSTATUS MirrorMoveFile(LPCWSTR FileName, // existing file name
						LPCWSTR NewFileName, BOOL ReplaceIfExisting,
						HANDLE handle) {
	WCHAR filePath[DOKAN_MAX_PATH];
	WCHAR newFilePath[DOKAN_MAX_PATH];
	HANDLE handle;
	DWORD bufferSize;
	BOOL result;
	size_t newFilePathLen;

	PFILE_RENAME_INFO renameInfo = NULL;

	GetFilePath(filePath, DOKAN_MAX_PATH, FileName);
	GetFilePath(newFilePath, DOKAN_MAX_PATH, NewFileName);

	if (!handle || handle == INVALID_HANDLE_VALUE) {
		return STATUS_INVALID_HANDLE;
	}

	newFilePathLen = wcslen(newFilePath);

	// the PFILE_RENAME_INFO struct has space for one WCHAR for the name at
	// the end, so that
	// accounts for the null terminator

	bufferSize = (DWORD)(sizeof(FILE_RENAME_INFO) +
						 newFilePathLen * sizeof(newFilePath[0]));

	renameInfo = (PFILE_RENAME_INFO)malloc(bufferSize);
	if (!renameInfo) {
		return STATUS_BUFFER_OVERFLOW;
	}
	ZeroMemory(renameInfo, bufferSize);

	renameInfo->ReplaceIfExists =
		ReplaceIfExisting
			? TRUE
			: FALSE; // some warning about converting BOOL to BOOLEAN
	renameInfo->RootDirectory = NULL; // hope it is never needed, shouldn't be
	renameInfo->FileNameLength =
		(DWORD)newFilePathLen *
		sizeof(newFilePath[0]); // they want length in bytes

	wcscpy_s(renameInfo->FileName, newFilePathLen + 1, newFilePath);

	result = SetFileInformationByHandle(handle, FileRenameInfo, renameInfo,
										bufferSize);

	free(renameInfo);

	if (result) {
		return STATUS_SUCCESS;
	} else {
		DWORD error = GetLastError();

		return DokanNtStatusFromWin32(error);
	}
}

NTSTATUS MirrorSetEndOfFile(LPCWSTR FileName, LONGLONG ByteOffset,
							HANDLE handle) {
	WCHAR filePath[DOKAN_MAX_PATH];
	LARGE_INTEGER offset;

	GetFilePath(filePath, DOKAN_MAX_PATH, FileName);

	if (!handle || handle == INVALID_HANDLE_VALUE) {
		return STATUS_INVALID_HANDLE;
	}

	offset.QuadPart = ByteOffset;
	if (!SetFilePointerEx(handle, offset, NULL, FILE_BEGIN)) {
		DWORD error = GetLastError();
		return DokanNtStatusFromWin32(error);
	}

	if (!SetEndOfFile(handle)) {
		DWORD error = GetLastError();

		return DokanNtStatusFromWin32(error);
	}

	return STATUS_SUCCESS;
}

NTSTATUS MirrorSetAllocationSize(LPCWSTR FileName, LONGLONG AllocSize,
								 HANDLE handle) {
	WCHAR filePath[DOKAN_MAX_PATH];
	LARGE_INTEGER fileSize;

	GetFilePath(filePath, DOKAN_MAX_PATH, FileName);

	if (!handle || handle == INVALID_HANDLE_VALUE) {

		return STATUS_INVALID_HANDLE;
	}

	if (GetFileSizeEx(handle, &fileSize)) {
		if (AllocSize < fileSize.QuadPart) {
			fileSize.QuadPart = AllocSize;
			if (!SetFilePointerEx(handle, fileSize, NULL, FILE_BEGIN)) {
				DWORD error = GetLastError();
				return DokanNtStatusFromWin32(error);
			}
			if (!SetEndOfFile(handle)) {
				DWORD error = GetLastError();

				return DokanNtStatusFromWin32(error);
			}
		}
	} else {
		DWORD error = GetLastError();

		return DokanNtStatusFromWin32(error);
	}
	return STATUS_SUCCESS;
}

NTSTATUS MirrorSetFileAttributes(LPCWSTR FileName, DWORD FileAttributes) {

	WCHAR filePath[DOKAN_MAX_PATH];

	GetFilePath(filePath, DOKAN_MAX_PATH, FileName);

	if (FileAttributes != 0) {
		if (!SetFileAttributes(filePath, FileAttributes)) {
			DWORD error = GetLastError();

			return DokanNtStatusFromWin32(error);
		}
	}

	return STATUS_SUCCESS;
}

NTSTATUS MirrorSetFileTime(LPCWSTR FileName, CONST FILETIME *CreationTime,
						   CONST FILETIME *LastAccessTime,
						   CONST FILETIME *LastWriteTime, HANDLE handle) {
	WCHAR filePath[DOKAN_MAX_PATH];

	GetFilePath(filePath, DOKAN_MAX_PATH, FileName);

	if (!handle || handle == INVALID_HANDLE_VALUE) {
		return STATUS_INVALID_HANDLE;
	}

	if (!SetFileTime(handle, CreationTime, LastAccessTime, LastWriteTime)) {
		DWORD error = GetLastError();
		return DokanNtStatusFromWin32(error);
	}

	return STATUS_SUCCESS;
}

NTSTATUS MirrorSetFileSecurity(LPCWSTR FileName,
							   PSECURITY_INFORMATION SecurityInformation,
							   PSECURITY_DESCRIPTOR SecurityDescriptor,
							   HANDLE handle) {
	WCHAR filePath[DOKAN_MAX_PATH];

	GetFilePath(filePath, DOKAN_MAX_PATH, FileName);

	if (!handle || handle == INVALID_HANDLE_VALUE) {
		return STATUS_INVALID_HANDLE;
	}

	if (!SetUserObjectSecurity(handle, SecurityInformation,
							   SecurityDescriptor)) {
		int error = GetLastError();

		return DokanNtStatusFromWin32(error);
	}
	return STATUS_SUCCESS;
}
