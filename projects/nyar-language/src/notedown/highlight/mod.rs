//! 文档/展示用高亮调度：按语言 id + pass 选择 language plugin。
//!
//! - **自研语言**：经 `HighlighterRegistry` 调度 `nyar_language::{lang}::highlight`。
//! - **默认 pass**：`Lexical`（SSG / 片段无工程分析）。
//! - **外部语言**：`foreign` 回退（syntect TextMate），不污染平台契约层。

mod foreign;

use std::sync::OnceLock;

use nyar_analyzer::highlight::{HighlightRequest, HighlighterKind, HighlighterRegistry, escape_html, render_spans_html};

use crate::valkyrie::highlight::ValkyrieHighlighterProvider;

fn registry() -> &'static HighlighterRegistry {
    static REGISTRY: OnceLock<HighlighterRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut reg = HighlighterRegistry::new();
        reg.register_provider(&ValkyrieHighlighterProvider);
        reg
    })
}

/// 按语言标识对代码块着色为 HTML（默认 **Lexical**）。
pub fn highlight_code_block(language: &str, code: &str) -> String {
    highlight_code_block_with(language, code, HighlighterKind::Lexical)
}

/// 按指定 pass 着色；无对应插件时尝试 foreign，再否则纯转义。
pub fn highlight_code_block_with(language: &str, code: &str, kind: HighlighterKind) -> String {
    let lang = language.trim().to_ascii_lowercase();
    let request = HighlightRequest::source_only(code);
    if let Some(highlighter) = registry().resolve(&lang, kind, false) {
        let spans = highlighter.highlight(&request);
        return render_spans_html(code, &spans);
    }
    foreign::highlight(&lang, code).unwrap_or_else(|| escape_html(code))
}
