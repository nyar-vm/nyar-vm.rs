//! Notedown 块级语法分析器。

mod inline;

use super::{
    lexer::{AtxHeaderData, NotedownLexer, NotedownToken, NotedownTokenKind, OrderedListItemData, TaskListItemData, TokenData, parse_attr},
    syntax::{
        Attr, Caption, DefinitionItem, ListAttributes, Meta, MetaValue, NotedownBlock, NotedownDocument, NotedownInline, TableBody, TableFoot,
        TableHead, TableRow,
    },
};
use inline::parse_inlines;

/// 解析 Notedown 文本为文档 AST。
pub fn parse(source: &str) -> NotedownDocument {
    let tokens = NotedownLexer::new(source).tokenize();
    NotedownParser::new(tokens).parse()
}

struct NotedownParser {
    tokens: Vec<NotedownToken>,
    pos: usize,
    footnote_blocks: Vec<NotedownBlock>,
}

impl NotedownParser {
    fn new(tokens: Vec<NotedownToken>) -> Self {
        Self { tokens, pos: 0, footnote_blocks: Vec::new() }
    }

    fn parse(mut self) -> NotedownDocument {
        let meta = self.parse_frontmatter();
        let mut blocks = self.parse_blocks();
        if !self.footnote_blocks.is_empty() {
            blocks.extend(std::mem::take(&mut self.footnote_blocks));
        }
        NotedownDocument { meta, blocks }
    }

    fn parse_frontmatter(&mut self) -> Meta {
        if self.pos >= self.tokens.len() {
            return Meta::default();
        }
        if !matches!(self.tokens[self.pos].kind, NotedownTokenKind::YamlFrontmatterDelimiter) {
            return Meta::default();
        }
        self.pos += 1;
        let mut values = std::collections::BTreeMap::new();
        while self.pos < self.tokens.len() {
            let token = &self.tokens[self.pos];
            if matches!(token.kind, NotedownTokenKind::YamlFrontmatterDelimiter) {
                self.pos += 1;
                break;
            }
            if matches!(token.kind, NotedownTokenKind::ParagraphText) {
                parse_yaml_line(&token.text, &mut values);
            }
            self.pos += 1;
        }
        Meta { values }
    }

    fn parse_blocks(&mut self) -> Vec<NotedownBlock> {
        let mut blocks = Vec::new();
        while self.pos < self.tokens.len() {
            match self.tokens[self.pos].kind {
                NotedownTokenKind::BlankLine => self.pos += 1,
                NotedownTokenKind::AtxHeader => blocks.push(self.parse_atx_header()),
                NotedownTokenKind::SetextUnderline => blocks.push(self.parse_setext_header()),
                NotedownTokenKind::CodeFenceStart => blocks.push(self.parse_code_block()),
                NotedownTokenKind::BlockQuote => blocks.push(self.parse_block_quote()),
                NotedownTokenKind::UnorderedListItem | NotedownTokenKind::TaskListItem => blocks.push(self.parse_bullet_list()),
                NotedownTokenKind::OrderedListItem => blocks.push(self.parse_ordered_list()),
                NotedownTokenKind::DefinitionTerm => blocks.push(self.parse_definition_list()),
                NotedownTokenKind::HorizontalRule => {
                    blocks.push(NotedownBlock::HorizontalRule);
                    self.pos += 1;
                }
                NotedownTokenKind::TableRow => blocks.push(self.parse_table()),
                NotedownTokenKind::DivFenceStart => blocks.push(self.parse_div()),
                NotedownTokenKind::LineBlockLine => blocks.push(self.parse_line_block()),
                NotedownTokenKind::FootnoteDefinition => self.parse_footnote_definition(),
                NotedownTokenKind::ParagraphText => blocks.push(self.parse_paragraph()),
                NotedownTokenKind::HtmlComment | NotedownTokenKind::EndOfFile => self.pos += 1,
                _ => self.pos += 1,
            }
        }
        blocks
    }

    fn parse_atx_header(&mut self) -> NotedownBlock {
        let token = self.tokens[self.pos].clone();
        self.pos += 1;
        let data = match token.data {
            Some(TokenData::AtxHeader(d)) => d,
            _ => AtxHeaderData { level: 1, content: String::new() },
        };
        let mut content = data.content;
        let mut attr = Attr::empty();
        if let Some(brace_start) = content.find(" {") {
            if let Some(brace_end) = content[brace_start..].find('}') {
                let abs_end = brace_start + brace_end;
                attr = parse_attr(content[brace_start + 2..abs_end].trim());
                content = content[..brace_start].to_string();
            }
        }
        content = content.trim_end_matches('#').trim().to_string();
        NotedownBlock::Header { level: data.level, attr, inlines: parse_inlines(&content) }
    }

    fn parse_setext_header(&mut self) -> NotedownBlock {
        let token = self.tokens[self.pos].clone();
        self.pos += 1;
        let level = if token.text.trim_start().starts_with('=') { 1 } else { 2 };
        let prev_text = if self.pos >= 2 { self.tokens[self.pos - 2].text.trim().to_string() } else { String::new() };
        NotedownBlock::Header { level, attr: Attr::empty(), inlines: parse_inlines(&prev_text) }
    }

    fn parse_code_block(&mut self) -> NotedownBlock {
        let start = self.tokens[self.pos].clone();
        self.pos += 1;
        let info = match start.data {
            Some(TokenData::CodeFenceInfo(s)) => s,
            _ => String::new(),
        };
        let mut language = info.clone();
        let mut attr_str = String::new();
        if let Some(brace) = info.find('{') {
            language = info[..brace].trim().to_string();
            if let Some(end) = info[brace..].find('}') {
                attr_str = info[brace + 1..brace + end].to_string();
            }
        }
        let mut text = String::new();
        while self.pos < self.tokens.len() {
            let kind = self.tokens[self.pos].kind;
            if matches!(kind, NotedownTokenKind::CodeFenceEnd) {
                self.pos += 1;
                break;
            }
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&self.tokens[self.pos].text);
            self.pos += 1;
        }
        let attr = if attr_str.is_empty() {
            if language.is_empty() { Attr::empty() } else { Attr { id: String::new(), classes: vec![language], key_values: Vec::new() } }
        }
        else {
            parse_attr(&attr_str)
        };
        NotedownBlock::CodeBlock { attr, text }
    }

    fn parse_block_quote(&mut self) -> NotedownBlock {
        let mut children = Vec::new();
        let mut para_lines = Vec::new();
        while self.pos < self.tokens.len() {
            match self.tokens[self.pos].kind {
                NotedownTokenKind::BlockQuote => {
                    let content = match &self.tokens[self.pos].data {
                        Some(TokenData::Text(s)) => s.clone(),
                        _ => String::new(),
                    };
                    if content.is_empty() {
                        if !para_lines.is_empty() {
                            children.push(build_paragraph(&para_lines));
                            para_lines.clear();
                        }
                        children.push(NotedownBlock::HorizontalRule);
                    }
                    else {
                        para_lines.push(content);
                    }
                    self.pos += 1;
                }
                NotedownTokenKind::BlankLine => {
                    if !para_lines.is_empty() {
                        children.push(build_paragraph(&para_lines));
                        para_lines.clear();
                    }
                    self.pos += 1;
                }
                _ => break,
            }
        }
        if !para_lines.is_empty() {
            children.push(build_paragraph(&para_lines));
        }
        NotedownBlock::BlockQuote { children }
    }

    fn parse_ordered_list(&mut self) -> NotedownBlock {
        let mut items = Vec::new();
        let mut start_number = 1i32;
        while self.pos < self.tokens.len() && matches!(self.tokens[self.pos].kind, NotedownTokenKind::OrderedListItem) {
            if let Some(TokenData::OrderedList(data)) = self.tokens[self.pos].data.clone() {
                if items.is_empty() {
                    start_number = data.start_number;
                }
                let inlines = parse_inlines(&data.content);
                items.push(vec![NotedownBlock::Plain { inlines }]);
            }
            self.pos += 1;
            self.skip_blank_lines();
        }
        NotedownBlock::OrderedList { attrs: ListAttributes { start_number }, items }
    }

    fn parse_bullet_list(&mut self) -> NotedownBlock {
        let mut items = Vec::new();
        while self.pos < self.tokens.len()
            && matches!(self.tokens[self.pos].kind, NotedownTokenKind::UnorderedListItem | NotedownTokenKind::TaskListItem)
        {
            let inlines = match self.tokens[self.pos].data.clone() {
                Some(TokenData::TaskList(TaskListItemData { content, is_checked })) => {
                    let mark = if is_checked { "x" } else { " " };
                    let mut v = vec![NotedownInline::Str(format!("[{mark}] "))];
                    v.extend(parse_inlines(&content));
                    v
                }
                Some(TokenData::Text(content)) => parse_inlines(&content),
                _ => Vec::new(),
            };
            items.push(vec![NotedownBlock::Plain { inlines }]);
            self.pos += 1;
            self.skip_blank_lines();
        }
        NotedownBlock::BulletList { items }
    }

    fn parse_definition_list(&mut self) -> NotedownBlock {
        let mut items = Vec::new();
        while self.pos < self.tokens.len() && matches!(self.tokens[self.pos].kind, NotedownTokenKind::DefinitionTerm) {
            let term = parse_inlines(self.tokens[self.pos].text.trim());
            self.pos += 1;
            let mut definitions = Vec::new();
            while self.pos < self.tokens.len() && matches!(self.tokens[self.pos].kind, NotedownTokenKind::DefinitionDescription) {
                let content = match &self.tokens[self.pos].data {
                    Some(TokenData::Text(s)) => s.as_str(),
                    _ => "",
                };
                definitions.push(vec![NotedownBlock::Plain { inlines: parse_inlines(content) }]);
                self.pos += 1;
                self.skip_blank_lines();
            }
            items.push(DefinitionItem { term, definitions });
            self.skip_blank_lines();
        }
        NotedownBlock::DefinitionList { items }
    }

    fn parse_table(&mut self) -> NotedownBlock {
        let mut all_rows = Vec::new();
        let mut captions = Vec::new();
        while self.pos < self.tokens.len() {
            match self.tokens[self.pos].kind {
                NotedownTokenKind::TableRow => {
                    let cells = match &self.tokens[self.pos].data {
                        Some(TokenData::TableCells(c)) => c.clone(),
                        _ => Vec::new(),
                    };
                    let row = TableRow { cells: cells.into_iter().map(|c| parse_inlines(&c)).collect() };
                    all_rows.push(row);
                    self.pos += 1;
                    self.skip_blank_lines();
                }
                NotedownTokenKind::ParagraphText if self.tokens[self.pos].text.starts_with("Table:") => {
                    captions.push(self.tokens[self.pos].text[6..].trim().to_string());
                    self.pos += 1;
                    self.skip_blank_lines();
                }
                _ => break,
            }
        }
        let caption = captions.first().map(|c| Caption { inlines: parse_inlines(c) });
        let head_rows = all_rows.first().cloned().into_iter().collect();
        let body_rows = if all_rows.len() > 1 { all_rows[1..].to_vec() } else { Vec::new() };
        NotedownBlock::Table {
            attr: Attr::empty(),
            caption,
            head: TableHead { attr: Attr::empty(), rows: head_rows },
            bodies: vec![TableBody { attr: Attr::empty(), row_head_columns: 0, rows: body_rows }],
            foot: TableFoot { attr: Attr::empty(), rows: Vec::new() },
        }
    }

    fn parse_div(&mut self) -> NotedownBlock {
        let attr = match self.tokens[self.pos].data.clone() {
            Some(TokenData::DivAttr(a)) => a,
            _ => Attr::empty(),
        };
        self.pos += 1;
        let mut children = Vec::new();
        while self.pos < self.tokens.len() {
            if matches!(self.tokens[self.pos].kind, NotedownTokenKind::DivFenceEnd) {
                self.pos += 1;
                break;
            }
            if matches!(self.tokens[self.pos].kind, NotedownTokenKind::BlankLine) {
                self.pos += 1;
                continue;
            }
            if let Some(block) = self.parse_single_block() {
                children.push(block);
            }
        }
        NotedownBlock::Div { attr, children }
    }

    fn parse_line_block(&mut self) -> NotedownBlock {
        let mut lines = Vec::new();
        while self.pos < self.tokens.len() && matches!(self.tokens[self.pos].kind, NotedownTokenKind::LineBlockLine) {
            let content = match &self.tokens[self.pos].data {
                Some(TokenData::Text(s)) => s.as_str(),
                _ => "",
            };
            lines.push(parse_inlines(content));
            self.pos += 1;
        }
        NotedownBlock::LineBlock { lines }
    }

    fn parse_footnote_definition(&mut self) {
        if let Some(TokenData::Footnote(data)) = self.tokens[self.pos].data.clone() {
            self.pos += 1;
            let inlines = parse_inlines(&data.content);
            self.footnote_blocks.push(NotedownBlock::Para { inlines });
        }
        else {
            self.pos += 1;
        }
    }

    fn parse_paragraph(&mut self) -> NotedownBlock {
        let mut lines = Vec::new();
        while self.pos < self.tokens.len() {
            match self.tokens[self.pos].kind {
                NotedownTokenKind::ParagraphText => {
                    lines.push(self.tokens[self.pos].text.clone());
                    self.pos += 1;
                }
                NotedownTokenKind::BlankLine => {
                    self.pos += 1;
                    break;
                }
                _ => break,
            }
        }
        build_paragraph(&lines)
    }

    fn parse_single_block(&mut self) -> Option<NotedownBlock> {
        match self.tokens[self.pos].kind {
            NotedownTokenKind::AtxHeader => Some(self.parse_atx_header()),
            NotedownTokenKind::ParagraphText => Some(self.parse_paragraph()),
            NotedownTokenKind::UnorderedListItem | NotedownTokenKind::TaskListItem => Some(self.parse_bullet_list()),
            NotedownTokenKind::OrderedListItem => Some(self.parse_ordered_list()),
            _ => {
                self.pos += 1;
                None
            }
        }
    }

    fn skip_blank_lines(&mut self) {
        while self.pos < self.tokens.len() && matches!(self.tokens[self.pos].kind, NotedownTokenKind::BlankLine) {
            self.pos += 1;
        }
    }
}

fn build_paragraph(lines: &[String]) -> NotedownBlock {
    if lines.is_empty() {
        return NotedownBlock::Para { inlines: Vec::new() };
    }
    let text = lines.join(" ");
    NotedownBlock::Para { inlines: parse_inlines(&text) }
}

fn parse_yaml_line(line: &str, values: &mut std::collections::BTreeMap<String, MetaValue>) {
    let Some((key, value)) = line.split_once(':')
    else {
        return;
    };
    let key = key.trim().to_string();
    let value_str = value.trim();
    if value_str == "true" {
        values.insert(key, MetaValue::Bool(true));
    }
    else if value_str == "false" {
        values.insert(key, MetaValue::Bool(false));
    }
    else {
        values.insert(key, MetaValue::String(value_str.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_heading_and_paragraph() {
        let doc = parse("# Hello\n\nWorld");
        assert!(matches!(doc.blocks[0], NotedownBlock::Header { level: 1, .. }));
        assert!(matches!(doc.blocks[1], NotedownBlock::Para { .. }));
    }

    #[test]
    fn parse_code_fence_with_language() {
        let doc = parse("```rust\nlet x = 1;\n```");
        if let NotedownBlock::CodeBlock { attr, text } = &doc.blocks[0] {
            assert_eq!(attr.language(), "rust");
            assert!(text.contains("let x"));
        }
        else {
            panic!("expected code block");
        }
    }

    #[test]
    fn parse_gfm_table() {
        let doc = parse("| A | B |\n|---|---|\n| 1 | 2 |");
        assert!(matches!(doc.blocks[0], NotedownBlock::Table { .. }));
    }
}
