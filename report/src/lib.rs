//! REPORT library — exposes modules for integration testing.
pub mod config;
pub mod daemon;
pub mod input;
pub mod mapping;
pub mod naming;
// Temporarily stubbed until Task 10 replaces with discovery-backed source map.
#[allow(dead_code)]
pub(crate) mod ndi_find {
    pub struct Discovery;
    impl Discovery {
        pub fn new() -> anyhow::Result<Self> {
            anyhow::bail!("NDI removed; rewire daemon.rs to temple::Receiver")
        }
        pub fn poll(&self, _t: std::time::Duration) -> Vec<String> {
            Vec::new()
        }
    }
}
pub mod pipeline;
