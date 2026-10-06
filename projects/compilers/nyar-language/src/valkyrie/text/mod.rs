//! 文本格式化能力（`std-data` 负责 lexer/parser，本模块负责输出文本）。

pub mod awsl;
pub mod format_syntax;
pub use crate::notedown::highlight;
pub mod msil;
pub mod to_document;
pub mod valkyrie;
pub mod von;
pub mod wat;
pub mod wit;

pub use format_syntax::FormatSyntax;
pub use to_document::ToDocument;
