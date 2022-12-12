use std::{fmt::Display, sync::Arc};

/// 一个合法标识符
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Identifier {
    text: Arc<str>,
}

impl Identifier {
    /// 创建一个标识符。
    pub fn new(s: &str) -> Self {
        Self { text: Arc::from(s) }
    }

    /// 返回标识符文本。
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

/// 一个已经完成绑定的完全限定名称。
///
/// `QualifiedName` 表示名称查找、`using`、`import`、`prelude`
/// 展开以及歧义消解都已经结束之后得到的稳定结果。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct QualifiedName {
    parts: Vec<Identifier>,
}

/// 一个尚未完成绑定的名称路径，需要结合 `using`、`import`、`prelude` 等机制继续解析。
///
/// 即便它与 `QualifiedName` 拥有相同的物理表示，也仍然是完全不同的语义对象，
/// 不能把“待解析路径”当成“已经完成绑定的限定名”使用。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NamePath {
    parts: Vec<Identifier>,
}

/// 由 `module`、`namespace` 等声明语句引入的命名空间。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NameSpace {
    parts: Vec<Identifier>,
}

impl Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.text)
    }
}

impl Display for QualifiedName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let parts: Vec<&str> = self.parts.iter().map(|i| i.as_str()).collect();
        write!(f, "{}", parts.join("::"))
    }
}

impl Display for NamePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let parts: Vec<&str> = self.parts.iter().map(|i| i.as_str()).collect();
        write!(f, "{}", parts.join("."))
    }
}

impl NamePath {
    /// 从路径片段创建待解析名称路径。
    pub fn new(parts: Vec<Identifier>) -> Self {
        Self { parts }
    }

    /// 在当前路径末尾追加一个片段。
    pub fn append(&mut self, part: impl Into<Identifier>) {
        self.parts.push(part.into());
    }

    /// 返回路径最后一个片段；若路径为空，则返回空标识符。
    pub fn name(&self) -> Identifier {
        self.parts.last().cloned().unwrap_or_default()
    }

    /// 返回当前路径所属的命名空间。
    pub fn namespace(&self) -> NameSpace {
        NameSpace { parts: self.parts[0..=self.parts.len() - 1].to_vec() }
    }

    /// 返回路径的各级片段。
    pub fn parts(&self) -> &[Identifier] {
        self.parts.as_ref()
    }

    /// 返回是否为空路径。
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }
}

impl QualifiedName {
    /// 从已解析的标识符片段创建限定名。
    pub fn new(parts: Vec<Identifier>) -> Self {
        Self { parts }
    }

    /// 返回限定名的各级片段。
    pub fn parts(&self) -> &[Identifier] {
        self.parts.as_ref()
    }

    /// 返回是否为空名称。
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }
}

/// 前端无关的稳定逻辑符号标识。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SymbolIdentity {
    /// 所属模块的限定名。
    pub module: QualifiedName,
    /// 模块内符号的限定名。
    pub symbol: QualifiedName,
}

impl SymbolIdentity {
    /// 创建一个逻辑符号标识。
    pub fn new(module: QualifiedName, symbol: QualifiedName) -> Self {
        Self { module, symbol }
    }
}
