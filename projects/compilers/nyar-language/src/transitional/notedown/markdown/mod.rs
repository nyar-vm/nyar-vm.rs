#![doc = include_str!("readme.md")]

mod bridge;
mod preprocess;

pub use bridge::{from_notedown, to_notedown};
pub use preprocess::preprocess_markdown;

use super::notedown::{NotedownDocument, formatter};

/// Markdown 文档（统一使用 Notedown IR）。
pub type MarkdownDocument = NotedownDocument;

/// 将 Markdown 源码解析为 Notedown IR。
///
/// Valkyrie 文档源文件通常为 Markdown 变体；经预处理后由 Notedown 解析器处理。
pub fn parse(source: &str) -> MarkdownDocument {
    let preprocessed = preprocess_markdown(source);
    NotedownDocument::parse(&preprocessed)
}

/// 将 Markdown 文档格式化为 Markdown 文本。
pub fn format(document: &MarkdownDocument) -> String {
    from_notedown(document)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_highlight_extension() {
        let doc = parse("==highlighted==");
        assert!(!doc.blocks.is_empty());
    }

    #[test]
    fn round_trip_heading() {
        let source = "# Title\n\nBody";
        let doc = parse(source);
        let formatted = format(&doc);
        assert!(formatted.contains("# Title"));
    }
}
