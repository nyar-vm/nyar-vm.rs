use std_data::text::valkyrie::ValkyrieRoot;

/// 当前 Rust 自举主线暂不启用 AST 级宏重写。
/// 该入口保留为稳定扩展点，保证 lowering 管线可编译、可演进。
pub(super) fn expand_macros_in_root(_root: &mut ValkyrieRoot) {}
