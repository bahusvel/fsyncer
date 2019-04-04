#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use error::Error;
use std::ffi::{OsStr, OsString};
use std::ptr;

metablock!(cfg(target_os = "windows") {
    use winapi::um::winnt::{PSID, SID};
    use std::io;
    use winapi::um::winbase::LocalFree;
    use winapi::um::accctrl::{TRUSTEE_W};
    use winapi::um::winnt::{PSECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SECURITY_DESCRIPTOR};
    use winapi::shared::winerror::ERROR_SUCCESS;
    use common::{os_to_wstr, WinapiBox};
    use winapi::shared::sddl::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW,
        ConvertStringSecurityDescriptorToSecurityDescriptorW,
        SDDL_REVISION_1,
    };
});

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Hash)]
pub enum FileSecurity {
    Windows {
        str_desc: Option<OsString>,
        info: Option<u32>,
        creator: Option<OsString>,
    },
    Unix {
        uid: u32,
        gid: u32,
    },
    Portable {
        owner: Option<String>,
        group: Option<String>,
    },
}

#[cfg(target_os = "windows")]
pub unsafe fn psid_to_string(sid: PSID) -> OsString {
    use common::wstr_to_os;
    use winapi::shared::sddl::ConvertSidToStringSidW;
    let mut p: *mut u16 = ptr::null_mut();
    if ConvertSidToStringSidW(sid, &mut p as *mut _) == 0 {
        panic!("Failed to get string SID");
    }
    let str_sid = wstr_to_os(p);
    LocalFree(p as *mut _);
    str_sid
}

#[cfg(target_os = "windows")]
pub unsafe fn string_to_sid(ssid: &OsStr) -> WinapiBox<SID> {
    use winapi::shared::sddl::ConvertStringSidToSidW;
    let mut p: PSID = ptr::null_mut();
    let buf = os_to_wstr(&ssid);
    if ConvertStringSidToSidW(buf.as_ptr(), &mut p as *mut _) == 0 {
        panic!("Failed to get string SID");
    }
    WinapiBox::from_raw(p as *mut _)
}

#[cfg(target_os = "windows")]
unsafe fn lookup_account(
    psid: PSID,
) -> Result<(String, String), Error<io::Error>> {
    // TODO keep cache of lookups
    use std::ffi::CStr;
    use winapi::um::winbase::LookupAccountSidA;
    use winapi::um::winnt::SID_NAME_USE;

    let name_buf: [u8; 256] = [0; 256];
    let mut name_len = name_buf.len() as u32;
    let domain_buf: [u8; 256] = [0; 256];
    let mut domain_len = domain_buf.len() as u32;
    let mut acc_type: SID_NAME_USE = 0;
    if LookupAccountSidA(
        ptr::null(),
        psid,
        name_buf.as_ptr() as *mut _,
        &mut name_len as *mut _,
        domain_buf.as_ptr() as *mut _,
        &mut domain_len as *mut _,
        &mut acc_type as *mut _,
    ) == 0
    {
        return Err(make_err!(io::Error::last_os_error()));
    }
    let domain = String::from(
        CStr::from_bytes_with_nul(&domain_buf[..domain_len as usize])
            .expect("Domain is an invalid CString")
            .to_str()
            .expect("Domain cannot be represented in UTF-8"),
    );
    let name = String::from(
        CStr::from_bytes_with_nul(&name_buf[..name_len as usize])
            .expect("Account name is an invalid CString")
            .to_str()
            .expect("Account name be represented in UTF-8"),
    );
    return Ok((domain, name));
}

#[cfg(target_os = "windows")]
impl FileSecurity {
    pub fn from_sddl(sddl: OsString) -> FileSecurity {
        FileSecurity::Windows {
            str_desc: Some(sddl),
            info: None,
            creator: None,
        }
    }
    pub unsafe fn parse_security(
        desc: PSECURITY_DESCRIPTOR,
        info: Option<PSECURITY_INFORMATION>,
        translate_sid: bool,
    ) -> Result<Self, Error<io::Error>> {
        use common::wstr_to_os;
        use winapi::um::winnt::{
            DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION,
            OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
            PROTECTED_SACL_SECURITY_INFORMATION, SACL_SECURITY_INFORMATION,
            UNPROTECTED_DACL_SECURITY_INFORMATION,
            UNPROTECTED_SACL_SECURITY_INFORMATION,
        };

        let mut s: *mut u16 = ptr::null_mut();
        if ConvertSecurityDescriptorToStringSecurityDescriptorW(
            desc,
            SDDL_REVISION_1 as u32,
            info.map(|i| *i).unwrap_or(
                DACL_SECURITY_INFORMATION
                    | PROTECTED_DACL_SECURITY_INFORMATION
                    | PROTECTED_SACL_SECURITY_INFORMATION
                    | OWNER_SECURITY_INFORMATION
                    | SACL_SECURITY_INFORMATION
                    | DACL_SECURITY_INFORMATION
                    | GROUP_SECURITY_INFORMATION
                    | UNPROTECTED_DACL_SECURITY_INFORMATION
                    | UNPROTECTED_SACL_SECURITY_INFORMATION,
            ),
            &mut s as *mut _,
            ptr::null_mut(),
        ) == 0
        {
            return Err(make_err!(io::Error::last_os_error()));
        }
        let str_desc = wstr_to_os(s);

        if translate_sid {
            panic!("Not implemented");
        }

        LocalFree(s as _);
        Ok(FileSecurity::Windows {
            creator: None,
            str_desc: Some(str_desc),
            info: info.map(|i| *i),
        })
    }

    pub unsafe fn creator_descriptor(
        &self,
    ) -> Result<WinapiBox<SECURITY_DESCRIPTOR>, Error<io::Error>> {
        use std::mem;
        use winapi::um::aclapi::{
            BuildSecurityDescriptorW, BuildTrusteeWithSidW,
        };
        use winapi::um::securitybaseapi::SetSecurityDescriptorControl;
        use winapi::um::winnt::SE_DACL_PROTECTED;
        let mut ownerT: TRUSTEE_W = mem::zeroed();
        let mut desc: PSECURITY_DESCRIPTOR = ptr::null_mut();
        let mut size: u32 = 0;

        match self {
            FileSecurity::Windows {
                creator: Some(creator),
                ..
            } => {
                let creator_sid = string_to_sid(creator);
                BuildTrusteeWithSidW(
                    &mut ownerT as *mut _,
                    creator_sid.as_ptr() as *mut _,
                );
                if BuildSecurityDescriptorW(
                    &mut ownerT as *mut _,
                    ptr::null_mut(),
                    0,
                    ptr::null_mut(),
                    0,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    &mut size as *mut _,
                    &mut desc as *mut _,
                ) != ERROR_SUCCESS
                {
                    return Err(make_err!(io::Error::last_os_error()));
                }

                if SetSecurityDescriptorControl(desc, SE_DACL_PROTECTED, 0) == 0
                {
                    return Err(make_err!(io::Error::last_os_error()));
                }

                let mut descriptor =
                    WinapiBox::from_raw(desc as *mut SECURITY_DESCRIPTOR);
                descriptor.add_borrow(creator_sid);
                Ok(descriptor)
            }
            FileSecurity::Windows { creator: None, .. } => {
                panic!("Creator is not set")
            }
            _ => panic!(
                "Cannot yet convert non windows filesecurity to descriptor"
            ),
        }
    }

    pub unsafe fn to_descriptor(
        &self,
    ) -> Result<Option<WinapiBox<SECURITY_DESCRIPTOR>>, Error<io::Error>> {
        let mut desc: PSECURITY_DESCRIPTOR = ptr::null_mut();
        match self {
            FileSecurity::Windows {
                str_desc: Some(str_desc),
                ..
            } => {
                let owned_s = os_to_wstr(&str_desc);
                if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    owned_s.as_ptr() as *mut _,
                    SDDL_REVISION_1 as u32,
                    &mut desc as *mut _,
                    ptr::null_mut(),
                ) == 0
                {
                    return Err(make_err!(io::Error::last_os_error()));
                }
                Ok(Some(WinapiBox::from_raw(desc as *mut _)))
            }
            FileSecurity::Windows { str_desc: None, .. } => Ok(None),
            _ => panic!(
                "Cannot yet convert non windows filesecurity to descriptor"
            ),
        }
    }
}
