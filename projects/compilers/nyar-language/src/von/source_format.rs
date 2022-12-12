//! VON **源码**格式化（CST 路径；与 [`print_von`] 模型 printer 分离）。

use crate::text::von::format_von_cst;

use crate::formatter::{FormatError, FormatOptions, FormattedOutput};

/// 格式化 `.von` 源码（保留 `#` 注释 trivia）。
pub(crate) fn format_von_source(source: &str, options: &FormatOptions) -> Result<FormattedOutput, FormatError> {
    format_von_cst(source, options)
}
