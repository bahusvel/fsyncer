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
#pragma warning(push)
#pragma warning(disable : 4820)
#include "dokan/dokan.h"
#include "dokan/fileinfo.h"
#pragma warning(pop)
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

DWORD OpCreateDirectory(LPCWSTR FileName,
						PSECURITY_DESCRIPTOR SecurityDescriptor,
						ACCESS_MASK genericDesiredAccess,
						DWORD fileAttributesAndFlags, ULONG ShareAccess,
						DWORD creationDisposition, HANDLE *handle) {
	DWORD error = ERROR_SUCCESS;

	SECURITY_ATTRIBUTES securityAttrib;
	securityAttrib.nLength = sizeof(securityAttrib);
	securityAttrib.lpSecurityDescriptor = SecurityDescriptor;
	securityAttrib.bInheritHandle = FALSE;

	// It is a create directory request
	if (creationDisposition == CREATE_NEW ||
		creationDisposition == OPEN_ALWAYS) {
		// We create folder
		if (!CreateDirectory(FileName, &securityAttrib)) {
			error = GetLastError();
			// Fail to create folder for OPEN_ALWAYS is not an error
			if (error != ERROR_ALREADY_EXISTS ||
				creationDisposition == CREATE_NEW) {
				return error;
			}
		}
	}

	// FILE_FLAG_BACKUP_SEMANTICS is required for opening directory
	// handles
	*handle = CreateFile( // This just opens the directory
		FileName, genericDesiredAccess, ShareAccess, &securityAttrib,
		OPEN_EXISTING, fileAttributesAndFlags | FILE_FLAG_BACKUP_SEMANTICS,
		NULL);

	if (*handle == INVALID_HANDLE_VALUE) {
		return GetLastError();
	}

	return ERROR_SUCCESS;
}

DWORD OpCreateFile(LPCWSTR FileName, PSECURITY_DESCRIPTOR SecurityDescriptor,
				   ACCESS_MASK genericDesiredAccess,
				   DWORD fileAttributesAndFlags, ULONG ShareAccess,
				   DWORD creationDisposition, HANDLE *handle) {
	DWORD error = ERROR_SUCCESS;

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
		return ERROR_ACCESS_DENIED;

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
		return GetLastError();
	} else {
		// Need to update FileAttributes with previous when Overwrite file
		if (fileAttr != INVALID_FILE_ATTRIBUTES &&
			creationDisposition == TRUNCATE_EXISTING) {
			if (!SetFileAttributes(FileName,
								   fileAttributesAndFlags | fileAttr)) {
				return GetLastError();
			}
		}
	}
	return ERROR_SUCCESS;
}

DWORD OpMoveFile(LPCWSTR NewFileName, BOOL ReplaceIfExisting, HANDLE handle) {
	DWORD bufferSize;
	BOOL result;
	size_t newFilePathLen;

	PFILE_RENAME_INFO renameInfo = NULL;

	if (handle == INVALID_HANDLE_VALUE) {
		return ERROR_INVALID_HANDLE;
	}

	newFilePathLen = wcslen(NewFileName);

	// the PFILE_RENAME_INFO struct has space for one WCHAR for the name at
	// the end, so that
	// accounts for the null terminator

	bufferSize = (DWORD)(sizeof(FILE_RENAME_INFO) +
						 newFilePathLen * sizeof(NewFileName[0]));

	renameInfo = (PFILE_RENAME_INFO)malloc(bufferSize);
	if (!renameInfo) {
		return ERROR_BUFFER_OVERFLOW;
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
		return ERROR_SUCCESS;
	} else {
		return GetLastError();
	}
}