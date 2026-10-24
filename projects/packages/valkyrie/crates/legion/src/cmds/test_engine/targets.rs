//! 测试目标解析。

use miette::{Result, miette};
use rusty_valkyrie::CanonicalTarget;

/// 解析 `--target` 参数（默认 `legion`，`all` 展开为 legion/clr/jvm/node）。
pub fn resolve_test_targets(target: Option<&str>) -> Vec<String> {
    match target {
        None | Some("") => vec!["legion".into()],
        Some(value) if value.eq_ignore_ascii_case("all") => {
            vec!["legion".into(), "clr".into(), "jvm".into(), "node".into()]
        }
        Some(value) => value.split(',').map(str::trim).filter(|item| !item.is_empty()).map(|item| item.to_ascii_lowercase()).collect(),
    }
}

/// 将短标签解析为 `CanonicalTarget`。
pub fn parse_target_label(label: &str) -> Result<CanonicalTarget> {
    label.parse::<CanonicalTarget>().map_err(|error| miette!("无法解析目标 '{label}': {error}"))
}
#[cfg(test)]
#[path = "../../../tests/path_wired/cmds_test_engine_targets.rs"]
mod cmds_test_engine_targets_tests;
