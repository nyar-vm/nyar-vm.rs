//! T-Grammar 编译期展开：Oak 前端阶段暂为 no-op。

use crate::valkyrie::frontend::ValkyrieRoot;

/// 在 HIR 降级前展开根 AST 中的 T-Grammar 节点（Oak 前端暂跳过）。
pub fn expand_tgrammar_in_root(_root: &mut ValkyrieRoot) {}
