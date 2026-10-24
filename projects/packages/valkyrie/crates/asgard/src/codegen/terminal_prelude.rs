//! 终端 TUI 宿主 V 预置：host_contract 原语声明 + 项目依赖闭包提供的 tui.v 运行时。
//!
//! 终端路径是纯 Valkyrie 实现：所有 TUI 业务逻辑（widget 树构建、绑定存储、焦点导航、
//! 渲染调度、事件循环）由 `tui.v` 提供，宿主只提供字符级 I/O 原语（`host_contract`）。
//! `tui.v` 属于 `std` 包，asgard 不携带其副本；运行时前导由调用方从
//! package graph 解析出的源码组传入。

/// 终端字符级 I/O 原语（host_contract）：由原生宿主实现，V 侧仅声明契约。
///
/// 这些声明与 `valkyrie.v/projects/std/source/terminal/_.v` 保持一致，
/// 在生成的 `app.v` 顶部出现，供 `tui.v` 运行时调用。
pub const TERMINAL_HOST_PRELUDE: &str = r#"
[host_contract] micro clear(): unit
[host_contract] micro set_foreground(color: i32): unit
[host_contract] micro set_background(color: i32): unit
[host_contract] micro reset_color(): unit
[host_contract] micro move_to(row: i32, col: i32): unit
[host_contract] micro size_rows(): i32
[host_contract] micro size_cols(): i32
[host_contract] micro poll_key(): i32
[host_contract] micro enter_alt_screen(): unit
[host_contract] micro exit_alt_screen(): unit
[host_contract] micro flush(): unit
[host_contract] micro put_char(ch: i32): unit
[host_contract] micro put_str(text: utf8): unit
[host_contract] micro set_reverse(on: i32): unit
[host_contract] micro sleep_ms(ms: i32): unit
"#;

/// 终端 V 预置：host_contract 声明 + `tui.v` 运行时，作为 AWSL 生成代码的前导。
pub fn terminal_prelude(tui_runtime: &str) -> String {
    let mut out = String::new();
    out.push_str(TERMINAL_HOST_PRELUDE);
    out.push('\n');
    out.push_str(tui_runtime);
    out.push('\n');
    out
}
#[cfg(test)]
#[path = "../../tests/path_wired/codegen_terminal_prelude.rs"]
mod codegen_terminal_prelude_tests;
