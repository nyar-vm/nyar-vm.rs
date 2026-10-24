//! Valkyrie 文本解析入口。
//!
//! 正式编译流只能调用 [`parse`] / [`parse_source`]，不得引入第二条 parser 成功路径。

pub use crate::frontend::{self, ValkyrieRoot, parse_source, std_range};
pub use oak_core::OakDiagnostics;

use oak_core::{Builder, ParseSession, SourceText};
use oak_valkyrie::{ValkyrieBuilder, ValkyrieLanguage};

/// Oak 前端返回的解析结果（含 diagnostics）。
pub type ParseOutput = OakDiagnostics<frontend::ast::ValkyrieRoot>;

/// 使用 Oak 解析并构造 Valkyrie AST 根节点。
pub fn parse(source: &str) -> ParseOutput {
    let language = ValkyrieLanguage::default();
    let builder = ValkyrieBuilder::new(&language);
    let text = SourceText::new(source);
    let mut session = ParseSession::<ValkyrieLanguage>::default();
    builder.build(&text, &[], &mut session)
}
