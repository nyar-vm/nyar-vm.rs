//! Tcl lexer: words, braces, quotes, `[...]`, newlines.

use super::ast::TclWord;

/// Tcl token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TclToken {
    /// Token text.
    pub text: String,
    /// Whether token was braced `{...}`.
    pub braced: bool,
}

impl TclToken {
    /// Convert to AST word.
    pub fn into_word(self) -> TclWord {
        TclWord { text: self.text, braced: self.braced }
    }
}

/// Tokenize into command word lists.
pub fn tokenize_words(source: &str) -> Vec<Vec<TclToken>> {
    let mut commands = Vec::new();
    let mut current = Vec::new();
    let mut chars = source.chars().peekable();
    while let Some(&ch) = chars.peek() {
        if ch == '\n' || ch == ';' {
            chars.next();
            if !current.is_empty() {
                commands.push(std::mem::take(&mut current));
            }
            continue;
        }
        if ch.is_whitespace() {
            chars.next();
            continue;
        }
        if ch == '#' && current.is_empty() {
            while let Some(&next) = chars.peek() {
                chars.next();
                if next == '\n' {
                    break;
                }
            }
            continue;
        }
        if ch == '{' {
            current.push(read_braced(&mut chars));
            continue;
        }
        if ch == '"' {
            current.push(read_quoted(&mut chars));
            continue;
        }
        current.push(read_bare_word(&mut chars));
    }
    if !current.is_empty() {
        commands.push(current);
    }
    commands
}

fn read_braced(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> TclToken {
    chars.next();
    let mut depth = 1usize;
    let mut text = String::new();
    while let Some(ch) = chars.next() {
        if ch == '{' {
            depth += 1;
            text.push(ch);
        }
        else if ch == '}' {
            depth -= 1;
            if depth == 0 {
                break;
            }
            text.push(ch);
        }
        else {
            text.push(ch);
        }
    }
    TclToken { text, braced: true }
}

fn read_quoted(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> TclToken {
    chars.next();
    let mut text = String::new();
    while let Some(ch) = chars.next() {
        if ch == '"' {
            break;
        }
        if ch == '\\' {
            if let Some(escaped) = chars.next() {
                text.push(escaped);
            }
        }
        else {
            text.push(ch);
        }
    }
    TclToken { text, braced: false }
}

fn read_bare_word(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> TclToken {
    let mut text = String::new();
    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() || ch == '\n' || ch == ';' {
            break;
        }
        if ch == '[' {
            text.push_str(&read_bracket_text(chars));
            continue;
        }
        text.push(chars.next().unwrap());
    }
    TclToken { text, braced: false }
}

fn read_bracket_text(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut text = String::from("[");
    chars.next();
    let mut depth = 1usize;
    while let Some(ch) = chars.next() {
        text.push(ch);
        if ch == '[' {
            depth += 1;
        }
        else if ch == ']' {
            depth -= 1;
            if depth == 0 {
                break;
            }
        }
        else if ch == '\\' {
            if let Some(escaped) = chars.next() {
                text.push(escaped);
            }
        }
    }
    text
}

/// Brace-aware Tcl list split.
pub fn split_tcl_list(source: &str) -> Vec<String> {
    tokenize_words(source).into_iter().flatten().map(|token| token.text).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_puts_and_set() {
        let cmds = tokenize_words("set x 1\nputs $x");
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0][0].text, "set");
    }

    #[test]
    fn tokenizes_command_substitution() {
        let cmds = tokenize_words("puts [expr {1 + 2}]");
        assert_eq!(cmds[0][1].text, "[expr {1 + 2}]");
    }
}
