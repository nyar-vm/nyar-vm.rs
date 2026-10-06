//! Notedown 文本格式化（round-trip）。

use super::syntax::{Attr, Caption, MathType, NotedownBlock, NotedownDocument, NotedownInline, QuoteType};

/// 将文档格式化为 Notedown 文本。
pub fn format(document: &NotedownDocument) -> String {
    let mut out = String::new();
    if !document.meta.values.is_empty() {
        out.push_str("---\n");
        for (key, value) in &document.meta.values {
            match value {
                super::MetaValue::String(s) => out.push_str(&format!("{key}: {s}\n")),
                super::MetaValue::Bool(b) => out.push_str(&format!("{key}: {b}\n")),
            }
        }
        out.push_str("---\n\n");
    }
    for block in &document.blocks {
        format_block(block, &mut out);
        out.push('\n');
    }
    out
}

fn format_block(block: &NotedownBlock, out: &mut String) {
    match block {
        NotedownBlock::Para { inlines } => {
            format_inlines(inlines, out);
            out.push('\n');
        }
        NotedownBlock::Plain { inlines } => format_inlines(inlines, out),
        NotedownBlock::Header { level, attr, inlines } => {
            out.extend(std::iter::repeat('#').take(*level as usize));
            out.push(' ');
            format_inlines(inlines, out);
            if !attr.is_empty() {
                out.push(' ');
                format_attr(attr, out);
            }
            out.push('\n');
        }
        NotedownBlock::CodeBlock { attr, text } => {
            let lang = attr.language();
            out.push_str("```");
            out.push_str(lang);
            if !attr.id.is_empty() || attr.classes.len() > 1 {
                out.push(' ');
                format_attr(attr, out);
            }
            out.push('\n');
            out.push_str(text);
            if !text.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("```\n");
        }
        NotedownBlock::BlockQuote { children } => {
            for child in children {
                out.push_str("> ");
                format_block(child, out);
            }
        }
        NotedownBlock::BulletList { items } => {
            for item in items {
                out.push_str("- ");
                for b in item {
                    format_block(b, out);
                }
                out.push('\n');
            }
        }
        NotedownBlock::OrderedList { attrs, items } => {
            let mut n = attrs.start_number;
            for item in items {
                out.push_str(&format!("{n}. "));
                n += 1;
                for b in item {
                    format_block(b, out);
                }
                out.push('\n');
            }
        }
        NotedownBlock::HorizontalRule => out.push_str("---\n"),
        NotedownBlock::Table { head, bodies, caption, .. } => {
            if let Some(Caption { inlines }) = caption {
                out.push_str("Table: ");
                format_inlines(inlines, out);
                out.push('\n');
            }
            for row in &head.rows {
                format_table_row(row, out);
            }
            for body in bodies {
                for row in &body.rows {
                    format_table_row(row, out);
                }
            }
        }
        NotedownBlock::LineBlock { lines } => {
            for line in lines {
                out.push_str("| ");
                format_inlines(line, out);
                out.push('\n');
            }
        }
        NotedownBlock::DefinitionList { items } => {
            for item in items {
                format_inlines(&item.term, out);
                out.push('\n');
                for def in &item.definitions {
                    out.push_str(": ");
                    for b in def {
                        format_block(b, out);
                    }
                    out.push('\n');
                }
            }
        }
        NotedownBlock::Div { attr, children } => {
            out.push_str(":::");
            if !attr.is_empty() {
                out.push(' ');
                format_attr(attr, out);
            }
            out.push('\n');
            for child in children {
                format_block(child, out);
            }
            out.push_str(":::\n");
        }
        NotedownBlock::RawBlock { content, .. } => out.push_str(content),
        NotedownBlock::Null => {}
    }
}

fn format_table_row(row: &super::TableRow, out: &mut String) {
    out.push('|');
    for cell in &row.cells {
        out.push(' ');
        format_inlines(cell, out);
        out.push_str(" |");
    }
    out.push('\n');
}

fn format_attr(attr: &Attr, out: &mut String) {
    out.push('{');
    if !attr.id.is_empty() {
        out.push('#');
        out.push_str(&attr.id);
    }
    for class in &attr.classes {
        out.push('.');
        out.push_str(class);
    }
    for (k, v) in &attr.key_values {
        out.push(' ');
        out.push_str(k);
        out.push('=');
        out.push_str(v);
    }
    out.push('}');
}

fn format_inlines(inlines: &[NotedownInline], out: &mut String) {
    for inline in inlines {
        match inline {
            NotedownInline::Str(s) => out.push_str(s),
            NotedownInline::Emph(c) => {
                out.push('*');
                format_inlines(c, out);
                out.push('*');
            }
            NotedownInline::Strong(c) => {
                out.push_str("**");
                format_inlines(c, out);
                out.push_str("**");
            }
            NotedownInline::Strikeout(c) => {
                out.push_str("~~");
                format_inlines(c, out);
                out.push_str("~~");
            }
            NotedownInline::Code(s) => {
                out.push('`');
                out.push_str(s);
                out.push('`');
            }
            NotedownInline::Math { math_type, content } => match math_type {
                MathType::Inline => {
                    out.push('$');
                    out.push_str(content);
                    out.push('$');
                }
                MathType::Display => {
                    out.push_str("$$");
                    out.push_str(content);
                    out.push_str("$$");
                }
            },
            NotedownInline::Link { content, target, .. } => {
                out.push('[');
                format_inlines(content, out);
                out.push(']');
                out.push('(');
                out.push_str(&target.url);
                if !target.title.is_empty() {
                    out.push_str(" \"");
                    out.push_str(&target.title);
                    out.push('"');
                }
                out.push(')');
            }
            NotedownInline::Image { alt, target, .. } => {
                out.push_str("![");
                format_inlines(alt, out);
                out.push(']');
                out.push('(');
                out.push_str(&target.url);
                out.push(')');
            }
            NotedownInline::SoftBreak | NotedownInline::Space => out.push(' '),
            NotedownInline::LineBreak => out.push_str("  \n"),
            NotedownInline::Cite { citations } => {
                out.push_str("[@");
                out.push_str(&citations.join("; @"));
                out.push(']');
            }
            NotedownInline::Span { attr, content } => {
                out.push('[');
                if !attr.classes.is_empty() {
                    out.push('.');
                    out.push_str(&attr.classes.join("."));
                }
                out.push(']');
                format_inlines(content, out);
                out.push_str("[/]");
            }
            NotedownInline::Quoted { quote_type, content } => {
                let (open, close) = match quote_type {
                    QuoteType::Double => ("\"", "\""),
                    QuoteType::Single => ("'", "'"),
                };
                out.push_str(open);
                format_inlines(content, out);
                out.push_str(close);
            }
            NotedownInline::Superscript(c) => {
                out.push('^');
                format_inlines(c, out);
                out.push('^');
            }
            NotedownInline::Subscript(c) => {
                out.push('~');
                format_inlines(c, out);
                out.push('~');
            }
            NotedownInline::SmallCaps(c) => {
                out.push_str("[.smallcaps]");
                format_inlines(c, out);
                out.push_str("[/.smallcaps]");
            }
            NotedownInline::RawInline { content, .. } => out.push_str(content),
            NotedownInline::Note(_) => out.push_str("[^note]"),
        }
    }
}

impl Attr {
    fn is_empty(&self) -> bool {
        self.id.is_empty() && self.classes.is_empty() && self.key_values.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::NotedownDocument;

    #[test]
    fn round_trip_heading() {
        let source = "# Title\n\nBody";
        let doc = NotedownDocument::parse(source);
        let formatted = format(&doc);
        let doc2 = NotedownDocument::parse(&formatted);
        assert_eq!(doc.blocks.len(), doc2.blocks.len());
    }
}
