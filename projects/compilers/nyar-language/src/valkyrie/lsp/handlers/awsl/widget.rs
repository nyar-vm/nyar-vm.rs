//! AWSL ↔ Valkyrie `widget` 统一视图
//!
//! AWSL 是 Valkyrie widget 机制的 Vue 风格表面语法：`<widget>` + `<script>` 等价于
//! `widget Name { … }`，模板经 asgard 前置降级后汇入同一 HIR/MIR 主线，不是平行语言。

use std_data::text::awsl::{AwslRoot, widget_name_from_stem};

/// 从 AWSL 根节点与文件名推导 widget 名（snake_case）。
pub fn widget_name_from_root(root: &AwslRoot, fallback_stem: &str) -> String {
    root.widget_name
        .as_deref()
        .filter(|n| !n.is_empty())
        .map(|n| n.to_string())
        .unwrap_or_else(|| widget_name_from_stem(fallback_stem))
}

/// 将 AWSL `<script>` 块包装为 Valkyrie `widget` 源码，供语义分析与符号索引复用。
/// script 正文语义为 **vx**（`parse_vx_root`），不是核心 `.v`。
///
/// 返回 `(synthetic_v, prefix_len)`：`prefix_len` 为 script 正文在 synthetic 中的起始偏移。
pub fn synthetic_widget_source(root: &AwslRoot, fallback_stem: &str) -> Option<(String, usize)> {
    let script = root.script.as_ref()?;
    let name = widget_name_from_root(root, fallback_stem);
    let prefix = format!("widget {name} {{\n");
    let suffix = "\n}\n";
    let prefix_len = prefix.len();
    Some((format!("{prefix}{script}{suffix}"), prefix_len))
}

/// 将 synthetic widget AST span 映射回 AWSL 源文件中 `<script>` 正文的字节 range。
pub fn map_synthetic_span_to_file(
    script_file_start: usize,
    synthetic_prefix_len: usize,
    synthetic_range: std::ops::Range<usize>,
) -> std::ops::Range<usize> {
    let rel_start = synthetic_range.start.saturating_sub(synthetic_prefix_len);
    let rel_end = synthetic_range.end.saturating_sub(synthetic_prefix_len);
    script_file_start + rel_start..script_file_start + rel_end
}

#[cfg(test)]
mod tests {
    use super::map_synthetic_span_to_file;

    #[test]
    fn map_synthetic_span_to_file_removes_prefix_and_adds_script_base() {
        let mapped = map_synthetic_span_to_file(120, 18, 26..30);
        assert_eq!(mapped, 128..132);
    }
}

/// 从 URI 推导组件文件名 stem（如 `todo.awsl` → `todo`）。
pub fn component_stem_from_uri(uri: &str) -> String {
    url::Url::parse(uri)
        .ok()
        .and_then(|u| u.to_file_path().ok())
        .and_then(|p| {
            p.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "component".into())
}
