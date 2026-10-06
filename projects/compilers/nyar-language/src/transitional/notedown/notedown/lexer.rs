//! Notedown 块级词法分析器。

use super::syntax::Attr;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotedownTokenKind {
    /// 空行。
    BlankLine,
    /// ATX 标题。
    AtxHeader,
    /// Setext 下划线。
    SetextUnderline,
    /// 代码围栏开始。
    CodeFenceStart,
    /// 代码围栏结束。
    CodeFenceEnd,
    /// 引用块。
    BlockQuote,
    /// 无序列表项。
    UnorderedListItem,
    /// 有序列表项。
    OrderedListItem,
    /// 任务列表项。
    TaskListItem,
    /// 定义术语。
    DefinitionTerm,
    /// 定义描述。
    DefinitionDescription,
    /// 分割线。
    HorizontalRule,
    /// 表格行。
    TableRow,
    /// Div 围栏开始。
    DivFenceStart,
    /// Div 围栏结束。
    DivFenceEnd,
    /// YAML frontmatter 分隔符。
    YamlFrontmatterDelimiter,
    /// 行块行。
    LineBlockLine,
    /// 段落文本。
    ParagraphText,
    /// HTML 注释。
    HtmlComment,
    /// 脚注定义。
    FootnoteDefinition,
    /// 文件结束。
    EndOfFile,
}

/// ATX 标题附加数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtxHeaderData {
    /// 级别 1-6。
    pub level: u8,
    /// 标题正文（含可选属性）。
    pub content: String,
}

/// 有序列表项数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderedListItemData {
    /// 项内容。
    pub content: String,
    /// 起始编号。
    pub start_number: i32,
}

/// 任务列表项数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskListItemData {
    /// 项内容。
    pub content: String,
    /// 是否勾选。
    pub is_checked: bool,
}

/// 脚注定义数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FootnoteData {
    /// 脚注 id。
    pub id: String,
    /// 定义内容。
    pub content: String,
}

/// 词法单元附加数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenData {
    /// ATX 标题。
    AtxHeader(AtxHeaderData),
    /// 字符串内容（引用/列表等）。
    Text(String),
    /// 有序列表。
    OrderedList(OrderedListItemData),
    /// 任务列表。
    TaskList(TaskListItemData),
    /// 代码围栏 info 字符串。
    CodeFenceInfo(String),
    /// Div 属性。
    DivAttr(Attr),
    /// 表格单元格。
    TableCells(Vec<String>),
    /// 脚注。
    Footnote(FootnoteData),
}

/// 词法单元。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotedownToken {
    /// 类型。
    pub kind: NotedownTokenKind,
    /// 原始行文本。
    pub text: String,
    /// 行号（1-based）。
    pub line: usize,
    /// 缩进。
    pub indent: usize,
    /// 附加数据。
    pub data: Option<TokenData>,
}

/// Notedown 词法分析器（拥有型，归一化 `\r\n`）。
pub struct NotedownLexer {
    lines: Vec<String>,
    line_index: usize,
    in_yaml_frontmatter: bool,
}

impl NotedownLexer {
    /// 创建词法分析器。
    pub fn new(source: &str) -> Self {
        let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
        Self { lines: normalized.split('\n').map(str::to_string).collect(), line_index: 0, in_yaml_frontmatter: false }
    }

    /// 执行词法分析。
    pub fn tokenize(mut self) -> Vec<NotedownToken> {
        let mut tokens = Vec::new();
        while self.line_index < self.lines.len() {
            let line = self.lines[self.line_index].clone();
            let trimmed = line.trim_start();
            let indent = line.len() - trimmed.len();
            if trimmed.is_empty() {
                tokens.push(NotedownToken { kind: NotedownTokenKind::BlankLine, text: line, line: self.line_index + 1, indent, data: None });
                self.line_index += 1;
                continue;
            }
            let trimmed_owned = trimmed.to_string();
            tokens.push(self.classify_line(&trimmed_owned, indent, line));
            self.line_index += 1;
        }
        tokens.push(NotedownToken {
            kind: NotedownTokenKind::EndOfFile,
            text: String::new(),
            line: self.lines.len() + 1,
            indent: 0,
            data: None,
        });
        tokens
    }

    fn classify_line(&mut self, trimmed: &str, indent: usize, line: String) -> NotedownToken {
        let line_no = self.line_index + 1;
        if self.line_index == 0 && trimmed == "---" {
            self.in_yaml_frontmatter = true;
            return token(NotedownTokenKind::YamlFrontmatterDelimiter, line, line_no, indent, None);
        }
        if self.in_yaml_frontmatter {
            if trimmed == "---" {
                self.in_yaml_frontmatter = false;
                return token(NotedownTokenKind::YamlFrontmatterDelimiter, line, line_no, indent, None);
            }
            return token(NotedownTokenKind::ParagraphText, line, line_no, indent, None);
        }
        if is_code_fence_end(trimmed) {
            return token(NotedownTokenKind::CodeFenceEnd, line, line_no, indent, None);
        }
        if is_code_fence_start(trimmed) {
            return token(
                NotedownTokenKind::CodeFenceStart,
                line,
                line_no,
                indent,
                Some(TokenData::CodeFenceInfo(extract_code_fence_info(trimmed))),
            );
        }
        if is_div_fence_start(trimmed) {
            return token(NotedownTokenKind::DivFenceStart, line, line_no, indent, Some(TokenData::DivAttr(extract_div_fence_attr(trimmed))));
        }
        if trimmed == ":::" {
            return token(NotedownTokenKind::DivFenceEnd, line, line_no, indent, None);
        }
        if is_horizontal_rule(trimmed) {
            if is_setext_underline_candidate(&self.lines, self.line_index) && indent == 0 {
                return token(NotedownTokenKind::SetextUnderline, line, line_no, indent, None);
            }
            return token(NotedownTokenKind::HorizontalRule, line, line_no, indent, None);
        }
        if let Some((level, rest)) = parse_atx_header(trimmed) {
            return token(
                NotedownTokenKind::AtxHeader,
                line,
                line_no,
                indent,
                Some(TokenData::AtxHeader(AtxHeaderData { level, content: rest.trim().to_string() })),
            );
        }
        if let Some(content) = parse_block_quote(trimmed) {
            return token(NotedownTokenKind::BlockQuote, line, line_no, indent, Some(TokenData::Text(content)));
        }
        if let Some((content, checked)) = parse_task_list_item(trimmed) {
            return token(
                NotedownTokenKind::TaskListItem,
                line,
                line_no,
                indent,
                Some(TokenData::TaskList(TaskListItemData { content, is_checked: checked })),
            );
        }
        if let Some(content) = parse_unordered_list_item(trimmed) {
            return token(NotedownTokenKind::UnorderedListItem, line, line_no, indent, Some(TokenData::Text(content)));
        }
        if let Some((content, start)) = parse_ordered_list_item(trimmed) {
            return token(
                NotedownTokenKind::OrderedListItem,
                line,
                line_no,
                indent,
                Some(TokenData::OrderedList(OrderedListItemData { content, start_number: start })),
            );
        }
        if let Some(content) = parse_definition_description(trimmed) {
            return token(NotedownTokenKind::DefinitionDescription, line, line_no, indent, Some(TokenData::Text(content)));
        }
        if is_table_row(trimmed) {
            return token(NotedownTokenKind::TableRow, line, line_no, indent, Some(TokenData::TableCells(extract_table_cells(trimmed))));
        }
        if let Some(content) = parse_line_block(trimmed) {
            return token(NotedownTokenKind::LineBlockLine, line, line_no, indent, Some(TokenData::Text(content)));
        }
        if let Some((id, content)) = parse_footnote_definition(trimmed) {
            return token(
                NotedownTokenKind::FootnoteDefinition,
                line,
                line_no,
                indent,
                Some(TokenData::Footnote(FootnoteData { id, content })),
            );
        }
        if trimmed.trim_start().starts_with("<!--") {
            return token(NotedownTokenKind::HtmlComment, line, line_no, indent, None);
        }
        if is_definition_term(trimmed) && is_next_line_definition_description(&self.lines, self.line_index) {
            return token(NotedownTokenKind::DefinitionTerm, line, line_no, indent, None);
        }
        token(NotedownTokenKind::ParagraphText, line, line_no, indent, None)
    }
}

fn token(kind: NotedownTokenKind, text: String, line: usize, indent: usize, data: Option<TokenData>) -> NotedownToken {
    NotedownToken { kind, text, line, indent, data }
}

fn parse_atx_header(line: &str) -> Option<(u8, &str)> {
    let mut i = 0;
    let chars: Vec<char> = line.chars().collect();
    while i < chars.len() && chars[i] == '#' {
        i += 1;
    }
    if i == 0 || i > 6 {
        return None;
    }
    if i >= chars.len() || chars[i] != ' ' {
        return None;
    }
    Some((i as u8, &line[i..]))
}

fn is_setext_underline_candidate(lines: &[String], line_index: usize) -> bool {
    if line_index == 0 {
        return false;
    }
    let prev = lines[line_index - 1].trim();
    !prev.is_empty() && !is_horizontal_rule(prev) && !is_code_fence_start(prev)
}

fn is_horizontal_rule(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    let ch = trimmed.chars().next().unwrap();
    if ch != '-' && ch != '*' && ch != '_' {
        return false;
    }
    let count = trimmed.chars().filter(|&c| c == ch).count();
    trimmed.chars().all(|c| c == ch || c == ' ') && count >= 3
}

fn parse_block_quote(line: &str) -> Option<String> {
    if line.starts_with('>') { Some(line[1..].trim_start().to_string()) } else { None }
}

fn parse_unordered_list_item(line: &str) -> Option<String> {
    if line.len() < 2 {
        return None;
    }
    let mut chars = line.chars();
    let marker = chars.next()?;
    if matches!(marker, '-' | '*' | '+') && chars.next()? == ' ' { Some(line[2..].to_string()) } else { None }
}

fn parse_ordered_list_item(line: &str) -> Option<(String, i32)> {
    let dot = line.find('.')?;
    let num_str = &line[..dot];
    if num_str.is_empty() || !num_str.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let start: i32 = num_str.parse().ok()?;
    let rest = if dot + 1 < line.len() { line[dot + 1..].trim_start() } else { "" };
    Some((rest.to_string(), start))
}

fn parse_task_list_item(line: &str) -> Option<(String, bool)> {
    let content = parse_unordered_list_item(line)?;
    let trimmed = content.trim_start();
    if let Some(rest) = trimmed.strip_prefix("[ ] ") {
        return Some((rest.to_string(), false));
    }
    if let Some(rest) = trimmed.strip_prefix("[x] ").or_else(|| trimmed.strip_prefix("[X] ")) {
        return Some((rest.to_string(), true));
    }
    None
}

fn is_code_fence_start(line: &str) -> bool {
    line.starts_with("```") || line.starts_with("~~~")
}

fn is_code_fence_end(line: &str) -> bool {
    matches!(line.trim(), "```" | "~~~")
}

fn parse_definition_description(line: &str) -> Option<String> {
    if line.starts_with(": ") {
        Some(line[2..].to_string())
    }
    else if line == ":" {
        Some(String::new())
    }
    else {
        None
    }
}

fn is_definition_term(line: &str) -> bool {
    !line.is_empty()
        && !line.starts_with('#')
        && !line.starts_with('>')
        && !line.starts_with('-')
        && !line.starts_with('*')
        && !line.starts_with('+')
        && !line.starts_with('|')
        && !line.starts_with(':')
        && !line.starts_with("```")
        && !line.starts_with("~~~")
        && !line.starts_with(":::")
}

fn is_next_line_definition_description(lines: &[String], line_index: usize) -> bool {
    lines.get(line_index + 1).and_then(|l| parse_definition_description(l.trim_start())).is_some()
}

fn parse_line_block(line: &str) -> Option<String> {
    line.strip_prefix("| ").map(str::to_string)
}

fn is_table_row(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('|') && trimmed[1..].contains('|')
}

fn is_div_fence_start(line: &str) -> bool {
    line.starts_with(":::") && line.len() > 3
}

fn parse_footnote_definition(line: &str) -> Option<(String, String)> {
    if !line.starts_with("[^") {
        return None;
    }
    let close = line.find(']')?;
    if close <= 2 {
        return None;
    }
    let id = line[2..close].to_string();
    if line.len() > close + 1 && line.as_bytes().get(close + 1) == Some(&b':') {
        let content = if close + 2 < line.len() { line[close + 2..].trim_start().to_string() } else { String::new() };
        Some((id, content))
    }
    else {
        None
    }
}

fn extract_code_fence_info(line: &str) -> String {
    let fence_char = line.chars().next().unwrap_or('`');
    let mut i = 0;
    for ch in line.chars() {
        if ch == fence_char {
            i += 1;
        }
        else {
            break;
        }
    }
    if i < line.len() { line[i..].trim().to_string() } else { String::new() }
}

fn extract_div_fence_attr(line: &str) -> Attr {
    let start = line.find('{');
    let end = line.rfind('}');
    match (start, end) {
        (Some(s), Some(e)) if e > s => parse_attr(&line[s + 1..e]),
        _ => Attr::empty(),
    }
}

fn extract_table_cells(line: &str) -> Vec<String> {
    let mut trimmed = line.trim();
    if trimmed.starts_with('|') {
        trimmed = &trimmed[1..];
    }
    if trimmed.ends_with('|') {
        trimmed = &trimmed[..trimmed.len() - 1];
    }
    trimmed.split('|').map(|c| c.trim().to_string()).collect()
}

/// 解析 `{#id .class key=value}` 属性串。
pub fn parse_attr(attr_str: &str) -> Attr {
    let mut id = String::new();
    let mut classes = Vec::new();
    let mut key_values = Vec::new();
    for part in attr_str.split_whitespace() {
        if let Some(rest) = part.strip_prefix('#') {
            if !rest.is_empty() {
                id = rest.to_string();
            }
        }
        else if let Some(rest) = part.strip_prefix('.') {
            if !rest.is_empty() {
                classes.push(rest.to_string());
            }
        }
        else if let Some((key, value)) = part.split_once('=') {
            key_values.push((key.to_string(), value.trim_matches('"').to_string()));
        }
    }
    Attr { id, classes, key_values }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_atx_header_and_code_fence() {
        let tokens = NotedownLexer::new("# Title\n\n```rust\nfn main() {}\n```").tokenize();
        assert!(tokens.iter().any(|t| matches!(t.kind, NotedownTokenKind::AtxHeader)));
        assert!(tokens.iter().any(|t| matches!(t.kind, NotedownTokenKind::CodeFenceStart)));
        assert!(tokens.iter().any(|t| matches!(t.kind, NotedownTokenKind::CodeFenceEnd)));
    }

    #[test]
    fn parse_attr_extracts_id_and_class() {
        let attr = parse_attr("#intro .warning .rust");
        assert_eq!(attr.id, "intro");
        assert!(attr.classes.contains(&"warning".to_string()));
        assert!(attr.classes.contains(&"rust".to_string()));
        assert_eq!(attr.language(), "warning");
    }
}
