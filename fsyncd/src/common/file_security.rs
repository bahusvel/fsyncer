#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ffi::OsString;
use std::ptr;

metablock!(cfg(target_os = "windows") {
    use winapi::um::winnt::{PACL, PSID, SID};
    use std::io::{Error, ErrorKind};
    use winapi::um::winbase::LocalFree;
    use winapi::shared::guiddef::GUID;
    use winapi::um::accctrl::{TRUSTEE_TYPE, SE_OBJECT_TYPE, TRUSTEE_W};
    use winapi::um::winnt::{PSECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SECURITY_DESCRIPTOR};
});

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Hash)]
pub enum FileSecurity {
    Windows {
        group: Option<String>,
        owner: Option<String>,
        dacl: Option<Vec<ACE>>,
        sacl: Option<Vec<ACE>>,
    },
    Unix {
        uid: u32,
        gid: u32,
    },
    Portable {
        owner: Option<String>,
        group: Option<String>,
    },
    Default,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Hash)]
pub enum TrusteeForm {
    Name(OsString),
    Sid(OsString),
    ObjectsAndSid {
        object_type: Option<WinGUID>,
        inherited_object_type: Option<WinGUID>,
        sid: OsString,
    },
    ObjectsAndName {
        object_type: ObjectType,
        object_type_name: Option<OsString>,
        inherited_object_type_name: Option<OsString>,
        name: OsString,
    },
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Hash)]
pub enum TrusteeType {
    TRUSTEE_IS_UNKNOWN,
    TRUSTEE_IS_USER,
    TRUSTEE_IS_GROUP,
    TRUSTEE_IS_DOMAIN,
    TRUSTEE_IS_ALIAS,
    TRUSTEE_IS_WELL_KNOWN_GROUP,
    TRUSTEE_IS_DELETED,
    TRUSTEE_IS_INVALID,
    TRUSTEE_IS_COMPUTER,
}

macro_rules! enummatch {
    ($val:expr, $en:ident, $($flags:ident),+) => {
        match $val {
            $(
                $flags => $en::$flags,
            )*
            _ => panic!("Failed to enum match"),
        }
    };
}

#[cfg(target_os = "windows")]
impl From<TRUSTEE_TYPE> for TrusteeType {
    fn from(t: TRUSTEE_TYPE) -> Self {
        use winapi::um::accctrl::*;
        enummatch! {t, TrusteeType,
            TRUSTEE_IS_UNKNOWN,
            TRUSTEE_IS_USER,
            TRUSTEE_IS_GROUP,
            TRUSTEE_IS_DOMAIN,
            TRUSTEE_IS_ALIAS,
            TRUSTEE_IS_WELL_KNOWN_GROUP,
            TRUSTEE_IS_DELETED,
            TRUSTEE_IS_INVALID,
            TRUSTEE_IS_COMPUTER
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Hash)]
#[repr(C)]
pub struct WinGUID {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

#[cfg(target_os = "windows")]
impl From<GUID> for WinGUID {
    fn from(g: GUID) -> Self {
        WinGUID {
            data1: g.Data1,
            data2: g.Data2,
            data3: g.Data3,
            data4: g.Data4,
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Hash)]
pub enum ObjectType {
    SE_UNKNOWN_OBJECT_TYPE,
    SE_FILE_OBJECT,
    SE_SERVICE,
    SE_PRINTER,
    SE_REGISTRY_KEY,
    SE_LMSHARE,
    SE_KERNEL_OBJECT,
    SE_WINDOW_OBJECT,
    SE_DS_OBJECT,
    SE_DS_OBJECT_ALL,
    SE_PROVIDER_DEFINED_OBJECT,
    SE_WMIGUID_OBJECT,
    SE_REGISTRY_WOW64_32KEY,
    SE_REGISTRY_WOW64_64KEY,
}

#[cfg(target_os = "windows")]
impl From<SE_OBJECT_TYPE> for ObjectType {
    fn from(o: SE_OBJECT_TYPE) -> Self {
        use winapi::um::accctrl::*;
        enummatch! {o, ObjectType,  SE_UNKNOWN_OBJECT_TYPE,
                    SE_FILE_OBJECT,
                    SE_SERVICE,
                    SE_PRINTER,
                    SE_REGISTRY_KEY,
                    SE_LMSHARE,
                    SE_KERNEL_OBJECT,
                    SE_WINDOW_OBJECT,
                    SE_DS_OBJECT,
                    SE_DS_OBJECT_ALL,
                    SE_PROVIDER_DEFINED_OBJECT,
                    SE_WMIGUID_OBJECT,
                    SE_REGISTRY_WOW64_32KEY,
                    SE_REGISTRY_WOW64_64KEY
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Hash)]
pub struct Trustee {
    ty: TrusteeType,
    form: TrusteeForm,
}

#[cfg(target_os = "windows")]
unsafe fn psid_to_string(sid: PSID) -> OsString {
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
unsafe fn string_to_sid(ssid: OsString) -> PSID {
    use winapi::shared::sddl::ConvertStringSidToSidW;
    use std::os::windows::ffi::OsStrExt;
    let mut p: PSID = ptr::null_mut();
    let mut buf: Vec<u16> = ssid.as_os_str().encode_wide().collect();
    buf.push(0);

    if ConvertStringSidToSidW(buf.as_ptr(), &mut p as *mut _) == 0 {
        panic!("Failed to get string SID");
    }
    p
}

#[cfg(target_os = "windows")]
impl From<TRUSTEE_W> for Trustee {
    fn from(t: TRUSTEE_W) -> Self {
        use common::wstr_to_os;
        use winapi::um::accctrl::*;
        use winapi::um::winnt::{
            ACE_INHERITED_OBJECT_TYPE_PRESENT, ACE_OBJECT_TYPE_PRESENT,
        };

        let ty = TrusteeType::from(t.TrusteeType);
        let form = unsafe {
            match t.TrusteeForm {
                TRUSTEE_IS_SID => {
                    TrusteeForm::Sid(psid_to_string(t.ptstrName as PSID))
                }
                TRUSTEE_IS_NAME => TrusteeForm::Name(wstr_to_os(t.ptstrName)),
                TRUSTEE_IS_OBJECTS_AND_SID => {
                    use winapi::um::accctrl::OBJECTS_AND_SID;
                    let t = t.ptstrName as *const OBJECTS_AND_SID;
                    TrusteeForm::ObjectsAndSid {
                        object_type: if flagset!(
                            (*t).ObjectsPresent,
                            ACE_OBJECT_TYPE_PRESENT
                        ) {
                            Some(WinGUID::from((*t).ObjectTypeGuid))
                        } else {
                            None
                        },
                        inherited_object_type: if flagset!(
                            (*t).ObjectsPresent,
                            ACE_INHERITED_OBJECT_TYPE_PRESENT
                        ) {
                            Some(WinGUID::from((*t).InheritedObjectTypeGuid))
                        } else {
                            None
                        },
                        sid: psid_to_string((*t).pSid as *mut _),
                    }
                }
                TRUSTEE_IS_OBJECTS_AND_NAME => {
                    use winapi::um::accctrl::OBJECTS_AND_NAME_W;
                    let t = t.ptstrName as *const OBJECTS_AND_NAME_W;
                    let ty = ObjectType::from((*t).ObjectType);
                    TrusteeForm::ObjectsAndName {
                        inherited_object_type_name: if flagset!(
                            (*t).ObjectsPresent,
                            ACE_INHERITED_OBJECT_TYPE_PRESENT
                        ) {
                            Some(wstr_to_os((*t).InheritedObjectTypeName))
                        } else {
                            None
                        },

                        object_type_name: if flagset!(
                            (*t).ObjectsPresent,
                            ACE_OBJECT_TYPE_PRESENT
                        ) {
                            Some(wstr_to_os((*t).ObjectTypeName))
                        } else {
                            None
                        },
                        object_type: ty,
                        name: wstr_to_os((*t).ptstrName),
                    }
                }
                F => panic!("Invalid trustee form {}", F),
            }
        };

        Trustee { ty, form }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Hash)]
pub struct ACE {
    permisssions: u32,
    mode: u32,
    inheritance: u32,
    trustee: Trustee,
}

#[cfg(target_os = "windows")]
unsafe fn acl_entries(acl: PACL) -> Result<Vec<ACE>, Error> {
    use winapi::shared::winerror::ERROR_SUCCESS;
    use winapi::um::accctrl::EXPLICIT_ACCESS_W;
    use winapi::um::aclapi::GetExplicitEntriesFromAclW;

    let mut count: u32 = 0;
    let mut entries: *mut EXPLICIT_ACCESS_W = ptr::null_mut();

    assert!(!acl.is_null());

    if GetExplicitEntriesFromAclW(
        acl,
        &mut count as *mut _,
        &mut entries as *mut _,
    ) != ERROR_SUCCESS
    {
        return Err(Error::new(ErrorKind::Other, "Failed to get acl entries"));
    }

    assert!(!entries.is_null());

    let mut rlist = Vec::new();

    for i in 0..count {
        let entry = entries.offset(i as isize);
        rlist.push(ACE {
            permisssions: (*entry).grfAccessPermissions,
            mode: (*entry).grfAccessMode,
            inheritance: (*entry).grfInheritance,
            trustee: Trustee::from((*entry).Trustee),
        });
    }

    LocalFree(entries as *mut _);
    Ok(rlist)
}

#[cfg(target_os = "windows")]
unsafe fn lookup_account(psid: PSID) -> Result<(String, String), Error> {
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
        return Err(Error::last_os_error());
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
unsafe fn acl_from_descriptor(
    descriptor: PSECURITY_DESCRIPTOR,
    dacl: bool,
) -> Result<(PACL, bool), Error> {
    use winapi::um::securitybaseapi::{
        GetSecurityDescriptorDacl, GetSecurityDescriptorSacl,
    };
    let mut ppacl = ptr::null_mut();
    let mut present = 0;
    let mut defaulted = 0;

    if dacl {
        if GetSecurityDescriptorDacl(
            descriptor,
            &mut present as *mut _,
            &mut ppacl as *mut _,
            &mut defaulted as *mut _,
        ) == 0
        {
            return Err(Error::last_os_error());
        }
    } else {
        if GetSecurityDescriptorSacl(
            descriptor,
            &mut present as *mut _,
            &mut ppacl as *mut _,
            &mut defaulted as *mut _,
        ) == 0
        {
            return Err(Error::last_os_error());
        }
    }
    if present == 0 {
        debug!(dacl);
        ppacl = ptr::null_mut();
    }

    Ok((ppacl, defaulted != 0))
}

#[cfg(target_os = "windows")]
unsafe fn psid_from_descriptor(
    descriptor: PSECURITY_DESCRIPTOR,
    owner: bool,
) -> Result<(PSID, bool), Error> {
    use winapi::um::securitybaseapi::{
        GetSecurityDescriptorGroup, GetSecurityDescriptorOwner,
    };
    let mut psid = ptr::null_mut();
    let mut defaulted = 0;

    if owner {
        if GetSecurityDescriptorOwner(
            descriptor,
            &mut psid as *mut _,
            &mut defaulted as *mut _,
        ) == 0
        {
            return Err(Error::last_os_error());
        }
    } else {
        if GetSecurityDescriptorGroup(
            descriptor,
            &mut psid as *mut _,
            &mut defaulted as *mut _,
        ) == 0
        {
            return Err(Error::last_os_error());
        }
    }

    Ok((psid, defaulted != 0))
}

#[cfg(target_os = "windows")]
impl FileSecurity {
    pub unsafe fn parse_security(
        desc: PSECURITY_DESCRIPTOR,
        info: Option<PSECURITY_INFORMATION>,
        translate_sid: bool,
    ) -> Result<Self, Error> {
        use winapi::um::winnt::{
            DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION,
            OWNER_SECURITY_INFORMATION, SACL_SECURITY_INFORMATION,
        };
        Ok(FileSecurity::Windows {
            // This is invalid, I tthink I need to use GetSecurityDescriptorXXXX
            owner: if info.is_some()
                && !flagset!(*info.unwrap(), OWNER_SECURITY_INFORMATION)
            {
                None
            } else {
                let (psid, _) = psid_from_descriptor(desc, true)?;
                if translate_sid {
                    let (_, acc_name) = lookup_account(psid)?;
                    Some(acc_name)
                } else {
                    Some(
                        psid_to_string(psid)
                            .into_string()
                            .expect("Unexpected charachter in SID"),
                    )
                }
            },
            group: if info.is_some()
                && !flagset!(*info.unwrap(), GROUP_SECURITY_INFORMATION)
            {
                None
            } else {
                let (psid, _) = psid_from_descriptor(desc, false)?;
                if translate_sid {
                    let (_, acc_name) = lookup_account(psid)?;
                    Some(acc_name)
                } else {
                    Some(
                        psid_to_string(psid)
                            .into_string()
                            .expect("Unexpected charachter in SID"),
                    )
                }
            },
            dacl: if info.is_some()
                && !flagset!(*info.unwrap(), DACL_SECURITY_INFORMATION)
            {
                None
            } else {
                let (acl, _) = acl_from_descriptor(desc, true)?;
                Some(acl_entries(acl)?)
            },
            sacl: if info.is_some()
                && !flagset!(*info.unwrap(), SACL_SECURITY_INFORMATION)
            {
                None
            } else {
                let (acl, _) = acl_from_descriptor(desc, false)?;
                Some(acl_entries(acl)?)
            },
        })
    }

    pub unsafe fn to_descriptor(&self) -> Result<SECURITY_DESCRIPTOR, Error> {
        use std::mem;
        use winapi::um::securitybaseapi::{
            InitializeSecurityDescriptor, SetSecurityDescriptorDacl,
            SetSecurityDescriptorGroup, SetSecurityDescriptorOwner,
            SetSecurityDescriptorSacl,
        };
        const FALSE: i32 = 0;
        const TRUE: i32 = 1;
        use winapi::um::winnt::SECURITY_DESCRIPTOR_REVISION;
        let mut desc: SECURITY_DESCRIPTOR = mem::zeroed();
        if InitializeSecurityDescriptor(
            &mut desc as *mut _ as *mut _,
            SECURITY_DESCRIPTOR_REVISION,
        ) == 0
        {
            return Err(Error::last_os_error());
        }
        if let FileSecurity::Windows {
            owner,
            group,
            sacl,
            dacl,
        } = self
        {
            if owner.is_some() {
                let psid = string_to_sid(OsString::from(owner.unwrap()));
                if SetSecurityDescriptorOwner(&mut desc as *mut _ as *mut _, psid, FALSE) == 0 {
                    return Err(Error::last_os_error());
                }
            }
            if group.is_some() {
                let psid = string_to_sid(OsString::from(group.unwrap()));
                if SetSecurityDescriptorGroup(&mut desc as *mut _ as *mut _, psid, FALSE) == 0 {
                    return Err(Error::last_os_error());
                }
            }
            if sacl.is_some() {
                if SetSecurityDescriptorSacl(&mut desc as *mut _ as *mut _, TRUE,, FALSE) == 0 {
                    return Err(Error::last_os_error());
                }
            }
            if dacl.is_some() {
                if SetSecurityDescriptorDacl(&mut desc as *mut _ as *mut _, TRUE, , FALSE) == 0 {
                    return Err(Error::last_os_error());
                }
            }
        } else {
            panic!("Cannot yet convert non windows filesecurity to descriptor")
        }

        Ok(desc)
    }
}
