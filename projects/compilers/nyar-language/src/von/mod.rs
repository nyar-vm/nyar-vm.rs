//! VON 文本格式化。

#[cfg(test)]
mod cst_format;
pub(crate) mod source_format;
mod value_format;
pub use value_format::{format_von_compact, format_von_pretty};
#[cfg(feature = "serde")]
pub use value_format::{to_string, to_string_pretty};
