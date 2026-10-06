//! CST / 源码格式化过渡模型。
//!
//! `von` / `awsl` 已自 vcc-data 迁入；`valkyrie` 仍经 `vcc-data`，待 Oak formatter 落地。

pub mod awsl;
pub mod von;

pub use vcc_data::text::valkyrie;
