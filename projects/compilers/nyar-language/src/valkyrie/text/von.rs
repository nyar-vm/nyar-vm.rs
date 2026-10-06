//! VON 文本格式化：`.von` 经 `oak_von::formatter`（`von::source_format`）。

#[cfg(feature = "serde")]
pub use crate::von::{to_string, to_string_pretty};
