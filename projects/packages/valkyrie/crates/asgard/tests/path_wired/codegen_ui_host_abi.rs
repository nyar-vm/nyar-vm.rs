//! Migrated from `src/codegen/ui_host_abi.rs` (`tests`).

use super::*;

#[test]
fn call_export_prefixes_handler() {
    assert_eq!(resolve_call_export("on_tap"), "awsl_call_on_tap");
    assert_eq!(resolve_call_export("awsl_call_on_tap"), "awsl_call_on_tap");
}

#[test]
fn sig_export_uses_route_and_name() {
    assert_eq!(resolve_sig_export("Counter", "count"), "awsl_sig_counter_count");
}
