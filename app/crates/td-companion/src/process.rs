use crate::{
    api::Ctx,
    devices::{PortSource, RealPorts},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
#[cfg(windows)]
#[path = "windows.rs"]
mod platform;
#[cfg(not(windows))]
mod platform {
    use std::{
        fs::{File, OpenOptions},
        io::{self, Write},
        os::unix::fs::{OpenOptionsExt, PermissionsExt},
        path::Path,
    };
    pub struct Instance {
        _file: File,
    }
    impl Instance {
        pub fn acquire(root: &Path) -> io::Result<Self> {
            let file = OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(root.join("companion.lock"))?;
            fs2::FileExt::try_lock_exclusive(&file)
                .map_err(|_| io::Error::other("The Companion is already running."))?;
            Ok(Self { _file: file })
        }
    }
    pub fn write_runtime(path: &Path, value: &serde_json::Value) -> io::Result<()> {
        let dir = path.parent().unwrap();
        std::fs::create_dir_all(dir)?;
        if std::fs::symlink_metadata(dir)?.file_type().is_symlink() {
            return Err(io::Error::other("The runtime directory is a symlink."));
        }
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        let temp = dir.join(format!(".companion.json.{}.tmp", std::process::id()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temp)?;
        let result = (|| {
            file.write_all(&serde_json::to_vec(value)?)?;
            file.sync_all()?;
            std::fs::rename(&temp, path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temp);
        }
        result
    }
}
use platform::write_runtime;
/// Rewrites the runtime file off the async runtime, holding the runtime lock. Returns false
/// when the file couldn't be secured.
pub(crate) async fn rewrite_runtime(ctx: &Ctx) -> bool {
    let Some(path) = ctx.runtime.clone() else {
        return true;
    };
    let ctx = ctx.clone();
    tokio::task::spawn_blocking(move || {
        let _held = ctx.runtime_lock.lock();
        write_runtime(&path, &ctx.runtime_value()).is_ok()
    })
    .await
    .unwrap_or(false)
}
struct RuntimeFile(PathBuf);
impl Drop for RuntimeFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn root() -> std::io::Result<PathBuf> {
    std::env::var_os("TOUCHDECK_APP_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::data_local_dir().map(|p| p.join(td_core::app_dirs::APP_ID)))
        .ok_or_else(|| std::io::Error::other("No app data directory."))
}
async fn bind(root: &Path) -> std::io::Result<tokio::net::TcpListener> {
    let settings = root.join("companion-settings.json");
    let port = std::fs::read(&settings)
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| v["port"].as_u64())
        .filter(|p| (49152..=65535).contains(p));
    if let Some(port) = port
        && let Ok(listener) =
            tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port as u16)).await
    {
        return Ok(listener);
    }
    for _ in 0..128 {
        let mut bytes = [0; 2];
        getrandom::fill(&mut bytes).map_err(std::io::Error::other)?;
        let port = 49152 + (u16::from_le_bytes(bytes) % 16384);
        if let Ok(listener) =
            tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await
        {
            std::fs::write(
                settings,
                serde_json::to_vec(&serde_json::json!({"port":port}))?,
            )?;
            return Ok(listener);
        }
    }
    Err(std::io::Error::other("No free Companion port."))
}
pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let (source, mock): (Arc<dyn PortSource>, bool) = match args.as_slice() {
        [] => (Arc::new(RealPorts::default()), false),
        #[cfg(feature = "test-hooks")]
        ["--emit-ts", path] => {
            if let Some(parent) = Path::new(path).parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, crate::bindings::generate())?;
            return Ok(());
        }
        #[cfg(feature = "test-hooks")]
        ["--mock-devices", count] => {
            let ports = crate::devices::MockPorts::default();
            for n in 1..=count.parse::<usize>()? {
                ports.plug(&format!("mock{n:04}"));
            }
            (Arc::new(ports), true)
        }
        _ => return Err("Unknown Companion argument.".into()),
    };
    let root = root()?;
    std::fs::create_dir_all(&root)?;
    let _instance = platform::Instance::acquire(&root)?;
    let listener = bind(&root).await?;
    assert!(listener.local_addr()?.ip().is_loopback());
    let runtime = root.join("runtime/companion.json");
    #[cfg(not(windows))]
    let runtime = if std::env::var_os("TOUCHDECK_APP_DATA_DIR").is_none() {
        std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .ok_or("XDG_RUNTIME_DIR is required")?
            .join("touchdeck/companion.json")
    } else {
        runtime
    };
    let ctx = Ctx::new(
        listener.local_addr()?.port(),
        mock,
        Some(runtime.clone()),
        crate::Limits::default(),
    );
    write_runtime(&runtime, &ctx.runtime_value())
        .map_err(|e| format!("The runtime file could not be secured: {e}"))?;
    let _runtime = RuntimeFile(runtime.clone());
    let refresh = ctx.clone();
    let worker = tokio::spawn(async move {
        loop {
            let source = source.clone();
            let ports = tokio::task::spawn_blocking(move || source.scan())
                .await
                .unwrap_or_default();
            refresh.devices.refresh(ports);
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
    let repair = ctx.clone();
    let maintenance = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            if !rewrite_runtime(&repair).await {
                let _ = repair.shutdown.send(true);
                break;
            }
        }
    });
    let signal = ctx.shutdown.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            let _ = signal.send(true);
        }
    });
    let stop = ctx.shutdown.subscribe();
    let limits = ctx.limits;
    crate::serve::serve(listener, crate::api::router(ctx), limits, stop).await;
    worker.abort();
    maintenance.abort();
    Ok(())
}
