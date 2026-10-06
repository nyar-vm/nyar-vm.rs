//! Valkyrie 文本格式化兼容导出。
//!
//! `.v` 正规格式化走 `oak_valkyrie::formatter`（经 `source_format::format_valkyrie`）。
//! `.vx` 与显式 CST 路径仍导出 legacy `format_valkyrie_cst`。

pub use crate::valkyrie::cst_format::format_valkyrie_cst;
