#include "dokan/dokan.h"
#include <malloc.h>
#include <stdio.h>
#include <stdlib.h>
#include <winbase.h>

WORD CONST_DOKAN_VERSION = DOKAN_VERSION;

static BOOL add_priviledge(LPCWSTR name, HANDLE token) {
	LUID luid;
	if (!LookupPrivilegeValue(0, name, &luid)) {
		if (GetLastError() != ERROR_SUCCESS) {
			return FALSE;
		}
	}
	LUID_AND_ATTRIBUTES attr;
	attr.Attributes = SE_PRIVILEGE_ENABLED;
	attr.Luid = luid;
	TOKEN_PRIVILEGES priv;
	priv.PrivilegeCount = 1;
	priv.Privileges[0] = attr;

	TOKEN_PRIVILEGES oldPriv;
	DWORD retSize;
	AdjustTokenPrivileges(token, FALSE, &priv, sizeof(TOKEN_PRIVILEGES),
						  &oldPriv, &retSize);
	if (GetLastError() != ERROR_SUCCESS) {
		return FALSE;
	}
	return TRUE;
}

BOOL __stdcall AddPrivileges() {
	HANDLE token = 0;
	if (!OpenProcessToken(GetCurrentProcess(),
						  TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &token)) {
		if (GetLastError() != ERROR_SUCCESS) {
			return FALSE;
		}
	}

	if (!add_priviledge(SE_RESTORE_NAME, token)) {
		CloseHandle(token);
		return FALSE;
	}

	if (!add_priviledge(SE_SECURITY_NAME, token)) {
		CloseHandle(token);
		return FALSE;
	}

	CloseHandle(token);
	return TRUE;
}