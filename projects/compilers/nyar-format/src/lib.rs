//! 跨语言源码格式化与 AST pretty printer。
//!
//! 本 crate 只注册与转发：Valkyrie / VON / AWSL 的源码格式化委托对应 Oak formatter，
//! AST pretty print 委托 `oak-*::printer`；WAT / WIT / MSIL 的文本模型与写出在此实现。
//! 语言语义不属于本 crate。

#![deny(missing_docs)]

pub mod awsl;
pub mod formatter;
pub mod msil;
pub mod printer;
pub mod von;
pub mod wat;
pub mod wit;
