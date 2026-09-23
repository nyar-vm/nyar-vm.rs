//! 外部语言 TextMate 回退高亮（非 Valkyrie 生态 language plugin）。

use std::sync::OnceLock;

use syntect::{highlighting::ThemeSet, parsing::SyntaxSet};

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme_set() -> &'static ThemeSet {
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

/// 尝试用 syntect 高亮；未知语法时返回 `None`。
pub fn highlight(language: &str, code: &str) -> Option<String> {
    let ps = syntax_set();
    let syntax = ps.find_syntax_by_token(language).or_else(|| ps.find_syntax_by_extension(language))?;
    let theme = &theme_set().themes["base16-ocean.dark"];
    syntect::html::highlighted_html_for_string(code, ps, syntax, theme).ok()
}
