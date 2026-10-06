//! Notedown 解析错误。

use std::fmt::{Display, Formatter};

/// Notedown 解析/格式化错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotedownError {
    /// 词法/语法错误。
    Parse {
        /// 行号（1-based）。
        line: usize,
        /// 错误描述。
        message: String,
    },
}

impl Display for NotedownError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse { line, message } => write!(f, "notedown parse error at line {line}: {message}"),
        }
    }
}

impl std::error::Error for NotedownError {}

impl NotedownError {
    /// 创建解析错误。
    pub fn parse(line: usize, message: impl Into<String>) -> Self {
        Self::Parse { line, message: message.into() }
    }
}
