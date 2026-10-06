//! Resolver 预处理门禁：Compiler 只消费已展开的最终源码文本。

use super::ParseError;

/// 拒绝 Compiler 内对 `<% match arch %>` 模板片段的文本重解析。
///
/// 目标模板必须由 Oak 前端或 Resolver 预处理为结构化源码；HIR lowering
/// 与语义编译不得再扫描或展开模板文本。
pub fn reject_unexpanded_target_templates(source: &str) -> Result<(), ParseError> {
    if source.contains("<% match ") {
        return Err(ParseError::invalid(
            "源码含有未展开的目标模板 `<% match ... %>`。模板必须由 Oak 前端或 Resolver 预处理，Compiler 不再执行文本展开",
        ));
    }
    Ok(())
}
