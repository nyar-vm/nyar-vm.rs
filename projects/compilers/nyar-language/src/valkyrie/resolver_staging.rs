//! 过渡对照：旧 `tgrammar` 文本展开仅供迁移测试，不是生产 API。

#[cfg(test)]
pub(crate) fn expand_target_templates_for_arch(source: &str, arch: &str) -> String {
    crate::transitional::tgrammar::preprocess_target_templates(source, arch)
}

#[cfg(test)]
mod tests {
    use super::expand_target_templates_for_arch;

    #[test]
    fn transitional_text_expander_matches_arch_branch() {
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
