pub mod api;
#[cfg(any(test, feature = "test-hooks"))]
pub mod bindings;
pub mod devices;
pub mod features;
pub use api::run;
#[cfg(any(test, feature = "test-hooks"))]
pub use api::testing;

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
