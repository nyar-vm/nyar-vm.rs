use core::range::Range;
use oak_lsp::types::{LocationRange, LspRange, SelectionRange, SourcePosition};
use oak_valkyrie::ast::Span;

pub fn span_to_range_usize(span: Span) -> Range<usize> {
    Range { start: span.start as usize, end: span.end as usize }
}

pub fn span_to_range(span: Span) -> LocationRange {
    LocationRange { uri: "".into(), range: Range { start: span.start as usize, end: span.end as usize } }
}

pub fn span_to_lsp_range(span: Span) -> LspRange {
    LspRange { start: span.start as usize, end: span.end as usize }
}

pub fn range_to_lsp_range_usize(range: Range<usize>) -> Range<usize> {
    range
}

pub fn span_to_selection_range(span: Span) -> SelectionRange {
    SelectionRange { range: Range { start: span.start as usize, end: span.end as usize }, parent: None }
}

pub fn make_source_position(offset: usize, line: u32, column: u32, length: usize) -> SourcePosition {
    SourcePosition { line, column, offset, length }
}
