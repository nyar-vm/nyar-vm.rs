use nyar_types::{FormatError, VmError};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, ScriptError>;

#[derive(Debug)]
pub struct ScriptError {
    pub kind: Box<ScriptErrorKind>,
}

#[derive(Debug)]
pub enum ScriptErrorKind {
    Io {
        error: std::io::Error,
        path: Option<PathBuf>,
    },
    NyarFormat {
        error: FormatError,
    },
    Vm {
        error: VmError,
    },
    Toml {
        error: toml::ser::Error,
    },
    Json {
        error: serde_json::Error,
    },
    Compile {
        message: String,
    },
    Other {
        message: String,
    },
}

impl ScriptError {
    pub fn io(error: std::io::Error, path: impl Into<Option<PathBuf>>) -> Self {
        Self {
            kind: Box::new(ScriptErrorKind::Io {
                error,
                path: path.into(),
            }),
        }
    }

    pub fn nyar(error: FormatError) -> Self {
        Self {
            kind: Box::new(ScriptErrorKind::NyarFormat { error }),
        }
    }

    pub fn vm(error: VmError) -> Self {
        Self {
            kind: Box::new(ScriptErrorKind::Vm { error }),
        }
    }

    pub fn toml(error: toml::ser::Error) -> Self {
        Self {
            kind: Box::new(ScriptErrorKind::Toml { error }),
        }
    }

    pub fn json(error: serde_json::Error) -> Self {
        Self {
            kind: Box::new(ScriptErrorKind::Json { error }),
        }
    }

    pub fn compile(message: impl Into<String>) -> Self {
        Self {
            kind: Box::new(ScriptErrorKind::Compile {
                message: message.into(),
            }),
        }
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self {
            kind: Box::new(ScriptErrorKind::Other {
                message: message.into(),
            }),
        }
    }
}

impl Display for ScriptError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.kind.as_ref() {
            ScriptErrorKind::Io { error, path } => {
                if let Some(path) = path {
                    write!(f, "io error at {:?}: {}", path, error)
                } else {
                    write!(f, "io error: {}", error)
                }
            }
            ScriptErrorKind::NyarFormat { error } => write!(f, "nyar format error: {:?}", error),
            ScriptErrorKind::Vm { error } => write!(f, "vm error: {:?}", error),
            ScriptErrorKind::Toml { error } => write!(f, "toml error: {}", error),
            ScriptErrorKind::Json { error } => write!(f, "json error: {}", error),
            ScriptErrorKind::Compile { message } => write!(f, "compile error: {}", message),
            ScriptErrorKind::Other { message } => write!(f, "error: {}", message),
        }
    }
}

impl Error for ScriptError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self.kind.as_ref() {
            ScriptErrorKind::Io { error, .. } => Some(error),
            ScriptErrorKind::Toml { error } => Some(error),
            ScriptErrorKind::Json { error } => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ScriptError {
    fn from(error: std::io::Error) -> Self {
        Self::io(error, None)
    }
}

impl From<FormatError> for ScriptError {
    fn from(error: FormatError) -> Self {
        Self::nyar(error)
    }
}

impl From<VmError> for ScriptError {
    fn from(error: VmError) -> Self {
        Self::vm(error)
    }
}

impl From<toml::ser::Error> for ScriptError {
    fn from(error: toml::ser::Error) -> Self {
        Self::toml(error)
    }
}

impl From<serde_json::Error> for ScriptError {
    fn from(error: serde_json::Error) -> Self {
        Self::json(error)
    }
}

impl From<String> for ScriptError {
    fn from(message: String) -> Self {
        Self::other(message)
    }
}

impl From<&str> for ScriptError {
    fn from(message: &str) -> Self {
        Self::other(message)
    }
}
