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

NTSTATUS MirrorCreateDirectory(LPCWSTR FileName,
							   PSECURITY_DESCRIPTOR SecurityDescriptor,
							   ACCESS_MASK genericDesiredAccess,
							   DWORD fileAttributesAndFlags, ULONG ShareAccess,
							   DWORD creationDisposition, HANDLE *handle) {
	NTSTATUS status = STATUS_SUCCESS;
	DWORD error = 0;

	SECURITY_ATTRIBUTES securityAttrib;
	securityAttrib.nLength = sizeof(securityAttrib);
	securityAttrib.lpSecurityDescriptor = SecurityDescriptor;
	securityAttrib.bInheritHandle = FALSE;

	DWORD fileAttr = GetFileAttributes(FileName);

	// It is a create directory request
	if (creationDisposition == CREATE_NEW ||
		creationDisposition == OPEN_ALWAYS) {
		// We create folder
		if (!CreateDirectory(FileName, &securityAttrib)) {
			error = GetLastError();
			// Fail to create folder for OPEN_ALWAYS is not an error
			if (error != ERROR_ALREADY_EXISTS ||
				creationDisposition == CREATE_NEW) {
				status = DokanNtStatusFromWin32(error);
			}
		}
	}
	if (status == STATUS_SUCCESS) {
		// FILE_FLAG_BACKUP_SEMANTICS is required for opening directory
		// handles
		*handle = CreateFile( // This just opens the directory
			FileName, genericDesiredAccess, ShareAccess, &securityAttrib,
			OPEN_EXISTING, fileAttributesAndFlags | FILE_FLAG_BACKUP_SEMANTICS,
			NULL);

		if (*handle == INVALID_HANDLE_VALUE) {
			error = GetLastError();
			status = DokanNtStatusFromWin32(error);
		} else {
			// Open succeed but we need to inform the driver
			// that the dir open and not created by returning
			// STATUS_OBJECT_NAME_COLLISION
			if (creationDisposition == OPEN_ALWAYS &&
				fileAttr != INVALID_FILE_ATTRIBUTES)
				return STATUS_OBJECT_NAME_COLLISION;
		}
	}
}

NTSTATUS MirrorCreateFile(LPCWSTR FileName,
						  PSECURITY_DESCRIPTOR SecurityDescriptor,
						  ACCESS_MASK genericDesiredAccess,
						  DWORD fileAttributesAndFlags, ULONG ShareAccess,
						  DWORD creationDisposition, HANDLE *handle) {

	NTSTATUS status = STATUS_SUCCESS;
	DWORD error = 0;

	SECURITY_ATTRIBUTES securityAttrib;
	securityAttrib.nLength = sizeof(securityAttrib);
	securityAttrib.lpSecurityDescriptor = SecurityDescriptor;
	securityAttrib.bInheritHandle = FALSE;

	/*
	if (ShareMode == 0 && AccessMode & FILE_WRITE_DATA)
			ShareMode = FILE_SHARE_WRITE;
	else if (ShareMode == 0)
			ShareMode = FILE_SHARE_READ;
	*/

	DWORD fileAttr = GetFileAttributes(FileName);

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

	*handle = CreateFile(
		FileName,
		genericDesiredAccess, // GENERIC_READ|GENERIC_WRITE|GENERIC_EXECUTE,
		ShareAccess,
		&securityAttrib, // security attribute
		creationDisposition,
		fileAttributesAndFlags, // |FILE_FLAG_NO_BUFFERING,
		NULL);					// template file handle

	if (*handle == INVALID_HANDLE_VALUE) {
		error = GetLastError();
		status = DokanNtStatusFromWin32(error);
	} else {
		// Need to update FileAttributes with previous when Overwrite file
		if (fileAttr != INVALID_FILE_ATTRIBUTES &&
			creationDisposition == TRUNCATE_EXISTING) {
			SetFileAttributes(FileName, fileAttributesAndFlags | fileAttr);
		}
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
	return status;
}

#pragma warning(push)
#pragma warning(disable : 4305)

static void MirrorCleanup(LPCWSTR FileName, PDOKAN_FILE_INFO DokanFileInfo) {
	if (DokanFileInfo->Context) {
		CloseHandle((HANDLE)(DokanFileInfo->Context));
		DokanFileInfo->Context = 0;
	}
	if (DokanFileInfo->DeleteOnClose) {
		// Should already be deleted by CloseHandle
		// if open with FILE_FLAG_DELETE_ON_CLOSE
		if (DokanFileInfo->IsDirectory) {
			if (!RemoveDirectory(FileName)) {
				// Failed to remove directory
			}
		} else {
			if (DeleteFile(FileName) == 0) {
				// Failed to remove file
			}
		}
	}
}

NTSTATUS MirrorWriteFile(LPCWSTR FileName, LPCVOID Buffer,
						 DWORD NumberOfBytesToWrite,
						 LPDWORD NumberOfBytesWritten, LONGLONG Offset,
						 PDOKAN_FILE_INFO DokanFileInfo) {
	HANDLE handle = (HANDLE)DokanFileInfo->Context;
	BOOL opened = FALSE;

	// reopen the file
	if (!handle || handle == INVALID_HANDLE_VALUE) {
		handle = CreateFile(FileName, GENERIC_WRITE, FILE_SHARE_WRITE, NULL,
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
	HANDLE handle = (HANDLE)DokanFileInfo->Context;

	DWORD dwAttrib = GetFileAttributes(FileName);

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

NTSTATUS MirrorDeleteDirectory(LPWSTR FileName,
							   PDOKAN_FILE_INFO DokanFileInfo) {
	// HANDLE	handle = (HANDLE)DokanFileInfo->Context;
	HANDLE hFind;
	WIN32_FIND_DATAW findData;
	size_t fileLen;

	if (!DokanFileInfo->DeleteOnClose)
		// Dokan notify that the file is requested not to be deleted.
		return STATUS_SUCCESS;

	fileLen = wcslen(FileName);
	if (FileName[fileLen - 1] != L'\\') {
		FileName[fileLen++] = L'\\';
	}
	FileName[fileLen] = L'*';
	FileName[fileLen + 1] = L'\0';

	hFind = FindFirstFile(FileName, &findData);

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
	DWORD bufferSize;
	BOOL result;
	size_t newFilePathLen;

	PFILE_RENAME_INFO renameInfo = NULL;

	if (!handle || handle == INVALID_HANDLE_VALUE) {
		return STATUS_INVALID_HANDLE;
	}

	newFilePathLen = wcslen(NewFileName);

	// the PFILE_RENAME_INFO struct has space for one WCHAR for the name at
	// the end, so that
	// accounts for the null terminator

	bufferSize = (DWORD)(sizeof(FILE_RENAME_INFO) +
						 newFilePathLen * sizeof(NewFileName[0]));

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
		sizeof(NewFileName[0]); // they want length in bytes

	wcscpy_s(renameInfo->FileName, newFilePathLen + 1, NewFileName);

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
	LARGE_INTEGER offset;

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
	LARGE_INTEGER fileSize;

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
	if (FileAttributes != 0) {
		if (!SetFileAttributes(FileName, FileAttributes)) {
			DWORD error = GetLastError();
			return DokanNtStatusFromWin32(error);
		}
	}
	return STATUS_SUCCESS;
}

NTSTATUS MirrorSetFileTime(LPCWSTR FileName, CONST FILETIME *CreationTime,
						   CONST FILETIME *LastAccessTime,
						   CONST FILETIME *LastWriteTime, HANDLE handle) {

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
