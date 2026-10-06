//! VON 过渡导出：CST 源码格式化仍走 `vcc-data`，待 `oak-von::formatter` 对齐 `oak-typescript`。

pub use crate::von::{format_von_compact, format_von_cst, format_von_pretty};
#[cfg(feature = "serde")]
pub use crate::von::{to_string, to_string_pretty};
