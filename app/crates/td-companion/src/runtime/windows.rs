//! Owner-only security on Windows: a protected DACL with one entry for the user, owned by
//! the user.
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

/// `ACCESS_ALLOWED_ACE_TYPE` from winnt.h.
const ACCESS_ALLOWED_ACE_TYPE: u32 = 0;

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
/// Memory the system allocated with `LocalAlloc`.
struct Local(*mut core::ffi::c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}

/// The user's SID in string form, from the process token.
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

/// Creates `runtime\` owned by the user with the protected DACL, or secures an existing one.
/// Refuses a reparse point and a folder another account owns; it never takes ownership.
pub(super) fn secure_dir(dir: &Path) -> io::Result<()> {
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let sid = user_sid()?;
    let folder = descriptor(&format!("O:{sid}D:P(A;OICI;FA;;;{sid})"))?;
    unsafe {
        let security = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: folder.0,
            bInheritHandle: 0,
        };
        if CreateDirectoryW(wide(dir).as_ptr(), &security) == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(ERROR_ALREADY_EXISTS as i32) {
                return Err(error);
            }
        }
        let attributes = GetFileAttributesW(wide(dir).as_ptr());
        if attributes == INVALID_FILE_ATTRIBUTES {
            return Err(io::Error::last_os_error());
        }
        if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::other(
                "it is a reparse point, such as a junction",
            ));
        }
        if attributes & FILE_ATTRIBUTE_DIRECTORY == 0 {
            return Err(io::Error::other("it is not a folder"));
        }
        if !owned_by_user(dir, &sid)? {
            return Err(io::Error::other(
                "another account owns it. Delete the folder or make yourself its owner",
            ));
        }
        let mut dacl = null_mut();
        let mut present = 0;
        let mut defaulted = 0;
        checked(GetSecurityDescriptorDacl(
            folder.0,
            &mut present,
            &mut dacl,
            &mut defaulted,
        ))?;
        // The owner may always change the DACL; the owner itself is left alone.
        let status = SetNamedSecurityInfoW(
            wide(dir).as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            dacl,
            null(),
        );
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
    }
    verify(dir, &sid)
}

/// Writes through a private temporary file and an atomic rename. A failed rename leaves the
/// old file in place; only a file whose security can't be verified is deleted.
pub(super) fn write_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let sid = user_sid()?;
    let private = descriptor(&format!("O:{sid}D:P(A;;FA;;;{sid})"))?;
    let temp = path.with_file_name(format!(".companion.json.{}.tmp", std::process::id()));
    unsafe {
        let security = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: private.0,
            bInheritHandle: 0,
        };
        let _ = std::fs::remove_file(&temp);
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
        let moved = (|| {
            file.write_all(bytes)?;
            checked(FlushFileBuffers(file.as_raw_handle()))?;
            drop(file);
            checked(MoveFileExW(
                wide(&temp).as_ptr(),
                wide(path).as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            ))
        })();
        if moved.is_err() {
            let _ = std::fs::remove_file(&temp);
            return moved;
        }
    }
    let verified = verify(path, &sid);
    if verified.is_err() {
        let _ = std::fs::remove_file(path);
    }
    verified
}

pub(super) fn verify_file(path: &Path) -> io::Result<()> {
    verify(path, &user_sid()?)
}

fn owned_by_user(path: &Path, sid: &str) -> io::Result<bool> {
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
        let mut expected = null_mut();
        checked(ConvertStringSidToSidW(wide(sid).as_ptr(), &mut expected))?;
        let _expected = Local(expected);
        Ok(EqualSid(owner, expected) != 0)
    }
}

/// The DACL is protected and holds exactly one allow entry, for the user, who also owns it.
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
            return Err(io::Error::other("its security is not owner-only"));
        }
        let mut ace = null_mut();
        checked(GetAce(acl, 0, &mut ace))?;
        let ace = &*ace.cast::<ACCESS_ALLOWED_ACE>();
        let ace_sid = (&ace.SidStart as *const u32).cast_mut().cast();
        if u32::from(ace.Header.AceType) != ACCESS_ALLOWED_ACE_TYPE
            || EqualSid(ace_sid, expected) == 0
        {
            return Err(io::Error::other("it grants access to another account"));
        }
        Ok(())
    }
}
