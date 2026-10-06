//! Valkyrie 语言前端诊断错误（语法与 lowering 边界）。
//!
//! 由 Oak 前端与 HIR lowering 产出，不依赖 `vcc-data` parser 类型。

use std::fmt::{Display, Formatter};
use std::ops::Range;

use miette::{Diagnostic, LabeledSpan, Severity};

/// Valkyrie 前端解析与 lowering 错误。
#[derive(Debug)]
pub enum ParseError {
    /// 文件读取失败。
    Io(std::io::Error),
    /// 语法或 lowering 失败。
    Invalid {
        /// 错误消息。
        message: String,
        /// 可选源码范围。
        span: Option<Range<usize>>,
    },
}

impl ParseError {
    /// 构造无 span 的无效输入错误。
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid { message: message.into(), span: None }
    }

    /// 构造带 span 的无效输入错误。
    pub fn invalid_at(message: impl Into<String>, span: Range<usize>) -> Self {
        Self::Invalid { message: message.into(), span: Some(span) }
    }
}

impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => Display::fmt(error, f),
            Self::Invalid { message, .. } => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ParseError {}

impl Diagnostic for ParseError {
    fn code<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        Some(Box::new(match self {
            Self::Io(_) => "valkyrie::parser::io",
            Self::Invalid { .. } => "valkyrie::parser::invalid",
        }))
    }

    fn severity(&self) -> Option<Severity> {
        Some(Severity::Error)
    }

    fn help<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        Some(Box::new(match self {
            Self::Io(_) => "请确认源文件存在且当前进程具备读取权限",
            Self::Invalid { .. } => "请检查语法是否完整，尤其是声明头、括号、属性与对象体",
        }))
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        match self {
            Self::Invalid { span: Some(span), .. } => {
                let labeled = LabeledSpan::new_with_span(Some("解析失败位置".to_string()), (span.start, span.end.saturating_sub(span.start)));
                Some(Box::new(std::iter::once(labeled)))
            }
            _ => None,
        }
    }
}

impl From<std::io::Error> for ParseError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
