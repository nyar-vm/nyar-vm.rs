//! WIT 文本文档模型（仅用于格式化与 printer 注册表，不承担解析职责）。

use std::fmt::{Display, Formatter};

/// `WIT` 解析错误（保留给未来 Oak 前端适配层）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WitError {
    /// 包声明缺失或格式不正确。
    InvalidPackage,
    /// 接口块格式不正确。
    InvalidInterface(String),
}

impl Display for WitError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPackage => write!(f, "无效的 `WIT` package 声明"),
            Self::InvalidInterface(message) => write!(f, "无效的 `WIT` interface：{message}"),
        }
    }
}

impl std::error::Error for WitError {}

/// `WIT` 接口项。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WitInterface {
    /// 接口名。
    pub name: String,
    /// 函数签名列表。
    pub functions: Vec<String>,
}

/// `WIT` 接口包模型。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WitPackage {
    /// 包名。
    pub package_name: String,
    /// 接口列表。
    pub interfaces: Vec<WitInterface>,
}

impl WitPackage {
    /// 创建一个新的 `WIT` 接口包。
    pub fn new(package_name: impl Into<String>) -> Self {
        Self { package_name: package_name.into(), interfaces: Vec::new() }
    }

    /// 追加一个接口定义。
    pub fn push_interface(&mut self, name: impl Into<String>, functions: Vec<String>) {
        self.interfaces.push(WitInterface { name: name.into(), functions });
    }
}
