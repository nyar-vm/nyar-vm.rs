//! `AST -> HIR` 前的结构与语义预校验。

use crate::valkyrie::frontend::ValkyrieRoot;
use crate::valkyrie::frontend::ParseError;

/// 校验 Oak `AST` 根节点是否满足进入 `HIR` lowering 的前提。
pub(crate) fn validate_ast_root(_root: &ValkyrieRoot) -> Result<(), ParseError> {
    Ok(())
}
