use nyar_analyzer::highlight::{
    AnalysisContext, HighlightKind, HighlightRequest, HighlightSpan, Highlighter, HighlighterKind, HighlighterProvider, merge_spans,
};
use rusty_valkyrie::{
    highlight::{CoreTypeSymbolTable, ValkyrieHighlighterProvider, ValkyrieLexicalHighlighter, ValkyrieSemanticHighlighter},
    *,
};

#[test]
fn lexical_highlights_keyword() {
    let request = HighlightRequest::source_only("let x = 1");
    let spans = ValkyrieLexicalHighlighter.highlight(&request);
    assert!(spans.iter().any(|s| s.kind == HighlightKind::Keyword));
    assert_eq!(ValkyrieLexicalHighlighter.kind(), HighlighterKind::Lexical);
}

#[test]
fn semantic_empty_without_analysis() {
    let request = HighlightRequest::source_only("let x = 1");
    let spans = ValkyrieSemanticHighlighter::new().highlight(&request);
    assert!(spans.is_empty());
}

#[test]
fn semantic_empty_with_stub_analysis() {
    struct StubCtx;
    impl AnalysisContext for StubCtx {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }
    let ctx = StubCtx;
    let request = HighlightRequest::with_analysis("let x = 1", &ctx);
    let spans = ValkyrieSemanticHighlighter::new().highlight(&request);
    assert!(spans.is_empty());
}

#[test]
fn provider_registers_both_kinds() {
    let items = ValkyrieHighlighterProvider.highlighters();
    assert_eq!(items.len(), 2);
    assert!(items.iter().any(|h| h.kind() == HighlighterKind::Lexical));
    assert!(items.iter().any(|h| h.kind() == HighlighterKind::Semantic));
}

fn find_span<'a>(spans: &'a [HighlightSpan], source: &str, kind: HighlightKind, expected_text: &str) -> Option<&'a HighlightSpan> {
    spans.iter().find(|s| s.kind == kind && &source[s.range.clone()] == expected_text)
}

#[test]
fn semantic_highlights_option_as_type_identifier() {
    let source = "let x: Option<i32> = None";
    let request = HighlightRequest::source_only(source);
    let spans = ValkyrieSemanticHighlighter::new().highlight(&request);
    assert!(find_span(&spans, source, HighlightKind::TypeIdentifier, "Option").is_some());
}

#[test]
fn semantic_highlights_result_as_type_identifier() {
    let source = "let r: Result<i32, String> = Fine(1)";
    let request = HighlightRequest::source_only(source);
    let spans = ValkyrieSemanticHighlighter::new().highlight(&request);
    assert!(find_span(&spans, source, HighlightKind::TypeIdentifier, "Result").is_some());
}

#[test]
fn semantic_highlights_some_as_variant_identifier() {
    let source = "Some(value)";
    let request = HighlightRequest::source_only(source);
    let spans = ValkyrieSemanticHighlighter::new().highlight(&request);
    assert!(find_span(&spans, source, HighlightKind::VariantIdentifier, "Some").is_some());
}

#[test]
fn semantic_highlights_none_as_variant_identifier() {
    let source = "let x: Option<i32> = None";
    let request = HighlightRequest::source_only(source);
    let spans = ValkyrieSemanticHighlighter::new().highlight(&request);
    assert!(find_span(&spans, source, HighlightKind::VariantIdentifier, "None").is_some());
}

#[test]
fn semantic_highlights_fine_as_variant_identifier() {
    let source = "Fine(value)";
    let request = HighlightRequest::source_only(source);
    let spans = ValkyrieSemanticHighlighter::new().highlight(&request);
    assert!(find_span(&spans, source, HighlightKind::VariantIdentifier, "Fine").is_some());
}

#[test]
fn semantic_highlights_fail_as_variant_identifier() {
    let source = "Fail(error)";
    let request = HighlightRequest::source_only(source);
    let spans = ValkyrieSemanticHighlighter::new().highlight(&request);
    assert!(find_span(&spans, source, HighlightKind::VariantIdentifier, "Fail").is_some());
}

#[test]
fn semantic_overrides_lexical_for_core_types() {
    let source = "let x: Option<i32> = None";
    let request = HighlightRequest::source_only(source);
    let lexical = ValkyrieLexicalHighlighter.highlight(&request);
    let semantic = ValkyrieSemanticHighlighter::new().highlight(&request);
    let merged = merge_spans(lexical, semantic);
    let none_span = merged.iter().find(|s| &source[s.range.clone()] == "None").expect("None span should exist");
    assert_eq!(none_span.kind, HighlightKind::VariantIdentifier);
    let option_span = merged.iter().find(|s| &source[s.range.clone()] == "Option").expect("Option span should exist");
    assert_eq!(option_span.kind, HighlightKind::TypeIdentifier);
}

#[test]
fn core_type_table_is_configurable() {
    let mut table = CoreTypeSymbolTable::core_types();
    table.register("JsonValue", HighlightKind::TypeIdentifier);
    assert_eq!(table.classify("JsonValue"), Some(HighlightKind::TypeIdentifier));
    assert_eq!(table.classify("Option"), Some(HighlightKind::TypeIdentifier));
    assert_eq!(table.classify("Some"), Some(HighlightKind::VariantIdentifier));
    assert_eq!(table.classify("missing"), None);
}
