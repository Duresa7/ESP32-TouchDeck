pub mod api;
#[cfg(any(test, feature = "test-hooks"))]
pub mod bindings;
pub mod devices;
pub mod features;
mod process;
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
