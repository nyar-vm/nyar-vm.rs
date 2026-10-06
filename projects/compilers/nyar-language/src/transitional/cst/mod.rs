//! CST / 源码格式化过渡模型。
//!
//! `von` 已自 vcc-data 迁入；`awsl` / `valkyrie` 仍经 `vcc-data`，待 Oak formatter 落地。

pub mod von;

pub use vcc_data::text::{awsl, valkyrie};
