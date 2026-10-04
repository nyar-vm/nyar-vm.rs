//! Semantic MIR 的实例身份链接边界。
//!
//! 链接器只接受前端已经冻结的 ItemInstanceId 闭包，不从名称或布局补造身份。

use crate::valkyrie::mir::MirModule;

/// 合并已由 Compiler 统一注册的依赖实例闭包。
pub(crate) fn link_reachable_dependency_mir(
    _consumer: &mut MirModule,
    dependency_mirs: &[MirModule],
) -> Result<(), std_data::text::valkyrie::ParseError> {
    if dependency_mirs.is_empty() {
        return Ok(());
    }
    Err(std_data::text::valkyrie::ParseError::invalid(
        "依赖链接要求 Compiler 提供统一 ItemInstanceId 闭包；名称链接路径已删除",
    ))
}
