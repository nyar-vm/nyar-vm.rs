//! CST → [`Document`] 转换契约（**源码正规格式化**路径）。

use nyar_analyzer::format::{Document, FormatOptions};

/// 将 CST 转为布局文档（保留 trivia 槽位）。
pub trait FormatSyntax {
    /// 按配置建成布局文档（尚未 `render`）。
    fn format_document(&self, options: &FormatOptions) -> Document;
}
