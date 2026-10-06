//! Notedown 行内解析。

use super::super::syntax::{Attr, MathType, NotedownInline, QuoteType, Target};

/// 解析行内内容为 NotedownInline 序列。
pub fn parse_inlines(text: &str) -> Vec<NotedownInline> {
    let mut inlines = Vec::new();
    if text.is_empty() {
        return inlines;
    }
    let mut i = 0;
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();

    let mut flush = |current: &mut String, inlines: &mut Vec<NotedownInline>| {
        if !current.is_empty() {
            inlines.push(NotedownInline::Str(std::mem::take(current)));
        }
    };

    while i < chars.len() {
        let ch = chars[i];
        if ch == '\\' && i + 1 < chars.len() {
            current.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if ch == '*' || ch == '_' {
            if let Some((inline, next)) = try_delimiter_run(&chars, i, ch) {
                flush(&mut current, &mut inlines);
                inlines.push(inline);
                i = next;
                continue;
            }
        }
        if ch == '`' {
            if let Some((inline, next)) = try_code_span(&chars, i) {
                flush(&mut current, &mut inlines);
                inlines.push(inline);
                i = next;
                continue;
            }
        }
        if ch == '$' {
            if let Some((inline, next)) = try_math(&chars, i) {
                flush(&mut current, &mut inlines);
                inlines.push(inline);
                i = next;
                continue;
            }
        }
        if ch == '~' && i + 1 < chars.len() && chars[i + 1] == '~' {
            if let Some((inline, next)) = try_wrapping(&chars, i, "~~", |c| NotedownInline::Strikeout(c)) {
                flush(&mut current, &mut inlines);
                inlines.push(inline);
                i = next;
                continue;
            }
        }
        if ch == '^' {
            if let Some((inline, next)) = try_wrapping(&chars, i, "^", |c| NotedownInline::Superscript(c)) {
                flush(&mut current, &mut inlines);
                inlines.push(inline);
                i = next;
                continue;
            }
        }
        if ch == '~' {
            if let Some((inline, next)) = try_wrapping(&chars, i, "~", |c| NotedownInline::Subscript(c)) {
                flush(&mut current, &mut inlines);
                inlines.push(inline);
                i = next;
                continue;
            }
        }
        if ch == '[' {
            if let Some((parsed, next)) = try_bracket(&chars, i) {
                flush(&mut current, &mut inlines);
                inlines.extend(parsed);
                i = next;
                continue;
            }
        }
        current.push(ch);
        i += 1;
    }
    flush(&mut current, &mut inlines);
    normalize_spaces(inlines)
}

fn normalize_spaces(mut inlines: Vec<NotedownInline>) -> Vec<NotedownInline> {
    let mut out = Vec::new();
    for inline in inlines {
        if matches!(inline, NotedownInline::Str(ref s) if s.is_empty()) {
            continue;
        }
        if let NotedownInline::Str(s) = &inline {
            if s.chars().all(char::is_whitespace) {
                out.push(NotedownInline::Space);
                continue;
            }
        }
        out.push(inline);
    }
    out
}

fn try_delimiter_run(chars: &[char], start: usize, delim: char) -> Option<(NotedownInline, usize)> {
    let open_count = count_leading(chars, start, delim);
    if open_count == 0 {
        return None;
    }
    let close = find_closing_delim(chars, start + open_count, delim, open_count)?;
    let inner: String = chars[start + open_count..close].iter().collect();
    let content = parse_inlines(&inner);
    let inline = if open_count >= 2 { NotedownInline::Strong(content) } else { NotedownInline::Emph(content) };
    Some((inline, close + open_count))
}

fn try_code_span(chars: &[char], start: usize) -> Option<(NotedownInline, usize)> {
    let open = count_leading(chars, start, '`');
    if open == 0 {
        return None;
    }
    let close = find_closing_backticks(chars, start + open, open)?;
    let code: String = chars[start + open..close].iter().collect();
    Some((NotedownInline::Code(code.trim().to_string()), close + open))
}

fn try_math(chars: &[char], start: usize) -> Option<(NotedownInline, usize)> {
    if start + 1 >= chars.len() {
        return None;
    }
    if chars[start + 1] == '$' {
        let mut i = start + 2;
        while i + 1 < chars.len() {
            if chars[i] == '$' && chars[i + 1] == '$' {
                let content: String = chars[start + 2..i].iter().collect();
                return Some((NotedownInline::Math { math_type: MathType::Display, content: content.trim().to_string() }, i + 2));
            }
            i += 1;
        }
        return None;
    }
    let mut i = start + 1;
    while i < chars.len() {
        if chars[i] == '$' {
            let content: String = chars[start + 1..i].iter().collect();
            return Some((NotedownInline::Math { math_type: MathType::Inline, content: content.trim().to_string() }, i + 1));
        }
        i += 1;
    }
    None
}

fn try_wrapping<F>(chars: &[char], start: usize, marker: &str, wrap: F) -> Option<(NotedownInline, usize)>
where
    F: FnOnce(Vec<NotedownInline>) -> NotedownInline,
{
    let mchars: Vec<char> = marker.chars().collect();
    if start + mchars.len() > chars.len() {
        return None;
    }
    for (offset, &mc) in mchars.iter().enumerate() {
        if chars[start + offset] != mc {
            return None;
        }
    }
    let mut i = start + mchars.len();
    while i + mchars.len() <= chars.len() {
        if chars[i..i + mchars.len()] == mchars {
            let inner: String = chars[start + mchars.len()..i].iter().collect();
            return Some((wrap(parse_inlines(&inner)), i + mchars.len()));
        }
        i += 1;
    }
    None
}

fn try_bracket(chars: &[char], start: usize) -> Option<(Vec<NotedownInline>, usize)> {
    if start + 1 < chars.len() && chars[start + 1] == '@' {
        return try_citation(chars, start);
    }
    if start + 1 < chars.len() && chars[start + 1] == '.' {
        return try_span(chars, start);
    }
    try_link(chars, start)
}

fn try_citation(chars: &[char], start: usize) -> Option<(Vec<NotedownInline>, usize)> {
    let close = find_char(chars, ']', start + 2)?;
    let inner: String = chars[start + 2..close].iter().collect();
    let citations: Vec<String> = inner.split(';').map(|s| s.trim().trim_start_matches('@').to_string()).filter(|s| !s.is_empty()).collect();
    Some((vec![NotedownInline::Cite { citations }], close + 1))
}

fn try_span(chars: &[char], start: usize) -> Option<(Vec<NotedownInline>, usize)> {
    let close = find_char(chars, ']', start + 1)?;
    let inner: String = chars[start + 1..close].iter().collect();
    if let Some(rest) = inner.strip_prefix(".") {
        let (attr_part, content_part) = split_span_attr(rest);
        let attr = super::super::lexer::parse_attr(attr_part.trim());
        let content = parse_inlines(content_part);
        return Some((vec![NotedownInline::Span { attr, content }], close + 1));
    }
    None
}

fn split_span_attr(rest: &str) -> (&str, &str) {
    if let Some(idx) = rest.find(']') {
        // [.class]content handled elsewhere; simple `.highlight]text`
        if let Some(end) = rest.find("]") {
            let (attr, content) = rest.split_at(end);
            return (attr.trim_start_matches('.'), content.trim_start_matches(']'));
        }
    }
    if let Some(space) = rest.find(' ') { (&rest[..space], rest[space + 1..].trim_start()) } else { (rest, "") }
}

fn try_link(chars: &[char], start: usize) -> Option<(Vec<NotedownInline>, usize)> {
    let close_bracket = find_char(chars, ']', start + 1)?;
    let link_text: String = chars[start + 1..close_bracket].iter().collect();
    if close_bracket + 1 >= chars.len() || chars[close_bracket + 1] != '(' {
        return None;
    }
    let close_paren = find_char(chars, ')', close_bracket + 2)?;
    let target_str: String = chars[close_bracket + 2..close_paren].iter().collect();
    let (url, title) = split_link_target(&target_str);
    let content = parse_inlines(&link_text);
    let inline = NotedownInline::Link { attr: Attr::empty(), content, target: Target { url, title } };
    Some((vec![inline], close_paren + 1))
}

fn split_link_target(target: &str) -> (String, String) {
    if let Some(idx) = target.find(" \"") {
        let url = target[..idx].trim().to_string();
        let rest = &target[idx + 2..];
        let title = rest.trim_end_matches('"').to_string();
        (url, title)
    }
    else {
        (target.trim().to_string(), String::new())
    }
}

fn count_leading(chars: &[char], start: usize, ch: char) -> usize {
    let mut count = 0;
    let mut i = start;
    while i < chars.len() && chars[i] == ch {
        count += 1;
        i += 1;
    }
    count
}

fn find_closing_delim(chars: &[char], from: usize, delim: char, count: usize) -> Option<usize> {
    let mut i = from;
    while i + count <= chars.len() {
        if chars[i..].iter().take(count).all(|&c| c == delim) {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_closing_backticks(chars: &[char], from: usize, count: usize) -> Option<usize> {
    find_closing_delim(chars, from, '`', count)
}

fn find_char(chars: &[char], ch: char, from: usize) -> Option<usize> {
    for i in from..chars.len() {
        if chars[i] == ch {
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_emphasis_and_code() {
        let inlines = parse_inlines("*hello* and `code`");
        assert!(inlines.iter().any(|i| matches!(i, NotedownInline::Emph(_))));
        assert!(inlines.iter().any(|i| matches!(i, NotedownInline::Code(_))));
    }

    #[test]
    fn parses_math_inline_and_display() {
        let inline = parse_inlines("x = $a+b$");
        assert!(inline.iter().any(|i| matches!(i, NotedownInline::Math { math_type: MathType::Inline, .. })));
        let display = parse_inlines("$$E=mc^2$$");
        assert!(display.iter().any(|i| matches!(i, NotedownInline::Math { math_type: MathType::Display, .. })));
    }

    #[test]
    fn parses_link() {
        let inlines = parse_inlines("[docs](guide.md)");
        assert!(inlines.iter().any(|i| matches!(i, NotedownInline::Link { .. })));
    }
}
