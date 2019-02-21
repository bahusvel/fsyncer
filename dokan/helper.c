#include "dokan/dokan.h"
#include "dokan/fileinfo.h"
#include <malloc.h>
#include <stdio.h>
#include <stdlib.h>
#include <winbase.h>

WORD CONST_DOKAN_VERSION = DOKAN_VERSION;

BOOL __stdcall AddSeSecurityNamePrivilege() {
	HANDLE token = 0;
	DbgPrint(L"## Attempting to add SE_SECURITY_NAME privilege to process "
			 L"token ##\n");
	DWORD err;
	LUID luid;
	if (!LookupPrivilegeValue(0, SE_SECURITY_NAME, &luid)) {
		err = GetLastError();
		if (err != ERROR_SUCCESS) {
			DbgPrint(
				L"  failed: Unable to lookup privilege value. error = %u\n",
				err);
			return FALSE;
		}
	}

	LUID_AND_ATTRIBUTES attr;
	attr.Attributes = SE_PRIVILEGE_ENABLED;
	attr.Luid = luid;

	TOKEN_PRIVILEGES priv;
	priv.PrivilegeCount = 1;
	priv.Privileges[0] = attr;

	if (!OpenProcessToken(GetCurrentProcess(),
						  TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &token)) {
		err = GetLastError();
		if (err != ERROR_SUCCESS) {
			DbgPrint(L"  failed: Unable obtain process token. error = %u\n",
					 err);
			return FALSE;
		}
	}

	TOKEN_PRIVILEGES oldPriv;
	DWORD retSize;
	AdjustTokenPrivileges(token, FALSE, &priv, sizeof(TOKEN_PRIVILEGES),
						  &oldPriv, &retSize);
	err = GetLastError();
	if (err != ERROR_SUCCESS) {
		DbgPrint(L"  failed: Unable to adjust token privileges: %u\n", err);
		CloseHandle(token);
		return FALSE;
	}

	BOOL privAlreadyPresent = FALSE;
	for (unsigned int i = 0; i < oldPriv.PrivilegeCount; i++) {
		if (oldPriv.Privileges[i].Luid.HighPart == luid.HighPart &&
			oldPriv.Privileges[i].Luid.LowPart == luid.LowPart) {
			privAlreadyPresent = TRUE;
			break;
		}
	}
	DbgPrint(privAlreadyPresent ? L"  success: privilege already present\n"
								: L"  success: privilege added\n");
	if (token)
		CloseHandle(token);
	return TRUE;
}