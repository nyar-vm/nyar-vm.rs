//! WIT 文本格式化。

pub mod format;
mod model;

pub use format::format_wit_package;
pub use model::{WitError, WitInterface, WitPackage};
