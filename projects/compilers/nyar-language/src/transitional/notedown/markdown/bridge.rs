//! Markdown ↔ Notedown 桥接（对齐 C# `MarkdownLanguage.to_notedown` / `from_notedown`）。

use super::super::notedown::{NotedownDocument, formatter};

use super::MarkdownDocument;

/// 将 Markdown AST（Notedown IR）转为 Notedown 文档（恒等映射）。
pub fn to_notedown(document: &MarkdownDocument) -> NotedownDocument {
    document.clone()
}

/// 将 Notedown IR 格式化为 Markdown 文本。
pub fn from_notedown(document: &NotedownDocument) -> String {
    formatter::format(document)
}
