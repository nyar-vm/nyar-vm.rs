//! 过渡层：仍在等待 Acorn 替代的 `vcc-data` 入口。
//!
//! 新代码不得在此扩展格式规则。只做 re-export 与桥接，便于逐后端迁入 `acorn-*`。

pub mod binary;
pub mod msil;
