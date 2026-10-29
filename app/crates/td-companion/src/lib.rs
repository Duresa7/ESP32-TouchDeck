pub mod actions;
pub mod api;
#[cfg(any(test, feature = "test-hooks"))]
pub mod bindings;
pub mod devices;
pub mod features;
pub mod jobs;
pub mod link;
mod process;
mod runtime;
mod serve;
pub mod startup;
pub mod store;
pub mod tray;
pub use serve::Limits;
#[cfg(any(test, feature = "test-hooks"))]
pub mod testing;
pub use process::run;

/// Wall-clock milliseconds, for display only.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
