use std::fmt::Display;

use crate::{Identifier, QualifiedName};

/// 前端与后端之间共享的外部导入链接描述。
///
/// 它只表达“链接到哪里”，不承载目标专用容器细节，
/// 例如 `MSIL` 的 `owner`、`JVM` 的常量池索引等都不应出现在这里。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ExternalImportLink {
    /// 互操作边界名称，例如 `host`。
    pub boundary: Identifier,
    /// 目标家族标签，例如 `clr`、`jvm`、`wasi`。
    pub platform_tag: Option<Identifier>,
    /// 按声明顺序保留的外部定位片段。
    ///
    /// 对于 `CLR`，它通常对应 `assembly`、`type`、`method`；
    /// 对于其他目标，也可以映射为模块名、导出名等稳定字符串片段。
    pub locator_segments: Vec<String>,
}

/// 外部调用允许携带的最小实参形状。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ExternalCallArgument {
    /// 一个稳定保留的字符串字面量。
    StringLiteral(String),
}

/// 已经完成绑定的外部调用边。
///
/// 它描述“哪个稳定操作调用了哪个外部绑定符号，以及调用时携带了哪些最小实参”。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ExternalCallEdge {
    /// 发起调用的稳定操作。
    pub caller: QualifiedName,
    /// 被调用的稳定符号。
    pub callee_symbol: QualifiedName,
    /// 调用时携带的最小实参。
    pub arguments: Vec<ExternalCallArgument>,
}

/// 同一语义片段内的内部调用边。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct InternalCallEdge {
    /// 发起调用的稳定操作。
    pub caller: QualifiedName,
    /// 被调用的片段内稳定符号。
    pub callee_symbol: QualifiedName,
}

impl ExternalImportLink {
    /// 创建一条稳定的外部导入链接。
    pub fn new(boundary: Identifier, platform_tag: Option<Identifier>, locator_segments: Vec<String>) -> Self {
        Self { boundary, platform_tag, locator_segments }
    }

    /// 创建一条宿主互操作导入链接。
    pub fn host(platform_tag: Option<Identifier>, locator_segments: Vec<String>) -> Self {
        Self::new(Identifier::new("host"), platform_tag, locator_segments)
    }

    /// 返回当前链接是否指向指定边界。
    pub fn matches_boundary(&self, boundary: &str) -> bool {
        self.boundary.as_str() == boundary
    }

    /// 返回当前链接是否标记到指定平台。
    pub fn matches_platform_tag(&self, platform_tag: &str) -> bool {
        self.platform_tag.as_ref().is_some_and(|current| current.as_str() == platform_tag)
    }

    /// 返回当前链接是否指向指定宿主平台。
    pub fn matches_host_platform(&self, platform_tag: &str) -> bool {
        self.matches_boundary("host") && self.matches_platform_tag(platform_tag)
    }

    /// 返回外部定位片段。
    pub fn locator_segments(&self) -> &[String] {
        &self.locator_segments
    }
}

impl ExternalCallEdge {
    /// 创建一条稳定的外部调用边。
    pub fn new(caller: QualifiedName, callee_symbol: QualifiedName, arguments: Vec<ExternalCallArgument>) -> Self {
        Self { caller, callee_symbol, arguments }
    }
}

impl InternalCallEdge {
    /// 创建一条稳定的内部调用边。
    pub fn new(caller: QualifiedName, callee_symbol: QualifiedName) -> Self {
        Self { caller, callee_symbol }
    }
}

impl Display for ExternalImportLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.boundary)?;
        if let Some(platform_tag) = &self.platform_tag {
            write!(f, "[{}]", platform_tag)?;
        }
        write!(f, "(")?;
        for (index, segment) in self.locator_segments.iter().enumerate() {
            if index > 0 {
                write!(f, ", ")?;
            }
            write!(f, "\"{}\"", segment)?;
        }
        write!(f, ")")
    }
}

#[cfg(test)]
mod tests {
    use super::ExternalImportLink;
    use crate::Identifier;

    #[test]
    fn builds_host_link_for_clr_console_write_line() {
        let link = ExternalImportLink::host(
            Some(Identifier::new("clr")),
            vec!["mscorlib".to_string(), "System.Console".to_string(), "WriteLine".to_string()],
        );

        assert!(link.matches_boundary("host"));
        assert!(link.matches_platform_tag("clr"));
        assert!(link.matches_host_platform("clr"));
        assert_eq!(link.locator_segments(), &["mscorlib".to_string(), "System.Console".to_string(), "WriteLine".to_string(),]);
    }

    #[test]
    fn renders_stable_debuggable_display_shape() {
        let link = ExternalImportLink::host(
            Some(Identifier::new("wasi")),
            vec!["wasi:io/streams".to_string(), "blocking-write-and-flush".to_string()],
        );

        assert_eq!(link.to_string(), "host[wasi](\"wasi:io/streams\", \"blocking-write-and-flush\")");
    }
}
