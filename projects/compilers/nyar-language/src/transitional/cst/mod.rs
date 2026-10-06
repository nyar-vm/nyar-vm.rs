//! CST / 源码格式化过渡模型。
//!
//! CST / 源码格式化过渡模型（已自 vcc-data 迁入，待 Oak formatter 正式命名）。

/// Legacy Valkyrie CST（仅测试对照；生产解析在 `oak-valkyrie`）。
#[cfg(test)]
pub mod valkyrie;
/// Legacy VON CST（仅测试对照；生产解析在 `oak-von`）。
#[cfg(test)]
pub mod von;
