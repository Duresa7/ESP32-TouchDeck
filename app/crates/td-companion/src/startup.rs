use crate::api::{CommonError, Ctx, Endpoint, Feature, NoQuery, Registration, Req};
use axum::http::Method;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
pub trait Startup: Send + Sync {
    fn get(&self) -> Result<bool, CommonError>;
    fn set(&self, enabled: bool) -> Result<(), CommonError>;
}
#[derive(Default)]
pub struct FakeStartup(pub std::sync::atomic::AtomicBool);
impl Startup for FakeStartup {
    fn get(&self) -> Result<bool, CommonError> {
        Ok(self.0.load(std::sync::atomic::Ordering::SeqCst))
    }
    fn set(&self, enabled: bool) -> Result<(), CommonError> {
        self.0.store(enabled, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}
pub struct NativeStartup;
#[cfg(windows)]
mod native {
    use super::*;
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::{Foundation::*, System::Registry::*};
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    const KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const NAME: &str = "Open TouchDeck";
    impl Startup for NativeStartup {
        fn get(&self) -> Result<bool, CommonError> {
            let mut size = 0;
            let result = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    wide(KEY).as_ptr(),
                    wide(NAME).as_ptr(),
                    RRF_RT_REG_SZ,
                    null_mut(),
                    null_mut(),
                    &mut size,
                )
            };
            match result {
                ERROR_SUCCESS => Ok(true),
                ERROR_FILE_NOT_FOUND => Ok(false),
                _ => Err(CommonError::Internal),
            }
        }
        fn set(&self, enabled: bool) -> Result<(), CommonError> {
            let command = if enabled {
                let exe = std::env::current_exe().map_err(|_| CommonError::Internal)?;
                wide(&format!("\"{}\" --packaged", exe.display()))
            } else {
                Vec::new()
            };
            let mut key = null_mut();
            let result = unsafe {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    wide(KEY).as_ptr(),
                    0,
                    null(),
                    0,
                    KEY_SET_VALUE,
                    null(),
                    &mut key,
                    null_mut(),
                )
            };
            if result != ERROR_SUCCESS {
                return Err(CommonError::Internal);
            }
            let result = if enabled {
                unsafe {
                    RegSetValueExW(
                        key,
                        wide(NAME).as_ptr(),
                        0,
                        REG_SZ,
                        command.as_ptr().cast(),
                        (command.len() * 2) as u32,
                    )
                }
            } else {
                unsafe { RegDeleteValueW(key, wide(NAME).as_ptr()) }
            };
            unsafe {
                RegCloseKey(key);
            }
            if result == ERROR_SUCCESS || (!enabled && result == ERROR_FILE_NOT_FOUND) {
                Ok(())
            } else {
                Err(CommonError::Internal)
            }
        }
    }
}
#[cfg(not(windows))]
impl Startup for NativeStartup {
    fn get(&self) -> Result<bool, CommonError> {
        Ok(false)
    }
    fn set(&self, enabled: bool) -> Result<(), CommonError> {
        if enabled {
            Err(CommonError::Internal)
        } else {
            Ok(())
        }
    }
}
#[derive(Serialize, Deserialize, TS)]
pub struct StartupSetting {
    pub at_sign_in: bool,
}
pub struct Get;
impl Endpoint for Get {
    const NAME: &'static str = "system.startup_get";
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/system/startup";
    type Path = ();
    type Query = NoQuery;
    type Body = ();
    type Ok = StartupSetting;
    type Error = CommonError;
    async fn handle(ctx: Ctx, _: Req<Self>) -> Result<Self::Ok, Self::Error> {
        tokio::task::spawn_blocking(move || {
            ctx.desktop
                .startup
                .get()
                .map(|at_sign_in| StartupSetting { at_sign_in })
        })
        .await
        .map_err(|_| CommonError::Internal)?
    }
}
pub struct Put;
impl Endpoint for Put {
    const NAME: &'static str = "system.startup_put";
    const METHOD: Method = Method::PUT;
    const PATH: &'static str = "/system/startup";
    type Path = ();
    type Query = NoQuery;
    type Body = StartupSetting;
    type Ok = StartupSetting;
    type Error = CommonError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        tokio::task::spawn_blocking(move || {
            ctx.desktop.startup.set(req.body.at_sign_in)?;
            Ok(req.body)
        })
        .await
        .map_err(|_| CommonError::Internal)?
    }
}
fn feature() -> Feature {
    Feature::new("system").endpoint::<Get>().endpoint::<Put>()
}
inventory::submit! { Registration(feature) }
