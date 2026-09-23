//! Valkyrie 语言语法高亮插件（多 pass：Lexical + Semantic）。
//!
//! 依赖平台层 [`nyar_analyzer::highlight`]：
//! - [`ValkyrieLexicalHighlighter`]：lexer 快速着色（文档 / 编辑实时）
//! - [`ValkyrieSemanticHighlighter`]：语义高亮（核心类型名 `Option` / `Result` 及其变体）

use std::collections::HashMap;

use nyar_analyzer::highlight::{
    HighlightKind, HighlightRequest, HighlightSpan, Highlighter, HighlighterKind, HighlighterProvider, render_spans_html,
};
use std_data::text::valkyrie::lexer::{Lexer as ValkyrieLexer, TokenKind};

/// 兼容别名：默认指词法高亮器。
pub type ValkyrieHighlighter = ValkyrieLexicalHighlighter;

/// Valkyrie 词法高亮器（快速，仅需源文本）。
#[derive(Debug, Default, Clone, Copy)]
pub struct ValkyrieLexicalHighlighter;

impl ValkyrieLexicalHighlighter {
    /// 构造。
    pub fn new() -> Self {
        Self
    }
}

impl Highlighter for ValkyrieLexicalHighlighter {
    fn language_id(&self) -> &str {
        "valkyrie"
    }

    fn kind(&self) -> HighlighterKind {
        HighlighterKind::Lexical
    }

    fn highlight(&self, request: &HighlightRequest<'_>) -> Vec<HighlightSpan> {
        let Ok(tokens) = ValkyrieLexer::tokenize(request.source)
        else {
            return Vec::new();
        };
        tokens
            .into_iter()
            .filter_map(|token| {
                if matches!(token.kind, TokenKind::Eof) {
                    return None;
                }
                let kind = classify_token_kind(&token.kind);
                (kind != HighlightKind::None).then(|| HighlightSpan::new(kind, token.span.clone()))
            })
            .collect()
    }
}

/// `core::types` 基础类型符号表（可配置，从 `core::types` 事实源派生）。
///
/// 用于 [`ValkyrieSemanticHighlighter`] 对核心类型名做语义着色。
/// 默认表包含 `Option` / `Result` 及其变体 `Some` / `None` / `Fine` / `Fail`，
/// 这些符号名直接对应 `valkyrie.v/projects/core/source/types/Option.v` 与 `Result.v` 的定义。
/// 调用方可通过 [`CoreTypeSymbolTable::register`] 追加自定义符号。
#[derive(Debug, Clone)]
pub struct CoreTypeSymbolTable {
    /// 符号名 → 高亮种类。
    symbols: HashMap<String, HighlightKind>,
}

impl CoreTypeSymbolTable {
    /// 从 `core::types` 事实源派生的默认符号表。
    ///
    /// 包含：
    /// - 类型名 `Option` / `Result` → [`HighlightKind::TypeIdentifier`]
    /// - 变体名 `Some` / `None` / `Fine` / `Fail` → [`HighlightKind::VariantIdentifier`]
    pub fn core_types() -> Self {
        let mut symbols = HashMap::new();
        symbols.insert("Option".to_string(), HighlightKind::TypeIdentifier);
        symbols.insert("Result".to_string(), HighlightKind::TypeIdentifier);
        symbols.insert("Some".to_string(), HighlightKind::VariantIdentifier);
        symbols.insert("None".to_string(), HighlightKind::VariantIdentifier);
        symbols.insert("Fine".to_string(), HighlightKind::VariantIdentifier);
        symbols.insert("Fail".to_string(), HighlightKind::VariantIdentifier);
        Self { symbols }
    }

    /// 追加一个自定义符号映射。
    pub fn register(&mut self, name: impl Into<String>, kind: HighlightKind) {
        self.symbols.insert(name.into(), kind);
    }

    /// 按符号名查询高亮种类。
    pub fn classify(&self, name: &str) -> Option<HighlightKind> {
        self.symbols.get(name).copied()
    }
}

impl Default for CoreTypeSymbolTable {
    fn default() -> Self {
        Self::core_types()
    }
}

/// Valkyrie 语义高亮器。
///
/// 基于 [`CoreTypeSymbolTable`] 对核心类型名（`Option` / `Result` 及其变体）做语义着色。
/// 词法高亮器将这些标识符着色为普通 `Identifier`（或 `Keyword`），语义 pass 将其升级为
/// `TypeIdentifier` / `VariantIdentifier`，由 `merge_spans` 覆盖词法结果。
///
/// 当 `HighlightRequest::analysis` 可 downcast 到具体符号模型时，可扩展为从 name resolution / HIR
/// 派生 span（当前平台层 `AnalysisContext` 仅为 `as_any` 占位，无具体实现，故走核心类型名表）。
#[derive(Debug, Default, Clone)]
pub struct ValkyrieSemanticHighlighter {
    /// 核心类型符号表。
    table: CoreTypeSymbolTable,
}

impl ValkyrieSemanticHighlighter {
    /// 构造，使用默认 `core::types` 符号表。
    pub fn new() -> Self {
        Self { table: CoreTypeSymbolTable::core_types() }
    }

    /// 构造，注入自定义符号表。
    pub fn with_table(table: CoreTypeSymbolTable) -> Self {
        Self { table }
    }

    /// 取内部符号表引用。
    pub fn table(&self) -> &CoreTypeSymbolTable {
        &self.table
    }
}

impl Highlighter for ValkyrieSemanticHighlighter {
    fn language_id(&self) -> &str {
        "valkyrie"
    }

    fn kind(&self) -> HighlighterKind {
        HighlighterKind::Semantic
    }

    fn highlight(&self, request: &HighlightRequest<'_>) -> Vec<HighlightSpan> {
        // 优先方案扩展点：若 request.analysis 可 downcast 到 Valkyrie 符号模型（name resolution / HIR），
        // 应从中获取类型符号 span 并映射到 HighlightKind，使高亮与名字解析共享同一套事实源。
        // 当前平台 AnalysisContext 仅为 as_any 占位，无具体实现，故走核心类型名表方案。
        let _ = request.analysis;
        let Ok(tokens) = ValkyrieLexer::tokenize(request.source)
        else {
            return Vec::new();
        };
        tokens
            .into_iter()
            .filter_map(|token| {
                if token.kind.is_trivia() || matches!(token.kind, TokenKind::Eof) {
                    return None;
                }
                let text = &request.source[token.span.clone()];
                let kind = self.table.classify(text)?;
                Some(HighlightSpan::new(kind, token.span))
            })
            .collect()
    }
}

/// Valkyrie `HighlighterProvider`：同时提供 Lexical 与 Semantic。
#[derive(Debug, Default, Clone, Copy)]
pub struct ValkyrieHighlighterProvider;

impl HighlighterProvider for ValkyrieHighlighterProvider {
    fn language_ids(&self) -> &[&str] {
        &["v", "valkyrie", "vx"]
    }

    fn highlighters(&self) -> Vec<Box<dyn Highlighter>> {
        vec![Box::new(ValkyrieLexicalHighlighter::new()), Box::new(ValkyrieSemanticHighlighter::new())]
    }
}

/// 对 Valkyrie 源码做词法着色为 `hl-*` HTML（文档默认路径）。
pub fn highlight_html(source: &str) -> String {
    let request = HighlightRequest::source_only(source);
    let spans = ValkyrieLexicalHighlighter.highlight(&request);
    render_spans_html(source, &spans)
}

fn classify_token_kind(kind: &TokenKind) -> HighlightKind {
    match kind {
        TokenKind::Keyword(_) => HighlightKind::Keyword,
        TokenKind::StringLiteral => HighlightKind::String,
        TokenKind::IntegerLiteral | TokenKind::FloatLiteral => HighlightKind::Number,
        TokenKind::Identifier => HighlightKind::Identifier,
        TokenKind::LParen
        | TokenKind::RParen
        | TokenKind::LBrace
        | TokenKind::RBrace
        | TokenKind::LBracket
        | TokenKind::RBracket
        | TokenKind::LOffsetBracket
        | TokenKind::ROffsetBracket
        | TokenKind::LAngle
        | TokenKind::RAngle
        | TokenKind::Comma
        | TokenKind::Semicolon
        | TokenKind::Colon
        | TokenKind::Dot
        | TokenKind::Apostrophe => HighlightKind::Punctuation,
        TokenKind::Eof => HighlightKind::None,
        _ => HighlightKind::Operator,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nyar_analyzer::highlight::{AnalysisContext, HighlighterKind, merge_spans};

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
}
