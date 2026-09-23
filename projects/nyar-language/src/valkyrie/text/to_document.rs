//! 模型 → [`Document`] 转换契约（**printer** 路径，非源码 fmt）。
//!
//! 源码正规格式化使用 [`FormatSyntax`]。

use nyar_analyzer::format::{Document, FormatOptions};

/// 将已解析**数据模型**转为布局文档（printer / serde）。
pub trait ToDocument {
    /// 按配置建成布局文档（尚未 `render`）。
    fn to_document(&self, options: &FormatOptions) -> Document;
}
