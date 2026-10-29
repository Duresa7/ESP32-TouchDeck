use crate::api::{CommonError, Ctx};
use parking_lot::Mutex;
use std::sync::Arc;
#[cfg(windows)]
mod windows;

pub trait BrowserOpener: Send + Sync {
    fn open(&self, url: &str) -> Result<(), CommonError>;
}
pub trait TrayNotifier: Send + Sync {
    fn notify(&self, message: &str);
}
#[derive(Default)]
pub struct FakeBrowserOpener(pub Mutex<Option<String>>);
impl BrowserOpener for FakeBrowserOpener {
    fn open(&self, url: &str) -> Result<(), CommonError> {
        *self.0.lock() = Some(url.into());
        Ok(())
    }
}
#[derive(Default)]
pub struct FakeTrayNotifier(pub Mutex<Option<String>>);
impl TrayNotifier for FakeTrayNotifier {
    fn notify(&self, message: &str) {
        *self.0.lock() = Some(message.into());
    }
}
pub struct Desktop {
    opener: Arc<dyn BrowserOpener>,
    notifier: Arc<dyn TrayNotifier>,
    pub startup: Arc<dyn crate::startup::Startup>,
    #[cfg(any(test, feature = "test-hooks"))]
    pub captures: Option<(Arc<FakeBrowserOpener>, Arc<FakeTrayNotifier>)>,
}
impl Desktop {
    pub(crate) fn new(mock: bool) -> Self {
        if mock {
            let opener = Arc::new(FakeBrowserOpener::default());
            let notifier = Arc::new(FakeTrayNotifier::default());
            return Self {
                opener: opener.clone(),
                notifier: notifier.clone(),
                startup: Arc::new(crate::startup::FakeStartup::default()),
                #[cfg(any(test, feature = "test-hooks"))]
                captures: Some((opener, notifier)),
            };
        }
        Self {
            opener: Arc::new(NativeBrowser),
            notifier: Arc::new(NativeNotifier),
            startup: Arc::new(crate::startup::NativeStartup),
            #[cfg(any(test, feature = "test-hooks"))]
            captures: None,
        }
    }
    pub(crate) fn deliver(&self, port: u16, code: &str) -> Result<(), CommonError> {
        self.notifier.notify(code);
        self.opener
            .open(&format!("http://127.0.0.1:{port}/pair#code={code}"))
    }
    pub(crate) fn notify(&self, message: &str) {
        self.notifier.notify(message);
    }
}
struct NativeBrowser;
impl BrowserOpener for NativeBrowser {
    fn open(&self, url: &str) -> Result<(), CommonError> {
        #[cfg(windows)]
        {
            windows::open(url)
        }
        #[cfg(not(windows))]
        {
            let _ = url;
            Err(CommonError::Internal)
        }
    }
}
struct NativeNotifier;
impl TrayNotifier for NativeNotifier {
    fn notify(&self, message: &str) {
        #[cfg(windows)]
        windows::notify(message);
        #[cfg(not(windows))]
        {
            let _ = message;
            eprintln!("Native tray notification unavailable.");
        }
    }
}
pub(crate) fn start(ctx: Ctx) {
    #[cfg(windows)]
    windows::start(ctx);
    #[cfg(not(windows))]
    {
        let _ = ctx;
        eprintln!("Native tray unavailable on this platform.");
    }
}

#[cfg(any(test, feature = "test-hooks"))]
mod captures {
    use super::*;
    use crate::api::{Endpoint, Feature, NoQuery, Registration, Req, SessionKind};
    use axum::http::Method;
    use serde::{Deserialize, Serialize};
    use ts_rs::TS;
    #[derive(Serialize, Deserialize, TS)]
    pub struct Delivery {
        pub url: Option<String>,
        pub notification: Option<String>,
    }
    pub struct Captures;
    impl Endpoint for Captures {
        const NAME: &'static str = "test.pairing_delivery";
        const METHOD: Method = Method::GET;
        const PATH: &'static str = "/test/pairing-delivery";
        type Path = ();
        type Query = NoQuery;
        type Body = ();
        type Ok = Delivery;
        type Error = CommonError;
        async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
            if req.client().kind != SessionKind::Window {
                return Err(CommonError::Forbidden("A window session is required."));
            }
            let (opener, notifier) = ctx.desktop.captures.as_ref().ok_or(CommonError::NotFound)?;
            Ok(Delivery {
                url: opener.0.lock().take(),
                notification: notifier.0.lock().take(),
            })
        }
    }
    fn feature() -> Feature {
        Feature::new("test").endpoint::<Captures>()
    }
    inventory::submit! { Registration(feature) }
}
