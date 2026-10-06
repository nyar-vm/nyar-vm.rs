use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VonValue {
    Null,
    Bool(bool),
    Number(i64),
    String(String),
    Array(Vec<VonValue>),
    Object(BTreeMap<String, VonValue>),
}

impl VonValue {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value.as_str()),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[VonValue]> {
        match self {
            Self::Array(values) => Some(values.as_slice()),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&BTreeMap<String, VonValue>> {
        match self {
            Self::Object(values) => Some(values),
            _ => None,
        }
    }
}
