use crate::{api::Ctx, devices::Config, runtime::Runtime};
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
    // How many mock Devices to plug in, when the Companion runs on mock ports.
    let mock_devices: Option<usize> = match args.as_slice() {
        [] => None,
        #[cfg(feature = "test-hooks")]
        ["--emit-ts", path] => {
            if let Some(parent) = Path::new(path).parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, crate::bindings::generate())?;
            let test = Path::new(path).with_file_name("api.test.gen.ts");
            std::fs::write(test, crate::bindings::generate_test())?;
            return Ok(());
        }
        #[cfg(feature = "test-hooks")]
        ["--mock-devices", count] => Some(count.parse()?),
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
    let port = listener.local_addr()?.port();
    let (data, open) = (root.clone(), runtime.clone());
    let ctx = tokio::task::spawn_blocking(move || {
        let grants = crate::actions::FileGrants::new(data.join("allowed-actions.json"));
        Ctx::new(
            port,
            mock_devices.is_some(),
            Some(open),
            crate::Limits::default(),
            &data,
            Arc::new(grants),
        )
    })
    .await?;
    let initial = ctx.clone();
    let written = tokio::task::spawn_blocking(move || {
        let value = initial.runtime_value();
        initial.runtime.as_ref().unwrap().repair(&value)
    })
    .await?;
    written.map_err(|e| format!("The runtime file could not be secured: {e}"))?;
    match mock_devices {
        None => ctx.devices.start(Config::real(&root)),
        #[cfg(feature = "test-hooks")]
        Some(count) => {
            // Mock Devices are kept in memory, so they never reach a real `devices.json`.
            let config = Config::mock(None);
            let (ports, _) = config.mock.clone().expect("mock config");
            ctx.devices.start(config);
            for n in 1..=count {
                ports.plug(&format!("mock{n:04}"), crate::devices::BoardKind::Device);
            }
        }
        #[cfg(not(feature = "test-hooks"))]
        Some(_) => unreachable!("only test-hooks builds take --mock-devices"),
    }
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
    let devices = ctx.devices.clone();
    crate::serve::serve(listener, crate::api::router(ctx), limits, stop).await;
    devices.stop();
    maintenance.abort();
    runtime.remove();
    Ok(())
}
