//! Migrated from `src/host.rs` (`tests`).

use super::*;

#[test]
fn parse_terminal_aliases() {
    assert_eq!(HostPlatform::parse("terminal"), HostPlatform::Terminal);
    assert_eq!(HostPlatform::parse("TUI"), HostPlatform::Terminal);
    assert_eq!(HostPlatform::parse("  terminal  "), HostPlatform::Terminal);
}

#[test]
fn terminal_as_str() {
    assert_eq!(HostPlatform::Terminal.as_str(), "terminal");
}
