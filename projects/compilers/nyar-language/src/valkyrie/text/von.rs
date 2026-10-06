//! VON 文本格式化：`.von` 经 `oak_von::formatter`（`von::source_format`）。

pub use crate::von::{format_von_compact, format_von_pretty};
#[cfg(feature = "serde")]
pub use crate::von::{to_string, to_string_pretty};
