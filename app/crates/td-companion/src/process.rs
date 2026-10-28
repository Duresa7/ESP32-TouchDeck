use crate::{
    api::Ctx,
    devices::{PortSource, RealPorts},
    link::{SerialSessions, Sessions},
    runtime::Runtime,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
/// Checks the runtime file off the async runtime and rewrites it on drift. Returns false when
/// it couldn't be secured after retries.
pub(crate) async fn rewrite_runtime(ctx: &Ctx) -> bool {
    let Some(runtime) = ctx.runtime.clone() else {
        return true;
    };
    let ctx = ctx.clone();
    tokio::task::spawn_blocking(move || runtime.repair(&ctx.runtime_value()).is_ok())
        .await
        .unwrap_or(false)
}
/// The 60 s maintenance step: sweeps ended sessions, then checks the runtime file. Returns
/// false when the Companion must stop.
pub(crate) async fn maintain(ctx: &Ctx) -> bool {
    ctx.sweep_sessions();
    rewrite_runtime(ctx).await
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
    type Source = (Arc<dyn PortSource>, Arc<dyn Sessions>, bool);
    let (source, sessions, mock): Source = match args.as_slice() {
        [] => (
            Arc::new(RealPorts::default()),
            Arc::new(SerialSessions),
            false,
        ),
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
            (Arc::new(ports.clone()), Arc::new(ports), true)
        }
        _ => return Err("Unknown Companion argument.".into()),
    };
    let root = root()?;
    std::fs::create_dir_all(&root)?;
    #[cfg(windows)]
    let dir = root.join("runtime");
    #[cfg(not(windows))]
    let dir = match std::env::var_os("TOUCHDECK_APP_DATA_DIR") {
        Some(_) => root.join("runtime"),
        None => std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .ok_or("XDG_RUNTIME_DIR is required")?
            .join("touchdeck"),
    };
    let runtime = Arc::new(Runtime::open(&dir)?);
    let listener = bind(&root).await?;
    assert!(listener.local_addr()?.ip().is_loopback());
    let ctx = Ctx::new(
        listener.local_addr()?.port(),
        mock,
        Some(runtime.clone()),
        crate::Limits::default(),
        &root,
        sessions,
    );
    let initial = ctx.clone();
    let written = tokio::task::spawn_blocking(move || {
        let value = initial.runtime_value();
        initial.runtime.as_ref().unwrap().repair(&value)
    })
    .await?;
    written.map_err(|e| format!("The runtime file could not be secured: {e}"))?;
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
            if !maintain(&repair).await {
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
    runtime.remove();
    Ok(())
}
