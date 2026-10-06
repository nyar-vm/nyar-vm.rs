//! Valkyrie 语法前端：Oak 是唯一文本事实源。
//!
//! HIR lowering、Semantic MIR 与跨包链接只能消费本模块产出的 Oak AST，
//! 不得再经 `vcc-data::AstParser` 或任何 legacy AST 桥接层。

mod error;
mod naming;

use oak_core::{Builder, ParseSession, SourceText};
use oak_valkyrie::{ValkyrieBuilder, ValkyrieLanguage};

pub use error::ParseError;
pub use naming::{DIAG_ABI_BINDING_NOT_SNAKE_CASE, DIAG_IDENTIFIER_NOT_SNAKE_CASE, NamingViolation, validate_snake_case};

/// Oak 前端 AST 类型别名，供 lowering 直接引用。
pub use oak_valkyrie::ast;

/// 解析后的 Valkyrie 模块根。
pub type ValkyrieRoot = ast::ValkyrieRoot;

/// 将 Oak span 转为标准区间。
pub fn std_range(span: &oak_core::Range<usize>) -> std::ops::Range<usize> {
    span.start..span.end
}

/// 使用 Oak 解析源码；失败时返回结构化 `ParseError`，禁止回退旧 parser。
pub fn parse_source(source: &str) -> Result<ValkyrieRoot, ParseError> {
    let language = ValkyrieLanguage::default();
    let builder = ValkyrieBuilder::new(&language);
    let text = SourceText::new(source);
    let mut session = ParseSession::<ValkyrieLanguage>::default();
    let output = builder.build(&text, &[], &mut session);
    output.result.map_err(|error| ParseError::invalid(error.to_string()))
}
