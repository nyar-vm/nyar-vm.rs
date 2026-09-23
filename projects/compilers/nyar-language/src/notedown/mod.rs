//! Notedown / Markdown 文档 HTML 渲染（代码高亮 + KaTeX）。
//!
//! 代码高亮经 [`crate::notedown::highlight`] 调度：平台契约在 `nyar_analyzer::highlight`，
//! 语言实现在 `nyar_language::{lang}::highlight`。

mod html;
mod highlight;
mod options;

pub use html::{NotedownHtmlResult, render, render_markdown};
pub use highlight::*;
pub use options::NotedownHtmlOptions;
