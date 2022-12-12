#![warn(missing_docs)]
#![feature(new_range_api)]
//! Rusty Go 解释器

pub mod frontend;
pub mod optimizer;
pub mod runtime;

pub use crate::frontend::RustyGoFrontend;
pub use crate::runtime::RustyGoRuntime;
