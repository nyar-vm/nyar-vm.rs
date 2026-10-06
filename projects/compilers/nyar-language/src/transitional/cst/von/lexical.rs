//! 共享的词法规则，供 lexer 与 format 使用。

/// 标识符首字符。
pub fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

/// 标识符后续字符。
pub fn is_identifier_continue(ch: char) -> bool {
    is_identifier_start(ch) || ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '@'
}

/// 解码 VON 字符串字面量（不含外围引号）。
pub fn decode_string_literal(source: &str) -> Result<String, String> {
    let mut buffer = String::new();
    let mut chars = source.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let escaped = chars.next().ok_or_else(|| "unterminated string escape".to_string())?;
            match escaped {
                '"' => buffer.push('"'),
                '\\' => buffer.push('\\'),
                'n' => buffer.push('\n'),
                'r' => buffer.push('\r'),
                't' => buffer.push('\t'),
                other => buffer.push(other),
            }
        }
        else {
            buffer.push(ch);
        }
    }
    Ok(buffer)
}
