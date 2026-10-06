use std::fmt::{Display, Formatter};

use miette::{Diagnostic, Severity};
use serde::{Deserialize, Serialize, de, ser};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VonParseError {
    message: String,
    position: usize,
}

impl VonParseError {
    pub(crate) fn new(position: usize, message: impl Into<String>) -> Self {
        Self { message: message.into(), position }
    }
}

impl Display for VonParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "VON parse error at {}: {}", self.position, self.message)
    }
}

impl std::error::Error for VonParseError {}

impl Diagnostic for VonParseError {
    fn code<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        Some(Box::new("von::parse"))
    }

    fn severity(&self) -> Option<Severity> {
        Some(Severity::Error)
    }

    fn help<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        Some(Box::new("请检查 `VON` 文本的键值分隔符、括号和引号是否成对出现"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VonSerdeError {
    message: String,
}

impl VonSerdeError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl Display for VonSerdeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.message, f)
    }
}

impl std::error::Error for VonSerdeError {}

impl Diagnostic for VonSerdeError {
    fn code<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        Some(Box::new("von::serde"))
    }

    fn severity(&self) -> Option<Severity> {
        Some(Severity::Error)
    }
}

impl ser::Error for VonSerdeError {
    fn custom<T>(msg: T) -> Self
    where
        T: Display,
    {
        Self::new(msg.to_string())
    }
}

impl de::Error for VonSerdeError {
    fn custom<T>(msg: T) -> Self
    where
        T: Display,
    {
        Self::new(msg.to_string())
    }
}

#[derive(Debug)]
pub enum VonError {
    Parse(VonParseError),
    Serialize(VonSerdeError),
    Deserialize(VonSerdeError),
}

impl Display for VonError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => Display::fmt(error, f),
            Self::Serialize(error) => write!(f, "VON serialize error: {}", error),
            Self::Deserialize(error) => write!(f, "VON deserialize error: {}", error),
        }
    }
}

impl std::error::Error for VonError {}

impl Diagnostic for VonError {
    fn code<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        Some(Box::new(match self {
            VonError::Parse(_) => "von::parse",
            VonError::Serialize(_) => "von::serialize",
            VonError::Deserialize(_) => "von::deserialize",
        }))
    }

    fn severity(&self) -> Option<Severity> {
        Some(Severity::Error)
    }

    fn diagnostic_source(&self) -> Option<&dyn Diagnostic> {
        match self {
            VonError::Parse(error) => Some(error),
            VonError::Serialize(error) => Some(error),
            VonError::Deserialize(error) => Some(error),
        }
    }
}

impl From<VonParseError> for VonError {
    fn from(value: VonParseError) -> Self {
        Self::Parse(value)
    }
}
