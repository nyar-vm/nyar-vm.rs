//! Runtime value model for legacy evaluators.

use std::{
    collections::HashMap,
    fmt::{Display, Formatter},
    sync::Arc,
};

/// Legacy VM runtime value.
#[derive(Clone)]
pub enum LegacyValue {
    /// Unit / null.
    Null,
    /// Boolean.
    Bool(bool),
    /// Signed integer.
    Int(i64),
    /// Floating point.
    Float(f64),
    /// UTF-8 string.
    String(String),
    /// Loop break sentinel.
    Break,
    /// Loop continue sentinel.
    Continue,
    /// Function return sentinel.
    Return(Box<LegacyValue>),
    /// Builtin callable.
    Builtin {
        /// Display name.
        name: String,
        /// Implementation.
        func: Arc<dyn Fn(&[LegacyValue]) -> LegacyValue + Send + Sync>,
    },
    /// Lambda closure.
    Lambda {
        /// Parameter names.
        params: Vec<String>,
        /// Body expression.
        body: Box<LegacyValue>,
        /// Captured environment.
        env: HashMap<String, LegacyValue>,
    },
}

impl std::fmt::Debug for LegacyValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Null => write!(f, "Null"),
            Self::Bool(value) => write!(f, "Bool({value})"),
            Self::Int(value) => write!(f, "Int({value})"),
            Self::Float(value) => write!(f, "Float({value})"),
            Self::String(value) => write!(f, "String({value:?})"),
            Self::Break => write!(f, "Break"),
            Self::Continue => write!(f, "Continue"),
            Self::Return(value) => write!(f, "Return({value:?})"),
            Self::Builtin { name, .. } => write!(f, "Builtin({name})"),
            Self::Lambda { params, .. } => write!(f, "Lambda({params:?})"),
        }
    }
}

impl PartialEq for LegacyValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) => true,
            (Self::Bool(left), Self::Bool(right)) => left == right,
            (Self::Int(left), Self::Int(right)) => left == right,
            (Self::Float(left), Self::Float(right)) => (left - right).abs() < f64::EPSILON,
            (Self::String(left), Self::String(right)) => left == right,
            (Self::Break, Self::Break) | (Self::Continue, Self::Continue) => true,
            (Self::Return(left), Self::Return(right)) => left == right,
            (Self::Builtin { name: left, .. }, Self::Builtin { name: right, .. }) => left == right,
            _ => false,
        }
    }
}

impl Display for LegacyValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Null => write!(f, "null"),
            Self::Bool(value) => write!(f, "{value}"),
            Self::Int(value) => write!(f, "{value}"),
            Self::Float(value) => write!(f, "{value}"),
            Self::String(value) => write!(f, "{value}"),
            Self::Break => write!(f, "<break>"),
            Self::Continue => write!(f, "<continue>"),
            Self::Return(value) => write!(f, "{value}"),
            Self::Builtin { name, .. } => write!(f, "<builtin {name}>"),
            Self::Lambda { .. } => write!(f, "<lambda>"),
        }
    }
}

impl LegacyValue {
    /// Coerce to `i64`.
    pub fn to_i64(&self) -> i64 {
        match self {
            Self::Int(value) => *value,
            Self::Float(value) => *value as i64,
            Self::Bool(value) => i64::from(*value),
            Self::String(value) => value.parse().unwrap_or(0),
            _ => 0,
        }
    }

    /// Coerce to `f64`.
    pub fn to_f64(&self) -> f64 {
        match self {
            Self::Float(value) => *value,
            Self::Int(value) => *value as f64,
            Self::String(value) => value.parse().unwrap_or(0.0),
            _ => 0.0,
        }
    }

    /// Coerce to `bool`.
    pub fn to_bool(&self) -> bool {
        match self {
            Self::Bool(value) => *value,
            Self::Null => false,
            Self::Int(value) => *value != 0,
            Self::Float(value) => *value != 0.0,
            Self::String(value) => !value.is_empty(),
            _ => true,
        }
    }

    /// Coerce to `String`.
    pub fn to_string_value(&self) -> String {
        match self {
            Self::String(value) => value.clone(),
            Self::Null => "null".to_string(),
            Self::Bool(value) => value.to_string(),
            Self::Int(value) => value.to_string(),
            Self::Float(value) => value.to_string(),
            other => other.to_string(),
        }
    }
}
