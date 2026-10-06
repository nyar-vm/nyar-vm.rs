//! VON **源码**格式化（CST 路径；与 [`print_von`] 模型 printer 分离）。
//!
//! 源码正规格式化属于 Oak 上游职责，目标形态对齐 `oak-typescript::formatter`
//!（`oak-von` 的 `formatter` 模块落地后，本路径应切到 Oak CST formatter）。
//! 当前仍走过渡 `vcc-data` CST，不在 `nyar-language` 内扩展排版规则。

use crate::text::von::format_von_cst;

use crate::formatter::{FormatError, FormatOptions, FormattedOutput};

/// 格式化 `.von` 源码（保留 `#` 注释 trivia）。
pub(crate) fn format_von_source(source: &str, options: &FormatOptions) -> Result<FormattedOutput, FormatError> {
    format_von_cst(source, options)
}
