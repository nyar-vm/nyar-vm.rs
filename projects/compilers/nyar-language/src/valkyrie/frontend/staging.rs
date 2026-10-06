//! Compiler 前端门禁：在 Oak TGrammar 节点尚未接入 HIR 展开前，拒绝错误旁路。

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

/// 拒绝仍停留在文本形态的 `<% match arch %>` 模板。
///
/// TGrammar 必须由 Oak 在 `support_t_grammar = true` 时解析为结构化节点；
/// 目标分支选择由 Compiler 在 HIR 展开阶段按 `CompilerBuildContext.arch` 完成。
/// Resolver 与 `transitional::tgrammar` 文本预处理不是生产路径。
pub fn reject_unexpanded_target_templates(source: &str) -> Result<(), ParseError> {
    if source.contains("<% match ") {
        return Err(ParseError::invalid(
            "源码含有未展开的 TGrammar 目标模板 `<% match ... %>`。\
             必须由 Oak 在 `support_t_grammar` 启用时解析为模板 AST 节点，\
             再由 Compiler 按 target/arch 展开；禁止 Resolver 或 `transitional::tgrammar` 文本预处理",
        ));
    }
    Ok(())
}
