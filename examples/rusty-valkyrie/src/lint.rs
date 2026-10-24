//! 编辑器 / CLI 诊断入口：只做 HIR 级验证，不进入正式产物链。

use crate::{HirError, hir::ValkyrieCompiler};

/// 对孤立 `.v` 源码做 HIR 级 lint。不进入正式产物链，也不是 Legion build 的替代入口。
pub fn lint_valkyrie_source(compiler: &ValkyrieCompiler, source: &str) -> Result<(), HirError> {
    compiler.compile_source(source).map(|_| ())
}

/// 对孤立 `.vx` 源码做 HIR 级 lint。
pub fn lint_valkyrie_vx_source(compiler: &ValkyrieCompiler, source: &str) -> Result<(), HirError> {
    compiler.compile_vx_source(source).map(|_| ())
}
