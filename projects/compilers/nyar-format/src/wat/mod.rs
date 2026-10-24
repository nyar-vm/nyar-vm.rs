//! WAT 文本格式化。

pub mod format;
mod model;

pub use format::format_wat_document;
pub use model::{WatDocument, WatError};
