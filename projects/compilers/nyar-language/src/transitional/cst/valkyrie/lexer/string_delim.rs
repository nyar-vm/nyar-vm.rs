//! S-Grammar 对称引号扫描：词法层不处理转义，仅匹配等量开闭引号。

/// 允许的最大引号层数（`""""` 等）。
pub const MAX_QUOTE_COUNT: usize = 8;

/// 若 `cursor` 处为字符串开引号，返回分隔符字符与连续引号个数。
///
/// 单独的 `'` 视为标签前缀（Apostrophe），不是字符串。
pub fn opening_quote_run(source: &str, cursor: usize) -> Option<(char, usize)> {
    let delim = source.get(cursor..)?.chars().next()?;
    if delim != '"' && delim != '\'' {
        return None;
    }
    let count = consecutive_quotes(source, cursor, delim);
    if delim == '\'' && count < 2 {
        return None;
    }
    if count > MAX_QUOTE_COUNT {
        return None;
    }
    Some((delim, count))
}

/// 统计从 `cursor` 起的连续相同引号个数。
pub fn consecutive_quotes(source: &str, cursor: usize, delim: char) -> usize {
    let mut count = 0usize;
    let mut pos = cursor;
    while source.get(pos..).is_some_and(|text| text.starts_with(delim)) {
        count += 1;
        pos += delim.len_utf8();
    }
    count
}

/// 在 `body_start` 之后查找恰好 `quote_count` 个 `delim` 组成的闭引号，返回闭引号后的字节偏移。
pub fn find_closing_quote_run(source: &str, body_start: usize, delim: char, quote_count: usize) -> Option<usize> {
    let mut cursor = body_start;
    while cursor < source.len() {
        if source.get(cursor..).is_some_and(|text| text.starts_with(delim)) {
            let run = consecutive_quotes(source, cursor, delim);
            if run == quote_count {
                return Some(cursor + quote_count * delim.len_utf8());
            }
            cursor += run * delim.len_utf8();
            continue;
        }
        let ch = source[cursor..].chars().next()?;
        cursor += ch.len_utf8();
    }
    None
}

/// 跳过完整字符串字面量（含可选 `r`/`t` 前缀与开闭引号），返回后缀偏移。
pub fn skip_string_literal(source: &str, cursor: usize) -> Result<usize, String> {
    let mut pos = cursor;
    if let Some(ch) = source[pos..].chars().next() {
        if (ch == 't' || ch == 'r') && opening_quote_run(source, pos + ch.len_utf8()).is_some() {
            pos += ch.len_utf8();
        }
    }
    let (delim, quote_count) = opening_quote_run(source, pos).ok_or_else(|| "expected string opener".to_string())?;
    let body_start = pos + quote_count * delim.len_utf8();
    if quote_count == 2 {
        return Ok(body_start);
    }
    find_closing_quote_run(source, body_start, delim, quote_count).ok_or_else(|| "unterminated string literal".to_string())
}
