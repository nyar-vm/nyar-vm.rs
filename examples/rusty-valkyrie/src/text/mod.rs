//! 文本格式化能力（Oak `formatter` / `printer` 负责 lexer/parser，本模块负责 Valkyrie 侧契约）。

pub mod format_syntax;
pub mod to_document;
pub mod valkyrie;

pub use format_syntax::FormatSyntax;
pub use oak_valkyrie::printer::Document as PrettyDocument;
pub use to_document::ToDocument;
