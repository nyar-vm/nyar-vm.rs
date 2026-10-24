//! Migrated from `src/host_backend.rs` (`tests`).

use super::*;

#[test]
fn terminal_maps_to_terminal_backend() {
    assert_eq!(HostBackend::from_platform(HostPlatform::Terminal), Ok(HostBackend::Terminal));
}
