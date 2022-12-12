//! AWSL 文本格式化（CST → Document）。

mod cst_format;
pub(crate) mod source_format;

pub use cst_format::format_awsl_cst;
