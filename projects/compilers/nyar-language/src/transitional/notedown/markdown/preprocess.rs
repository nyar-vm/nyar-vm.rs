//! Markdown 预处理：将 GFM / Valkyrie 扩展转为 Notedown 语法。

/// 将 Markdown 扩展语法转为 Notedown 可识别形式。
pub fn preprocess_markdown(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '=' && chars.peek() == Some(&'=') {
            if let Some(converted) = convert_highlight(&mut chars) {
                out.push_str(&converted);
                continue;
            }
        }
        out.push(c);
    }
    out
}

fn convert_highlight(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<String> {
    chars.next(); // second '='
    let mut inner = String::new();
    while let Some(c) = chars.next() {
        if c == '=' && chars.peek() == Some(&'=') {
            chars.next();
            return Some(format!("[.highlight]{inner}[/]"));
        }
        inner.push(c);
    }
    Some(format!("=={inner}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_highlight_markers() {
        assert_eq!(preprocess_markdown("==foo=="), "[.highlight]foo[/]");
    }

    #[test]
    fn leaves_unclosed_highlight() {
        assert_eq!(preprocess_markdown("==foo"), "==foo");
    }
}
