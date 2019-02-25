#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ffi::OsString;

metablock!(cfg(target_os = "windows") {
    use winapi::um::winnt::{PACL, PSID};
    use std::io::{Error, ErrorKind};
    use winapi::um::winbase::LocalFree;
    use winapi::shared::guiddef::GUID;
    use winapi::um::accctrl::{TRUSTEE_TYPE, SE_OBJECT_TYPE, TRUSTEE_W};
});

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
impl From<TRUSTEE_W> for Trustee {
    fn from(t: TRUSTEE_W) -> Self {
        use common::wstr_to_os;
        use winapi::shared::sddl::ConvertSidToStringSidW;
        use winapi::um::accctrl::*;
        use winapi::um::winnt::{ACE_INHERITED_OBJECT_TYPE_PRESENT, ACE_OBJECT_TYPE_PRESENT};

        unsafe fn psid_to_string(sid: PSID) -> OsString {
            let mut p: *mut u16;
            if ConvertSidToStringSidW(sid, &mut p as *mut _) == 0 {
                panic!("Failed to get strign SID");
            }
            let os_sid = wstr_to_os(p);
            LocalFree(*p as *mut _);
            os_sid
        }

        let ty = TrusteeType::from(t.TrusteeType);

        let form = match t.TrusteeForm {
            TRUSTEE_IS_SID => TrusteeForm::Sid(psid_to_string(t.ptstrName as PSID)),
            TRUSTEE_IS_NAME => TrusteeForm::Name(wstr_to_os(t.ptstrName)),
            TRUSTEE_IS_OBJECTS_AND_SID => {
                use winapi::um::accctrl::OBJECTS_AND_SID;
                let t = t.ptstrName as *const OBJECTS_AND_SID;

                TrusteeForm::ObjectsAndSid {
                    object_type: if flagset!((*t).ObjectsPresent, ACE_OBJECT_TYPE_PRESENT) {
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

                    object_type_name: if flagset!((*t).ObjectsPresent, ACE_OBJECT_TYPE_PRESENT) {
                        Some(wstr_to_os((*t).ObjectTypeName))
                    } else {
                        None
                    },
                    object_type: ty,
                    name: wstr_to_os((*t).ptstrName),
                }
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
pub unsafe fn acl_entries(descriptor: PACL) -> Result<Vec<ACE>, Error> {
    use winapi::shared::winerror::ERROR_SUCCESS;
    use winapi::um::accctrl::EXPLICIT_ACCESS_W;
    use winapi::um::aclapi::GetExplicitEntriesFromAclW;

    let mut count: u32 = 0;
    let mut entries: *mut EXPLICIT_ACCESS_W;

    if GetExplicitEntriesFromAclW(descriptor, &mut count as *mut _, &mut entries as *mut _)
        != ERROR_SUCCESS
    {
        return Err(Error::new(ErrorKind::Other, "Failed to get acl entries"));
    }

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
