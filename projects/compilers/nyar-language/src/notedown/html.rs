//! Notedown AST → HTML 渲染。

use std::sync::OnceLock;

use katex::{KatexContext, Settings, render_to_string};
use std_data::text::markdown;
use std_data::text::notedown::{MathType, NotedownBlock, NotedownDocument, NotedownInline, QuoteType};

use crate::notedown::highlight::highlight_code_block;

use super::options::NotedownHtmlOptions;

/// HTML 渲染结果。
#[derive(Debug, Clone, Default)]
pub struct NotedownHtmlResult {
    /// 正文 HTML 片段。
    pub html: String,
    /// 是否包含 KaTeX 输出（需注入 KaTeX CSS）。
    pub has_katex: bool,
}

static KATEX_CTX: OnceLock<KatexContext> = OnceLock::new();

fn katex_ctx() -> &'static KatexContext {
    KATEX_CTX.get_or_init(KatexContext::default)
}

/// 将 Markdown 源文本渲染为 HTML。
pub fn render_markdown(source: &str, options: &NotedownHtmlOptions) -> NotedownHtmlResult {
    let document = markdown::parse(source);
    render(&document, options)
}

/// 将 Notedown 文档渲染为 HTML。
pub fn render(document: &NotedownDocument, options: &NotedownHtmlOptions) -> NotedownHtmlResult {
    let mut renderer = HtmlRenderer::new(options);
    for block in &document.blocks {
        renderer.render_block(block);
    }
    NotedownHtmlResult { html: renderer.out, has_katex: renderer.has_katex }
}

struct HtmlRenderer<'a> {
    options: &'a NotedownHtmlOptions,
    out: String,
    has_katex: bool,
    heading_counter: u32,
}

impl<'a> HtmlRenderer<'a> {
    fn new(options: &'a NotedownHtmlOptions) -> Self {
        Self { options, out: String::new(), has_katex: false, heading_counter: 0 }
    }

    fn render_block(&mut self, block: &NotedownBlock) {
        match block {
            NotedownBlock::Para { inlines } => {
                self.out.push_str("<p>");
                self.render_inlines(inlines);
                self.out.push_str("</p>\n");
            }
            NotedownBlock::Plain { inlines } => {
                self.render_inlines(inlines);
                self.out.push('\n');
            }
            NotedownBlock::Header { level, inlines, attr } => self.render_header(*level, inlines, attr),
            NotedownBlock::CodeBlock { attr, text } => self.render_code_block(attr.language(), text),
            NotedownBlock::BlockQuote { children } => {
                self.out.push_str("<blockquote>\n");
                for child in children {
                    self.render_block(child);
                }
                self.out.push_str("</blockquote>\n");
            }
            NotedownBlock::BulletList { items } => {
                self.out.push_str("<ul>\n");
                for item in items {
                    self.out.push_str("<li>");
                    self.render_list_item(item);
                    self.out.push_str("</li>\n");
                }
                self.out.push_str("</ul>\n");
            }
            NotedownBlock::OrderedList { attrs, items } => {
                let start = attrs.start_number;
                if start == 1 {
                    self.out.push_str("<ol>\n");
                } else {
                    self.out.push_str(&format!("<ol start=\"{start}\">\n"));
                }
                for item in items {
                    self.out.push_str("<li>");
                    self.render_list_item(item);
                    self.out.push_str("</li>\n");
                }
                self.out.push_str("</ol>\n");
            }
            NotedownBlock::HorizontalRule => self.out.push_str("<hr />\n"),
            NotedownBlock::Table { head, bodies, caption, .. } => self.render_table(head, bodies, caption.as_ref()),
            NotedownBlock::LineBlock { lines } => {
                for line in lines {
                    self.out.push_str("<p>");
                    self.render_inlines(line);
                    self.out.push_str("<br />\n");
                }
            }
            NotedownBlock::DefinitionList { items } => {
                self.out.push_str("<dl>\n");
                for item in items {
                    self.out.push_str("<dt>");
                    self.render_inlines(&item.term);
                    self.out.push_str("</dt>\n");
                    for def in &item.definitions {
                        self.out.push_str("<dd>");
                        for b in def {
                            self.render_block(b);
                        }
                        self.out.push_str("</dd>\n");
                    }
                }
                self.out.push_str("</dl>\n");
            }
            NotedownBlock::Div { attr, children } => {
                self.out.push_str("<div");
                self.write_attr(attr);
                self.out.push_str(">\n");
                for child in children {
                    self.render_block(child);
                }
                self.out.push_str("</div>\n");
            }
            NotedownBlock::RawBlock { content, format } => {
                if format == "html" {
                    self.out.push_str(content);
                    self.out.push('\n');
                }
            }
            NotedownBlock::Null => {}
        }
    }

    fn render_list_item(&mut self, blocks: &[NotedownBlock]) {
        for (i, block) in blocks.iter().enumerate() {
            if i > 0 {
                self.out.push_str("<br />\n");
            }
            self.render_block(block);
        }
    }

    fn render_header(&mut self, level: u8, inlines: &[NotedownInline], _attr: &std_data::text::notedown::Attr) {
        let level = level.clamp(1, 6);
        let text = inlines_to_plain(inlines);
        let id_attr = if self.options.generate_heading_ids {
            self.heading_counter += 1;
            let id = slugify_heading(&text, self.heading_counter);
            format!(" id=\"{id}\"")
        } else {
            String::new()
        };
        self.out.push_str(&format!("<h{level}{id_attr}>"));
        self.render_inlines(inlines);
        self.out.push_str(&format!("</h{level}>\n"));
    }

    fn render_code_block(&mut self, language: &str, text: &str) {
        let lang = language.trim();
        self.out.push_str("<div class=\"vp-code-block\">");
        if !lang.is_empty() {
            self.out.push_str(&format!("<div class=\"vp-code-lang\">{}</div>", escape_html(lang)));
        }
        self.out.push_str("<pre><code");
        if !lang.is_empty() {
            self.out.push_str(&format!(" class=\"language-{}\"", escape_html(lang)));
        }
        self.out.push('>');
        if self.options.highlight_code {
            self.out.push_str(&highlight_code_block(lang, text));
        } else {
            self.out.push_str(&escape_html(text));
        }
        self.out.push_str("</code></pre></div>\n");
    }

    fn render_table(
        &mut self,
        head: &std_data::text::notedown::TableHead,
        bodies: &[std_data::text::notedown::TableBody],
        caption: Option<&std_data::text::notedown::Caption>,
    ) {
        if let Some(caption) = caption {
            self.out.push_str("<p><em>");
            self.render_inlines(&caption.inlines);
            self.out.push_str("</em></p>\n");
        }
        self.out.push_str("<div class=\"vp-table-wrapper\"><table>\n");
        if !head.rows.is_empty() {
            self.out.push_str("<thead>\n");
            for row in &head.rows {
                self.render_table_row(row, true);
            }
            self.out.push_str("</thead>\n");
        }
        self.out.push_str("<tbody>\n");
        for body in bodies {
            for row in &body.rows {
                if is_table_separator_row(row) {
                    continue;
                }
                self.render_table_row(row, false);
            }
        }
        self.out.push_str("</tbody></table></div>\n");
    }

    fn render_table_row(&mut self, row: &std_data::text::notedown::TableRow, header: bool) {
        self.out.push_str("<tr>\n");
        let tag = if header { "th" } else { "td" };
        for cell in &row.cells {
            self.out.push_str(&format!("<{tag}>"));
            self.render_inlines(cell);
            self.out.push_str(&format!("</{tag}>\n"));
        }
        self.out.push_str("</tr>\n");
    }

    fn render_inlines(&mut self, inlines: &[NotedownInline]) {
        for inline in inlines {
            self.render_inline(inline);
        }
    }

    fn render_inline(&mut self, inline: &NotedownInline) {
        match inline {
            NotedownInline::Str(s) => self.out.push_str(&escape_html(s)),
            NotedownInline::Emph(c) => {
                self.out.push_str("<em>");
                self.render_inlines(c);
                self.out.push_str("</em>");
            }
            NotedownInline::Strong(c) => {
                self.out.push_str("<strong>");
                self.render_inlines(c);
                self.out.push_str("</strong>");
            }
            NotedownInline::Strikeout(c) => {
                self.out.push_str("<del>");
                self.render_inlines(c);
                self.out.push_str("</del>");
            }
            NotedownInline::Code(s) => {
                self.out.push_str("<code>");
                self.out.push_str(&escape_html(s));
                self.out.push_str("</code>");
            }
            NotedownInline::Math { math_type, content } => self.render_math(*math_type, content),
            NotedownInline::Link { content, target, .. } => {
                let url = rewrite_link_url(&target.url, self.options);
                self.out.push_str(&format!("<a href=\"{}\"", escape_html_attr(&url)));
                if !target.title.is_empty() {
                    self.out.push_str(&format!(" title=\"{}\"", escape_html_attr(&target.title)));
                }
                self.out.push('>');
                self.render_inlines(content);
                self.out.push_str("</a>");
            }
            NotedownInline::Image { alt, target, .. } => {
                let title = if target.title.is_empty() {
                    String::new()
                } else {
                    format!(" title=\"{}\"", escape_html_attr(&target.title))
                };
                let alt_text = escape_html_attr(&inlines_to_plain(alt));
                self.out.push_str(&format!("<img src=\"{}\" alt=\"{alt_text}\"{title} />", escape_html_attr(&target.url)));
            }
            NotedownInline::SoftBreak => self.out.push('\n'),
            NotedownInline::Space => self.out.push(' '),
            NotedownInline::LineBreak => self.out.push_str("<br />\n"),
            NotedownInline::Cite { citations } => {
                self.out.push_str("[@");
                self.out.push_str(&citations.join("; @"));
                self.out.push(']');
            }
            NotedownInline::Span { attr, content } => {
                if attr.classes.iter().any(|c| c == "highlight") {
                    self.out.push_str("<mark>");
                    self.render_inlines(content);
                    self.out.push_str("</mark>");
                } else {
                    self.out.push_str("<span");
                    self.write_attr(attr);
                    self.out.push('>');
                    self.render_inlines(content);
                    self.out.push_str("</span>");
                }
            }
            NotedownInline::Quoted { quote_type, content } => {
                let (open, close) = match quote_type {
                    QuoteType::Double => ("\"", "\""),
                    QuoteType::Single => ("'", "'"),
                };
                self.out.push_str(open);
                self.render_inlines(content);
                self.out.push_str(close);
            }
            NotedownInline::Superscript(c) => {
                self.out.push_str("<sup>");
                self.render_inlines(c);
                self.out.push_str("</sup>");
            }
            NotedownInline::Subscript(c) => {
                self.out.push_str("<sub>");
                self.render_inlines(c);
                self.out.push_str("</sub>");
            }
            NotedownInline::SmallCaps(c) => {
                self.out.push_str("<span class=\"smallcaps\">");
                self.render_inlines(c);
                self.out.push_str("</span>");
            }
            NotedownInline::RawInline { content, format } => {
                if format == "html" {
                    self.out.push_str(content);
                }
            }
            NotedownInline::Note(_) => self.out.push_str("<sup class=\"footnote-ref\">[?]</sup>"),
        }
    }

    fn render_math(&mut self, math_type: MathType, latex: &str) {
        match render_katex(math_type, latex) {
            Ok(html) => {
                self.has_katex = true;
                match math_type {
                    MathType::Inline => {
                        self.out.push_str("<span class=\"vp-math vp-math-inline\">");
                        self.out.push_str(&html);
                        self.out.push_str("</span>");
                    }
                    MathType::Display => {
                        self.out.push_str("<div class=\"vp-math vp-math-display\">");
                        self.out.push_str(&html);
                        self.out.push_str("</div>\n");
                    }
                }
            }
            Err(err) => {
                let title = escape_html_attr(&err.to_string());
                let body = escape_html(latex);
                self.out.push_str(&format!("<span class=\"vp-math-error\" title=\"{title}\">{body}</span>"));
            }
        }
    }

    fn write_attr(&mut self, attr: &std_data::text::notedown::Attr) {
        if !attr.id.is_empty() {
            self.out.push_str(&format!(" id=\"{}\"", escape_html_attr(&attr.id)));
        }
        if !attr.classes.is_empty() {
            self.out.push_str(&format!(" class=\"{}\"", escape_html_attr(&attr.classes.join(" "))));
        }
    }
}

fn render_katex(math_type: MathType, latex: &str) -> Result<String, katex::ParseError> {
    let settings = match math_type {
        MathType::Inline => Settings::default(),
        MathType::Display => Settings::builder().display_mode(true).build(),
    };
    render_to_string(katex_ctx(), latex, &settings)
}

fn is_table_separator_row(row: &std_data::text::notedown::TableRow) -> bool {
    row.cells.iter().all(|cell| {
        let text: String = cell
            .iter()
            .filter_map(|i| match i {
                NotedownInline::Str(s) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        let t = text.trim();
        t.is_empty() || t.chars().all(|c| c == '-' || c == ':' || c == '|' || c.is_whitespace())
    })
}

fn rewrite_link_url(url: &str, options: &NotedownHtmlOptions) -> String {
    if !options.rewrite_md_links {
        return url.to_string();
    }
    if url.starts_with("http://") || url.starts_with("https://") || url.starts_with('#') || url.starts_with("mailto:") {
        return url.to_string();
    }
    if url.ends_with(".md") {
        let mut rewritten = url.to_string();
        if let Some(stripped) = rewritten.strip_suffix(".md") {
            rewritten = format!("{stripped}.html");
        }
        return rewritten;
    }
    url.to_string()
}

fn slugify_heading(text: &str, counter: u32) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if (ch == ' ' || ch == '-' || ch == '_') && !last_dash && !slug.is_empty() {
            slug.push('-');
            last_dash = true;
        }
    }
    if slug.is_empty() {
        slug = format!("heading-{counter}");
    }
    slug
}

fn inlines_to_plain(inlines: &[NotedownInline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            NotedownInline::Str(s) => out.push_str(s),
            NotedownInline::Code(s) => out.push_str(s),
            NotedownInline::Emph(c) | NotedownInline::Strong(c) | NotedownInline::Strikeout(c) => {
                out.push_str(&inlines_to_plain(c));
            }
            NotedownInline::Space | NotedownInline::SoftBreak => out.push(' '),
            _ => {}
        }
    }
    out.trim().to_string()
}

fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

fn escape_html_attr(text: &str) -> String {
    escape_html(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_heading_with_id() {
        let doc = markdown::parse("# Hello\n\nWorld");
        let result = render(&doc, &NotedownHtmlOptions::legion_doc(""));
        assert!(result.html.contains("<h1 id=\"hello\">"));
        assert!(result.html.contains("<p>"));
    }

    #[test]
    fn rewrites_md_links() {
        let doc = markdown::parse("[link](./foo.md)");
        let result = render(&doc, &NotedownHtmlOptions::legion_doc(""));
        assert!(result.html.contains("href=\"./foo.html\""));
    }

    #[test]
    fn renders_code_block_wrapper() {
        let doc = markdown::parse("```rust\nlet x = 1;\n```");
        let result = render(&doc, &NotedownHtmlOptions::legion_doc(""));
        assert!(result.html.contains("vp-code-block"));
        assert!(result.html.contains("vp-code-lang"));
    }
}
