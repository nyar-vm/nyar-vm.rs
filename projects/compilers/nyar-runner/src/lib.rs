#![doc = include_str!("readme.md")]
#![warn(missing_docs)]

/// 统一运行时家族门面与运行模板描述。
pub mod runtime;

pub use runtime::{PreparedRuntimeCommand, RuntimeContract, RuntimeFamily, RuntimeTemplate};
