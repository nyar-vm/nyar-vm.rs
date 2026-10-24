#![doc = include_str!("readme.md")]
#![deny(missing_docs)]

pub mod advanced;
pub mod hir;
pub mod witness;

use miette::{Diagnostic, LabeledSpan as MietteLabeledSpan, Severity};
pub use nyar_types::{Identifier, NamePath, QualifiedName, SourceID, SourceSpan};

/// Severity level for diagnostic reports
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ReportKind {
    #[default]
    /// 错误级别。
    Error,
    /// 警告级别。
    Warning,
    /// 附注级别。
    Note,
    /// 帮助提示级别。
    Help,
}

impl ReportKind {
    /// 转换为 miette 严重级别。
    pub fn as_miette_severity(&self) -> Severity {
        match self {
            ReportKind::Error => Severity::Error,
            ReportKind::Warning => Severity::Warning,
            ReportKind::Note => Severity::Advice,
            ReportKind::Help => Severity::Advice,
        }
    }
}

/// A labeled span in source code for diagnostics
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LabeledSpan {
    /// 源码跨度。
    pub span: SourceSpan,
    /// 是否为主标签。
    pub primary: bool,
    /// 标签键（可选）。
    pub key: Option<String>,
    /// 附加键值数据。
    pub data: Vec<(String, String)>,
}

/// A help message for an error
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HelpMessage {
    /// 帮助文本键。
    pub key: String,
    /// 附加键值数据。
    pub data: Vec<(String, String)>,
}

/// The kind of error that occurred
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ValkyrieErrorKind {
    /// I/O error (file not found, permission denied, etc.)
    IoError {
        /// 错误消息。
        message: String,
        /// 相关文件路径（若有）。
        path: Option<String>,
    },
    /// Parse error (failed to parse source code)
    ParseError {
        /// 错误消息。
        message: String,
    },
    /// Syntax error (invalid syntax)
    SyntaxError {
        /// 错误消息。
        message: String,
    },
    /// Type error (type mismatch)
    TypeError {
        /// 期望类型描述。
        expected: String,
        /// 实际类型描述。
        found: String,
    },
    /// Runtime error (general runtime failure)
    RuntimeError {
        /// 错误消息。
        message: String,
    },
    /// VM error (stack underflow, index out of bounds, etc.)
    VmError {
        /// 错误码。
        code: u32,
        /// 错误键。
        key: String,
        /// 错误消息。
        message: String,
    },
    /// Compile error (AOT compilation failure)
    CompileError {
        /// 错误消息。
        message: String,
    },
    /// Naming convention violation (e.g. identifier must be snake_case).
    NamingViolation {
        /// lint 错误码。
        code: u32,
        /// 错误消息。
        message: String,
    },
    /// Unknown error
    Unknown,
}

impl std::fmt::Display for ValkyrieErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValkyrieErrorKind::IoError { message, path } => {
                if let Some(p) = path {
                    write!(f, "IoError: {} (path: {})", message, p)
                }
                else {
                    write!(f, "IoError: {}", message)
                }
            }
            ValkyrieErrorKind::ParseError { message } => {
                write!(f, "ParseError: {}", message)
            }
            ValkyrieErrorKind::SyntaxError { message } => {
                write!(f, "SyntaxError: {}", message)
            }
            ValkyrieErrorKind::TypeError { expected, found } => {
                write!(f, "TypeError: expected {}, found {}", expected, found)
            }
            ValkyrieErrorKind::RuntimeError { message } => {
                write!(f, "RuntimeError: {}", message)
            }
            ValkyrieErrorKind::VmError { code, key, message } => {
                write!(f, "VmError [E{:04X}]: {} - {}", code, key, message)
            }
            ValkyrieErrorKind::CompileError { message } => {
                write!(f, "CompileError: {}", message)
            }
            ValkyrieErrorKind::NamingViolation { code, message } => {
                write!(f, "NamingViolation [E{:04X}]: {}", code, message)
            }
            ValkyrieErrorKind::Unknown => {
                write!(f, "Unknown error")
            }
        }
    }
}

impl ValkyrieErrorKind {
    /// 返回机器可读错误键。
    pub fn key(&self) -> String {
        match self {
            ValkyrieErrorKind::IoError { .. } => "io_error".to_string(),
            ValkyrieErrorKind::ParseError { .. } => "parse_error".to_string(),
            ValkyrieErrorKind::SyntaxError { .. } => "syntax_error".to_string(),
            ValkyrieErrorKind::TypeError { .. } => "type_error".to_string(),
            ValkyrieErrorKind::RuntimeError { .. } => "runtime_error".to_string(),
            ValkyrieErrorKind::VmError { key, .. } => key.clone(),
            ValkyrieErrorKind::CompileError { .. } => "compile_error".to_string(),
            ValkyrieErrorKind::NamingViolation { .. } => "naming_violation".to_string(),
            ValkyrieErrorKind::Unknown => "unknown_error".to_string(),
        }
    }

    /// 返回数值错误码。
    pub fn code(&self) -> u32 {
        match self {
            ValkyrieErrorKind::IoError { .. } => 0x0001,
            ValkyrieErrorKind::ParseError { .. } => 0x0002,
            ValkyrieErrorKind::SyntaxError { .. } => 0x0005,
            ValkyrieErrorKind::TypeError { .. } => 0x0003,
            ValkyrieErrorKind::RuntimeError { .. } => 0x0004,
            ValkyrieErrorKind::VmError { code, .. } => *code,
            ValkyrieErrorKind::CompileError { .. } => 0x2001,
            ValkyrieErrorKind::NamingViolation { code, .. } => *code,
            ValkyrieErrorKind::Unknown => 0xFFFF,
        }
    }

    /// 返回面向用户的错误类别。
    pub fn category(&self) -> &'static str {
        match self {
            ValkyrieErrorKind::IoError { .. } => "I/O 错误",
            ValkyrieErrorKind::ParseError { .. } => "解析错误",
            ValkyrieErrorKind::SyntaxError { .. } => "语法错误",
            ValkyrieErrorKind::TypeError { .. } => "类型错误",
            ValkyrieErrorKind::RuntimeError { .. } => "运行时错误",
            ValkyrieErrorKind::VmError { .. } => "虚拟机错误",
            ValkyrieErrorKind::CompileError { .. } => "编译错误",
            ValkyrieErrorKind::NamingViolation { .. } => "命名 lint",
            ValkyrieErrorKind::Unknown => "未知错误",
        }
    }

    /// 返回格式化的 `E####` 错误码字符串。
    pub fn formatted_code(&self) -> String {
        format!("E{:04X}", self.code())
    }

    /// 返回结构化附加数据键值对。
    pub fn data(&self) -> Vec<(String, String)> {
        match self {
            ValkyrieErrorKind::IoError { message, path } => {
                let mut data = vec![("message".to_string(), message.clone())];
                if let Some(p) = path {
                    data.push(("path".to_string(), p.clone()));
                }
                data
            }
            ValkyrieErrorKind::ParseError { message } => {
                vec![("message".to_string(), message.clone())]
            }
            ValkyrieErrorKind::SyntaxError { message } => {
                vec![("message".to_string(), message.clone())]
            }
            ValkyrieErrorKind::TypeError { expected, found } => {
                vec![("expected".to_string(), expected.clone()), ("found".to_string(), found.clone())]
            }
            ValkyrieErrorKind::RuntimeError { message } => {
                vec![("message".to_string(), message.clone())]
            }
            ValkyrieErrorKind::VmError { code, key, message } => {
                vec![("code".to_string(), format!("E{:04X}", code)), ("key".to_string(), key.clone()), ("message".to_string(), message.clone())]
            }
            ValkyrieErrorKind::CompileError { message } => {
                vec![("message".to_string(), message.clone())]
            }
            ValkyrieErrorKind::NamingViolation { code, message } => {
                vec![("code".to_string(), format!("E{:04X}", code)), ("message".to_string(), message.clone())]
            }
            ValkyrieErrorKind::Unknown => vec![],
        }
    }
}

/// A diagnostic error for Valkyrie compilation
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ValkyrieError {
    /// 诊断严重级别。
    pub level: ReportKind,
    /// 错误种类。
    pub kind: ValkyrieErrorKind,
    /// 源码标签列表。
    pub labels: Vec<LabeledSpan>,
    /// 可选帮助信息。
    pub help: Option<HelpMessage>,
}

impl ValkyrieError {
    /// 构造运行时错误。
    pub fn runtime_error(message: String) -> Self {
        Self { level: ReportKind::Error, kind: ValkyrieErrorKind::RuntimeError { message }, labels: Vec::new(), help: None }
    }

    /// 构造解析错误。
    pub fn parse_error(message: String) -> Self {
        Self { level: ReportKind::Error, kind: ValkyrieErrorKind::ParseError { message }, labels: Vec::new(), help: None }
    }

    /// 构造语法错误。
    pub fn syntax_error(message: impl Into<String>, _source: &str) -> Self {
        Self { level: ReportKind::Error, kind: ValkyrieErrorKind::SyntaxError { message: message.into() }, labels: Vec::new(), help: None }
    }

    /// 构造类型错误。
    pub fn type_error(expected: String, found: String) -> Self {
        Self { level: ReportKind::Error, kind: ValkyrieErrorKind::TypeError { expected, found }, labels: Vec::new(), help: None }
    }

    /// 构造 I/O 错误。
    pub fn io_error(message: String, path: Option<String>) -> Self {
        Self { level: ReportKind::Error, kind: ValkyrieErrorKind::IoError { message, path }, labels: Vec::new(), help: None }
    }

    /// 构造虚拟机错误。
    pub fn vm_error(code: u32, key: String, message: String) -> Self {
        Self { level: ReportKind::Error, kind: ValkyrieErrorKind::VmError { code, key, message }, labels: Vec::new(), help: None }
    }

    /// 构造编译错误。
    pub fn compile_error(message: String) -> Self {
        Self { level: ReportKind::Error, kind: ValkyrieErrorKind::CompileError { message }, labels: Vec::new(), help: None }
    }

    /// 构造命名 lint 警告。
    pub fn naming_violation(code: u32, name: &str, span: SourceSpan) -> Self {
        let message = format!("Name '{name}' should be snake_case");
        Self {
            level: ReportKind::Warning,
            kind: ValkyrieErrorKind::NamingViolation { code, message },
            labels: vec![LabeledSpan { primary: true, span, key: Some("naming lint".into()), data: Vec::new() }],
            help: None,
        }
    }

    /// 返回错误码。
    pub fn code(&self) -> u32 {
        self.kind.code()
    }
}

impl std::fmt::Display for ValkyrieError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.level, self.kind)?;

        for label in &self.labels {
            if label.primary {
                write!(f, " at {}:{}-{}", label.span.source.version_id, label.span.get_start(), label.span.get_end())?;
            }
        }

        if let Some(ref help) = self.help {
            write!(f, "\n  help: {}", help.key)?;
            for (k, v) in &help.data {
                write!(f, " {}={}", k, v)?;
            }
        }

        Ok(())
    }
}

impl std::error::Error for ValkyrieError {}

impl Diagnostic for ValkyrieError {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(format!("valkyrie::{:04X}", self.code())))
    }

    fn severity(&self) -> Option<Severity> {
        Some(self.level.as_miette_severity())
    }

    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.help.as_ref().map(|help| {
            let mut message = help.key.clone();
            for (key, value) in &help.data {
                message.push(' ');
                message.push_str(key);
                message.push('=');
                message.push_str(value);
            }
            Box::new(message) as Box<dyn std::fmt::Display>
        })
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = MietteLabeledSpan> + '_>> {
        if self.labels.is_empty() {
            return None;
        }

        Some(Box::new(self.labels.iter().map(|label| {
            let start = label.span.get_start();
            let end = label.span.get_end();
            let span = (start as usize, end.saturating_sub(start) as usize);
            let text = if let Some(key) = &label.key {
                Some(key.clone())
            }
            else if label.data.is_empty() {
                None
            }
            else {
                Some(label.data.iter().map(|(key, value)| format!("{key}={value}")).collect::<Vec<_>>().join(", "))
            };

            MietteLabeledSpan::new_with_span(text, span)
        })))
    }
}

impl From<nyar_types::NyarError> for ValkyrieError {
    fn from(e: nyar_types::NyarError) -> Self {
        let code = e.code;
        let key = e.kind.key().to_string();
        let message = format!("{}", e.kind);

        Self::vm_error(code, key, message)
    }
}

/// Result type for Valkyrie operations
pub type Result<T = ()> = std::result::Result<T, ValkyrieError>;
