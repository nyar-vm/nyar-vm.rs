use std::fmt::{Display, Formatter};

/// 解析后的模式。当前将整个模式视为单个字面量。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPattern {
    /// 字面量模式文本。
    pub literal: String,
}

/// 模式解析错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// 模式字符串为空。
    EmptyPattern,
}

impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyPattern => write!(f, "模式字符串不能为空"),
        }
    }
}

impl std::error::Error for ParseError {}

/// 解析模式字符串。当前无正则语法，整体作为字面量处理。
pub fn parse(pattern: &str) -> Result<ParsedPattern, ParseError> {
    if pattern.is_empty() {
        return Err(ParseError::EmptyPattern);
    }

    Ok(ParsedPattern { literal: pattern.to_string() })
}
