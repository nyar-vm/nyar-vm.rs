//! Markdown 渲染与标题提取。

use std::path::Path;

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd, html};

/// 文档页面渲染上下文。
#[derive(Debug, Clone, Default)]
pub struct DocRenderContext {
    /// 当前 `.md` 文件相对分区根目录的路径（POSIX `/`）。
    pub relative_md_path: String,
}

impl DocRenderContext {
    /// 从相对 Markdown 路径构建上下文。
    pub fn from_relative_md(relative: impl Into<String>) -> Self {
        Self { relative_md_path: relative.into() }
    }

    fn current_dir(&self) -> String {
        Path::new(&self.relative_md_path).parent().map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default()
    }
}

/// HTML 渲染结果。
#[derive(Debug, Clone, Default)]
pub struct HtmlRenderResult {
    /// 正文 HTML。
    pub html: String,
    /// 是否包含 KaTeX 公式（页面需 KaTeX CSS）。
    pub has_katex: bool,
}

/// 将 Markdown 转为 HTML 正文片段。
pub fn render_markdown_to_html(source: &str, ctx: &DocRenderContext) -> HtmlRenderResult {
    let rewritten = rewrite_md_links(source, ctx);
    HtmlRenderResult { html: render_markdown_fragment(&rewritten), has_katex: false }
}

/// 将 Markdown 转为 HTML（无链接上下文，兼容旧调用）。
pub fn render_markdown_to_html_simple(source: &str) -> String {
    render_markdown_fragment(source)
}

/// 从 Markdown 提取第一个 `#` 标题。
pub fn extract_title(source: &str) -> Option<String> {
    let mut in_title = false;
    let mut title = String::new();
    for event in Parser::new_ext(source, Options::all()) {
        match event {
            Event::Start(Tag::Heading { level: HeadingLevel::H1, .. }) => in_title = true,
            Event::End(TagEnd::Heading(HeadingLevel::H1)) if in_title => return Some(title),
            Event::Text(text) if in_title => title.push_str(&text),
            Event::Code(code) if in_title => title.push_str(&code),
            Event::SoftBreak | Event::HardBreak if in_title => title.push(' '),
            _ => {}
        }
    }
    None
}

fn render_markdown_fragment(source: &str) -> String {
    let parser = Parser::new_ext(source, Options::all());
    let mut html_out = String::new();
    html::push_html(&mut html_out, parser);
    html_out
}

fn rewrite_md_links(source: &str, ctx: &DocRenderContext) -> String {
    let _ = ctx.current_dir();
    source
        .lines()
        .map(|line| {
            if let Some((prefix, url)) = line.split_once("](") {
                if let Some(suffix) = url.strip_suffix(')') {
                    if suffix.ends_with(".md") {
                        let rewritten = format!("{}.html", &suffix[..suffix.len() - 3]);
                        return format!("{prefix}]({rewritten})");
                    }
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[cfg(test)]
#[path = "../../../tests/path_wired/cmds_doc_markdown.rs"]
mod cmds_doc_markdown_tests;
