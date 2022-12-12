//! `ValkyrieSemanticHighlighter` 对 `core::types` 基础类型（`Option` / `Result` 及其变体）
//! 的语义高亮回归测试。
//!
//! 覆盖 `HighlighterRegistry::highlight_merged` 端到端路径：词法 pass 产出基础着色，
//! 语义 pass 将核心类型名升级为 `TypeIdentifier` / `VariantIdentifier` 并覆盖词法结果。

use nyar_analyzer::highlight::{HighlightKind, HighlightRequest, HighlighterRegistry};
use nyar_language::valkyrie::highlight::ValkyrieHighlighterProvider;

fn merged_spans(source: &str) -> Vec<nyar_analyzer::highlight::HighlightSpan> {
    let mut registry = HighlighterRegistry::new();
    registry.register_provider(&ValkyrieHighlighterProvider);
    let request = HighlightRequest::source_only(source);
    registry.highlight_merged("valkyrie", &request).expect("highlighter should be registered")
}

fn assert_span_kind(source: &str, expected_text: &str, expected_kind: HighlightKind) {
    let spans = merged_spans(source);
    let span = spans
        .iter()
        .find(|s| &source[s.range.clone()] == expected_text)
        .unwrap_or_else(|| panic!("no span matched text {:?} in source {:?}; spans: {:?}", expected_text, source, spans));
    assert_eq!(span.kind, expected_kind, "text {:?} in source {:?}", expected_text, source);
}

#[test]
fn option_type_highlighted_as_type_identifier() {
    assert_span_kind("let x: Option<i32> = None", "Option", HighlightKind::TypeIdentifier);
}

#[test]
fn result_type_highlighted_as_type_identifier() {
    assert_span_kind("let r: Result<i32, String> = Fine(1)", "Result", HighlightKind::TypeIdentifier);
}

#[test]
fn some_variant_highlighted_as_variant_identifier() {
    assert_span_kind("Some(value)", "Some", HighlightKind::VariantIdentifier);
}

#[test]
fn none_variant_highlighted_as_variant_identifier() {
    assert_span_kind("let x: Option<i32> = None", "None", HighlightKind::VariantIdentifier);
}

#[test]
fn fine_variant_highlighted_as_variant_identifier() {
    assert_span_kind("Fine(value)", "Fine", HighlightKind::VariantIdentifier);
}

#[test]
fn fail_variant_highlighted_as_variant_identifier() {
    assert_span_kind("Fail(error)", "Fail", HighlightKind::VariantIdentifier);
}

#[test]
fn merged_semantic_overrides_lexical_keyword_for_none() {
    let source = "let x: Option<i32> = None";
    let spans = merged_spans(source);
    let none_span = spans.iter().find(|s| &source[s.range.clone()] == "None").expect("None span should exist");
    assert_eq!(none_span.kind, HighlightKind::VariantIdentifier);
    let option_span = spans.iter().find(|s| &source[s.range.clone()] == "Option").expect("Option span should exist");
    assert_eq!(option_span.kind, HighlightKind::TypeIdentifier);
}

#[test]
fn semantic_pass_does_not_touch_unrelated_identifiers() {
    let source = "let value = Some(inner)";
    let spans = merged_spans(source);
    let value_span = spans.iter().find(|s| &source[s.range.clone()] == "value").expect("value span should exist");
    assert_eq!(value_span.kind, HighlightKind::Identifier);
    let inner_span = spans.iter().find(|s| &source[s.range.clone()] == "inner").expect("inner span should exist");
    assert_eq!(inner_span.kind, HighlightKind::Identifier);
    let some_span = spans.iter().find(|s| &source[s.range.clone()] == "Some").expect("Some span should exist");
    assert_eq!(some_span.kind, HighlightKind::VariantIdentifier);
}
