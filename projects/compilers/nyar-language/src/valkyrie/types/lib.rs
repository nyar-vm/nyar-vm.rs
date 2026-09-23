#![doc = include_str!("readme.md")]
#![warn(missing_docs)]
#![allow(missing_docs)]

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
    Error,
    Warning,
    Note,
    Help,
}

impl ReportKind {
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
    pub span: SourceSpan,
    pub primary: bool,
    pub key: Option<String>,
    pub data: Vec<(String, String)>,
}

/// A help message for an error
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HelpMessage {
    pub key: String,
    pub data: Vec<(String, String)>,
}

/// The kind of error that occurred
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ValkyrieErrorKind {
    /// I/O error (file not found, permission denied, etc.)
    IoError { message: String, path: Option<String> },
    /// Parse error (failed to parse source code)
    ParseError { message: String },
    /// Syntax error (invalid syntax)
    SyntaxError { message: String },
    /// Type error (type mismatch)
    TypeError { expected: String, found: String },
    /// Runtime error (general runtime failure)
    RuntimeError { message: String },
    /// VM error (stack underflow, index out of bounds, etc.)
    VmError { code: u32, key: String, message: String },
    /// Compile error (AOT compilation failure)
    CompileError { message: String },
    /// Naming convention violation (e.g. identifier must be snake_case).
    NamingViolation { code: u32, message: String },
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

    pub fn formatted_code(&self) -> String {
        format!("E{:04X}", self.code())
    }

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
    pub level: ReportKind,
    pub kind: ValkyrieErrorKind,
    pub labels: Vec<LabeledSpan>,
    pub help: Option<HelpMessage>,
}

impl ValkyrieError {
    pub fn runtime_error(message: String) -> Self {
        Self { level: ReportKind::Error, kind: ValkyrieErrorKind::RuntimeError { message }, labels: Vec::new(), help: None }
    }

    pub fn parse_error(message: String) -> Self {
        Self { level: ReportKind::Error, kind: ValkyrieErrorKind::ParseError { message }, labels: Vec::new(), help: None }
    }

    pub fn syntax_error(message: impl Into<String>, _source: &str) -> Self {
        Self { level: ReportKind::Error, kind: ValkyrieErrorKind::SyntaxError { message: message.into() }, labels: Vec::new(), help: None }
    }

    pub fn type_error(expected: String, found: String) -> Self {
        Self { level: ReportKind::Error, kind: ValkyrieErrorKind::TypeError { expected, found }, labels: Vec::new(), help: None }
    }

    pub fn io_error(message: String, path: Option<String>) -> Self {
        Self { level: ReportKind::Error, kind: ValkyrieErrorKind::IoError { message, path }, labels: Vec::new(), help: None }
    }

    pub fn vm_error(code: u32, key: String, message: String) -> Self {
        Self { level: ReportKind::Error, kind: ValkyrieErrorKind::VmError { code, key, message }, labels: Vec::new(), help: None }
    }

    pub fn compile_error(message: String) -> Self {
        Self { level: ReportKind::Error, kind: ValkyrieErrorKind::CompileError { message }, labels: Vec::new(), help: None }
    }

    pub fn naming_violation(code: u32, name: &str, span: SourceSpan) -> Self {
        let message = format!("Name '{name}' should be snake_case");
        Self {
            level: ReportKind::Warning,
            kind: ValkyrieErrorKind::NamingViolation { code, message },
            labels: vec![LabeledSpan { primary: true, span, key: Some("naming lint".into()), data: Vec::new() }],
            help: None,
        }
    }

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
