/*
' is chosed as delimeter in SDDL to store names instead of SIDs,
it can technically be contained within SDDL itself but that is unlikely.
If it is ever encountered this code will panic,
and delimeter can be changed to something more unique like <{[name]}>
*/
macro_rules! SDDL_DELIM {
    () => {
        "'"
    };
}
use std::ffi::OsString;
use std::ptr;
use winapi::um::winnt::{PSID, SID};
use std::ffi::OsStr;
use winapi::um::winbase::LocalFree;
use winapi::um::winnt::{PSECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SECURITY_DESCRIPTOR};
use common::{os_to_wstr, WinapiBox};
use winapi::shared::sddl::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW,
    ConvertStringSecurityDescriptorToSecurityDescriptorW,
    SDDL_REVISION_1,
};
use regex::{Regex, Captures};

lazy_static! {
    static ref SID_REGEX: Regex = Regex::new("S-1-5(-[0-9]+)+").unwrap();
    static ref NAME_REGEX: Regex = Regex::new(concat!(SDDL_DELIM!(), "(.+?)", SDDL_DELIM!())).unwrap();
}

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

pub unsafe fn string_to_sid(ssid: &OsStr) -> WinapiBox<SID> {
    use winapi::shared::sddl::ConvertStringSidToSidW;
    let mut p: PSID = ptr::null_mut();
    let buf = os_to_wstr(&ssid);
    if ConvertStringSidToSidW(buf.as_ptr(), &mut p as *mut _) == 0 {
        panic!("Failed to get string SID");
    }
    WinapiBox::from_raw(p as *mut _)
}

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
        return Err(trace_err!(io::Error::last_os_error()));
    }
    debug!(domain_len, name_len);
    let domain = String::from(
        CStr::from_bytes_with_nul(&domain_buf[..domain_len as usize+1])
            .expect("Domain is an invalid CString")
            .to_str()
            .expect("Domain cannot be represented in UTF-8"),
    );
    let name = String::from(
        CStr::from_bytes_with_nul(&name_buf[..name_len as usize+1])
            .expect("Account name is an invalid CString")
            .to_str()
            .expect("Account name be represented in UTF-8"),
    );
    return Ok((domain, name));
}

unsafe fn lookup_sid(name: &str) -> Result<String, Error<io::Error>> {
    use std::mem;
    use winapi::um::winbase::LookupAccountNameW;
    use winapi::um::winnt::SID_NAME_USE;
    let wname = os_to_wstr(OsStr::new(name));
    let mut sid_buf: [u8; 256] = [0; 256];
    let mut sid_len = sid_buf.len() as u32;
    let mut domain_buf: [u16; 256] = [0; 256];
    let mut domain_len = domain_buf.len() as u32;
    let mut name_use: SID_NAME_USE = mem::zeroed();
    if LookupAccountNameW(
        ptr::null_mut(),
        wname.as_ptr() as *mut _,
        sid_buf.as_mut_ptr() as *mut _,
        &mut sid_len as *mut _,
        domain_buf.as_mut_ptr(),
        &mut domain_len as *mut _,
        &mut name_use as *mut _,
    ) == 0
    {
        return Err(trace_err!(io::Error::last_os_error()));
    }

    Ok(psid_to_string(sid_buf.as_ptr() as *mut _)
        .into_string()
        .expect("SID contains weird charachters"))
}

impl FileSecurity {
    // pub fn from_sddl(sddl: OsString) -> FileSecurity {
    //     FileSecurity::Windows {
    //         str_desc: sddl,
    //         info: None,
    //     }
    // }
    pub fn mut_sddl(&mut self) -> &mut String {
        if let FileSecurity::Windows { str_desc, .. } = self {
            return str_desc;
        } else {
            panic!("Cannot retrieve sddl of non windows security");
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

        if desc.is_null() {
            return Ok(FileSecurity::Windows {
                str_desc: String::new(),
                info: info.map(|i| *i),
            });
        }

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
            return Err(trace_err!(io::Error::last_os_error()));
        }
        let mut str_desc = wstr_to_os(s)
            .into_string()
            .expect("SDDL contains weird charachters");

        if translate_sid {
            assert!(!str_desc.contains(SDDL_DELIM!()));
            str_desc = SID_REGEX
                .replace_all(&str_desc, |caps: &Captures| {
                    let psid = string_to_sid(OsStr::new(&caps[0]));
                    match lookup_account(psid.as_ptr() as *mut _) {
                        Ok((_domain, name)) => format!("'{}'", name),
                        Err(e) => {
                            eprintln!("Account lookup failed {} {:?}", &caps[0], e);
                            String::from(&caps[0])
                        }
                    }
                })
                .into_owned();
        }

        LocalFree(s as _);
        Ok(FileSecurity::Windows {
            str_desc: str_desc,
            info: info.map(|i| *i),
        })
    }

    pub unsafe fn to_descriptor(
        &self,
    ) -> Result<WinapiBox<SECURITY_DESCRIPTOR>, Error<io::Error>> {
        let mut desc: PSECURITY_DESCRIPTOR = ptr::null_mut();
        match self {
            FileSecurity::Windows { str_desc, .. } => {
                let sddl = NAME_REGEX.replace_all(&str_desc, |caps: &Captures| {
                                match lookup_sid(&caps[1]) {
                                    Ok(sid) => sid,
                                    Err(e) => {
                                        eprintln!(
                                            "Account lookup failed {} {:?}",
                                            &caps[1], e
                                        );
                                        String::from("S-1-0-0") // Nobody SID
                                    }
                                }
                            });
                let owned_s = os_to_wstr(OsStr::new(&*sddl));
                if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    owned_s.as_ptr() as *mut _,
                    SDDL_REVISION_1 as u32,
                    &mut desc as *mut _,
                    ptr::null_mut(),
                ) == 0
                {
                    return Err(trace_err!(io::Error::last_os_error()));
                }
                Ok(WinapiBox::from_raw(desc as *mut _))
            }
            _ => panic!(
                "Cannot yet convert non windows filesecurity to descriptor"
            ),
        }
    }
}

pub fn copy_security(src: &Path, dst: &Path) -> Result<(), Error<io::Error>> {
    use common::with_file;
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;
    use winapi::um::winbase::FILE_FLAG_BACKUP_SEMANTICS;
    use winapi::um::winnt::{
        ACCESS_SYSTEM_SECURITY, DACL_SECURITY_INFORMATION, GENERIC_READ,
        GENERIC_WRITE, GROUP_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
        READ_CONTROL, SACL_SECURITY_INFORMATION, WRITE_DAC,
    };
    use winapi::um::winuser::{GetUserObjectSecurity, SetUserObjectSecurity};
    const DESC_LENGTH: usize = 4096;
    let info = OWNER_SECURITY_INFORMATION
        | GROUP_SECURITY_INFORMATION
        | DACL_SECURITY_INFORMATION
        | SACL_SECURITY_INFORMATION;
    let mut desc: [u8; DESC_LENGTH] = [0; DESC_LENGTH];
    let mut needed: u32 = 0;
    trace!(trace!(with_file(
        src,
        OpenOptions::new()
            .access_mode(ACCESS_SYSTEM_SECURITY | GENERIC_READ | READ_CONTROL)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS),
        |handle| {
            if unsafe {
                GetUserObjectSecurity(
                    handle,
                    &info as *const _ as *mut _,
                    &mut desc as *mut _ as *mut _,
                    DESC_LENGTH as u32,
                    &mut needed as *mut _,
                )
            } == 0
            {
                Err(trace_err!(io::Error::last_os_error()))
            } else {
                Ok(())
            }
        },
    )));
    if needed as usize > DESC_LENGTH {
        panic!("Failed to copy really large descriptor, Denis is lazy.");
    }
    trace!(trace!(with_file(
        dst,
        OpenOptions::new()
            .access_mode(ACCESS_SYSTEM_SECURITY | GENERIC_WRITE | WRITE_DAC)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS),
        |handle| {
            if unsafe {
                SetUserObjectSecurity(
                    handle,
                    &info as *const _ as *mut _,
                    &desc as *const _ as *mut _,
                )
            } == 0
            {
                Err(trace_err!(io::Error::last_os_error()))
            } else {
                Ok(())
            }
        },
    )));

    Ok(())
}