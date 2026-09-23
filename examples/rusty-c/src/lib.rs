#![warn(missing_docs)]
#![feature(new_range_api)]
//! Rusty C 解释器

pub mod frontend;
pub mod optimizer;
pub mod runtime;

pub use crate::frontend::RustyCFrontend;
pub use crate::runtime::RustyCRuntime;
