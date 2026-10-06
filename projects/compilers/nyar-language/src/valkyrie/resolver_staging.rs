//! Resolver 侧源码预处理；Compiler 只消费已展开的最终文本。

/// 按目标架构展开源码中的 `<% match arch %>` 模板片段。
///
/// Legion Resolver 在调用 `frontend::parse_source` 之前必须使用本函数；
/// Compiler 内禁止再次执行模板文本扫描或展开。
pub fn expand_target_templates_for_arch(source: &str, arch: &str) -> String {
    crate::transitional::tgrammar::preprocess_target_templates(source, arch)
}

#[cfg(test)]
mod tests {
    use super::expand_target_templates_for_arch;

    #[test]
    fn expands_arch_match_for_resolver() {
        let source = r#"<% match arch %>
<% case "wasm32" %>
return 1
<% else %>
return 0
<% end match %>"#;
        let expanded = expand_target_templates_for_arch(source, "wasm32");
        assert!(!expanded.contains("<% match "));
        assert!(expanded.contains("return 1"));
        assert!(!expanded.contains("return 0"));
    }
}
