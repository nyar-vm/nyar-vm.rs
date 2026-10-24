//! Migrated from `src/codegen/terminal_prelude.rs` (`tests`).

use super::*;

#[test]
fn host_prelude_declares_all_primitives() {
    for sym in [
        "micro clear",
        "micro set_foreground",
        "micro set_background",
        "micro reset_color",
        "micro move_to",
        "micro size_rows",
        "micro size_cols",
        "micro poll_key",
        "micro enter_alt_screen",
        "micro exit_alt_screen",
        "micro flush",
        "micro put_char",
        "micro put_str",
        "micro set_reverse",
        "micro sleep_ms",
    ] {
        assert!(TERMINAL_HOST_PRELUDE.contains(sym), "prelude 缺少声明 {sym}");
    }
}

/// 测试用运行时桩：真实 `tui.v` 由 package graph 提供，asgard 不内嵌副本。
const TEST_RUNTIME: &str = "structure TuiRuntime { }\nmicro widget_column(): unit { }\nmicro asgard_terminal_run(): i32 { return 0 }";

#[test]
fn terminal_prelude_combines_host_contract_and_runtime() {
    let combined = terminal_prelude(TEST_RUNTIME);
    assert!(combined.contains("[host_contract] micro clear"), "host_contract 缺失");
    assert!(combined.contains("structure TuiRuntime"), "运行时缺失");
    assert!(combined.contains("micro asgard_terminal_run"), "主入口缺失");
}
