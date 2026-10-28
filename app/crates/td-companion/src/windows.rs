use std::{
    io::{self, Write},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle},
    },
    path::Path,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
    System::Threading::*,
};
fn wide(s: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(Some(0)).collect()
}
fn checked(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
struct Local(*mut core::ffi::c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
pub struct Instance(HANDLE);
impl Instance {
    pub fn acquire(_: &Path) -> io::Result<Self> {
        unsafe {
            let handle = CreateMutexW(null(), 0, wide(r"Local\touchdeck-companion").as_ptr());
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(handle);
                return Err(io::Error::other("The Companion is already running."));
            }
            Ok(Self(handle))
        }
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn user_sid() -> io::Result<String> {
    unsafe {
        let mut token = null_mut();
        checked(OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_QUERY,
            &mut token,
        ))?;
        let mut length = 0;
        GetTokenInformation(token, TokenUser, null_mut(), 0, &mut length);
        let mut buffer = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
        let result = GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            length,
            &mut length,
        );
        CloseHandle(token);
        checked(result)?;
        let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
        let mut string = null_mut();
        checked(ConvertSidToStringSidW(user.User.Sid, &mut string))?;
        let _allocated = Local(string.cast());
        let mut n = 0;
        while *string.add(n) != 0 {
            n += 1;
        }
        Ok(String::from_utf16_lossy(std::slice::from_raw_parts(
            string, n,
        )))
    }
}
fn descriptor(sddl: &str) -> io::Result<Local> {
    unsafe {
        let mut sd = null_mut();
        checked(ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide(sddl).as_ptr(),
            1,
            &mut sd,
            null_mut(),
        ))?;
        Ok(Local(sd))
    }
}
pub fn write_runtime(path: &Path, value: &serde_json::Value) -> io::Result<()> {
    unsafe {
        let dir = path.parent().unwrap();
        std::fs::create_dir_all(dir)?;
        let attributes = GetFileAttributesW(wide(dir).as_ptr());
        if attributes == INVALID_FILE_ATTRIBUTES || attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::other(
                "The runtime directory is a reparse point or unavailable.",
            ));
        }
        let sid = user_sid()?;
        let directory = descriptor(&format!("O:{sid}D:P(A;OICI;FA;;;{sid})"))?;
        let mut dacl = null_mut();
        let mut present = 0;
        let mut defaulted = 0;
        let mut owner = null_mut();
        checked(GetSecurityDescriptorDacl(
            directory.0,
            &mut present,
            &mut dacl,
            &mut defaulted,
        ))?;
        checked(GetSecurityDescriptorOwner(
            directory.0,
            &mut owner,
            &mut defaulted,
        ))?;
        // Changing the owner needs WRITE_OWNER, which an owner doesn't hold implicitly.
        // The Companion usually created `runtime\` and owns it already, so only a foreign
        // owner is replaced, and that fails closed when the user can't take it.
        let mut information = DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION;
        if !owned_by(dir, owner)? {
            information |= OWNER_SECURITY_INFORMATION;
        }
        let status = SetNamedSecurityInfoW(
            wide(dir).as_ptr(),
            SE_FILE_OBJECT,
            information,
            owner,
            null_mut(),
            dacl,
            null(),
        );
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        let file_sd = descriptor(&format!("O:{sid}D:P(A;;FA;;;{sid})"))?;
        let security = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: file_sd.0,
            bInheritHandle: 0,
        };
        let temp = dir.join(format!(".companion.json.{}.tmp", std::process::id()));
        let handle = CreateFileW(
            wide(&temp).as_ptr(),
            GENERIC_WRITE,
            0,
            &security,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        );
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let mut file = std::fs::File::from_raw_handle(handle);
        let result = (|| {
            file.write_all(&serde_json::to_vec(value)?)?;
            checked(FlushFileBuffers(file.as_raw_handle()))?;
            drop(file);
            checked(MoveFileExW(
                wide(&temp).as_ptr(),
                wide(path).as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            ))?;
            verify(path, &sid)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
            let _ = std::fs::remove_file(path);
        }
        result
    }
}
fn owned_by(path: &Path, sid: PSID) -> io::Result<bool> {
    unsafe {
        let mut owner = null_mut();
        let mut sd = null_mut();
        let status = GetNamedSecurityInfoW(
            wide(path).as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            null_mut(),
            null_mut(),
            &mut sd,
        );
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        let _allocated = Local(sd);
        Ok(EqualSid(owner, sid) != 0)
    }
}
fn verify(path: &Path, sid: &str) -> io::Result<()> {
    unsafe {
        let mut owner = null_mut();
        let mut acl = null_mut();
        let mut sd = null_mut();
        let status = GetNamedSecurityInfoW(
            wide(path).as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut acl,
            null_mut(),
            &mut sd,
        );
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        let _allocated = Local(sd);
        let mut expected = null_mut();
        checked(ConvertStringSidToSidW(wide(sid).as_ptr(), &mut expected))?;
        let _expected = Local(expected);
        let mut control = 0;
        let mut revision = 0;
        checked(GetSecurityDescriptorControl(
            sd,
            &mut control,
            &mut revision,
        ))?;
        if control & SE_DACL_PROTECTED == 0
            || acl.is_null()
            || (*acl).AceCount != 1
            || EqualSid(owner, expected) == 0
        {
            return Err(io::Error::other("The runtime file security is invalid."));
        }
        let mut ace = null_mut();
        checked(GetAce(acl, 0, &mut ace))?;
        let ace = &*ace.cast::<ACCESS_ALLOWED_ACE>();
        if ace.Header.AceType!=0 /* ACCESS_ALLOWED_ACE_TYPE */||EqualSid((&ace.SidStart as *const u32).cast_mut().cast(),expected)==0
        {
            return Err(io::Error::other(
                "The runtime file grants unexpected access.",
            ));
        }
        Ok(())
    }
}
