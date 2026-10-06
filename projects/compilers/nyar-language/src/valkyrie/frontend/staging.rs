//! Compiler 前端门禁：在 `support_t_grammar = false` 时拒绝模板源码。

use super::ParseError;

/// 当 `support_t_grammar = false` 时拒绝源码中的模板片段。
pub fn reject_unsupported_template_source(source: &str) -> Result<(), ParseError> {
    if source.contains("<%") {
        return Err(ParseError::invalid(
            "当前 `ValkyrieLanguage` 未启用 `support_t_grammar`，源码不得包含 TGrammar 模板片段",
        ));
    }
    Ok(())
}
