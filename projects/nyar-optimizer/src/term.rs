//! 结构化代数项，供 `E-Graph` 吸收与抽取。

use nyar_types::{Identifier, QualifiedName};
use std::fmt;

/// 可进入 `E-Graph` 的结构化代数项。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AlgebraicTerm {
    /// 整数常量。
    Literal(i64),
    /// 已绑定的符号引用。
    Symbol(QualifiedName),
    /// 带参数的操作应用。
    Apply { operator: QualifiedName, arguments: Vec<AlgebraicTerm> },
}

impl AlgebraicTerm {
    /// 构造整数常量项。
    pub fn literal(value: i64) -> Self {
        Self::Literal(value)
    }

    /// 构造符号项。
    pub fn symbol(name: QualifiedName) -> Self {
        Self::Symbol(name)
    }

    /// 构造操作应用项。
    pub fn apply(operator: QualifiedName, arguments: Vec<AlgebraicTerm>) -> Self {
        Self::Apply { operator, arguments }
    }

    /// 若该项是整数常量，则返回值。
    pub fn as_literal(&self) -> Option<i64> {
        match self {
            Self::Literal(value) => Some(*value),
            _ => None,
        }
    }
}

impl fmt::Display for AlgebraicTerm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Literal(value) => write!(f, "{value}"),
            Self::Symbol(name) => write!(f, "{name}"),
            Self::Apply { operator, arguments } => {
                write!(f, "{operator}(")?;
                for (index, argument) in arguments.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{argument}")?;
                }
                write!(f, ")")
            }
        }
    }
}

/// 解析结构化项文本。
///
/// 支持：
/// - 整数常量：`42`、`-7`
/// - 符号：`core.add`、`graphic.dot`
/// - 应用：`core.add(1, 2)`、`core.mul(core.add(1, 2), 3)`
pub fn parse_term(input: &str) -> Result<AlgebraicTerm, String> {
    let tokens = tokenize(input)?;
    let mut parser = TermParser::new(tokens);
    let term = parser.parse_term()?;
    if parser.has_remaining() {
        return Err(format!("unexpected trailing input near `{}`", parser.peek()));
    }
    Ok(term)
}

fn qualified_from_dotted(value: &str) -> QualifiedName {
    QualifiedName::new(value.split('.').map(Identifier::new).collect())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Integer(i64),
    Identifier(String),
    Dot,
    LParen,
    RParen,
    Comma,
}

fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.peek().copied() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }
        match ch {
            '(' => {
                chars.next();
                tokens.push(Token::LParen);
            }
            ')' => {
                chars.next();
                tokens.push(Token::RParen);
            }
            ',' => {
                chars.next();
                tokens.push(Token::Comma);
            }
            '.' => {
                chars.next();
                tokens.push(Token::Dot);
            }
            '-' | '0'..='9' => {
                let mut text = String::new();
                if ch == '-' {
                    text.push(ch);
                    chars.next();
                }
                while chars.peek().copied().is_some_and(|next| next.is_ascii_digit()) {
                    text.push(chars.next().expect("digit"));
                }
                let value = text.parse::<i64>().map_err(|error| error.to_string())?;
                tokens.push(Token::Integer(value));
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let mut text = String::new();
                while chars.peek().copied().is_some_and(|next| next.is_ascii_alphanumeric() || next == '_') {
                    text.push(chars.next().expect("identifier"));
                }
                tokens.push(Token::Identifier(text));
            }
            other => return Err(format!("unexpected character `{other}`")),
        }
    }
    Ok(tokens)
}

struct TermParser {
    tokens: Vec<Token>,
    index: usize,
}

impl TermParser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, index: 0 }
    }

    fn has_remaining(&self) -> bool {
        self.index < self.tokens.len()
    }

    fn peek(&self) -> String {
        self.tokens.get(self.index).cloned().map(|token| format!("{token:?}")).unwrap_or_else(|| "<eof>".to_string())
    }

    fn parse_term(&mut self) -> Result<AlgebraicTerm, String> {
        match self.tokens.get(self.index).cloned() {
            Some(Token::Integer(value)) => {
                self.index += 1;
                Ok(AlgebraicTerm::Literal(value))
            }
            Some(Token::Identifier(start)) => {
                self.index += 1;
                let mut name = start;
                while self.tokens.get(self.index) == Some(&Token::Dot) {
                    self.index += 1;
                    let segment = self.expect_identifier("qualified name segment")?;
                    name.push('.');
                    name.push_str(&segment);
                }
                let operator = qualified_from_dotted(&name);
                if self.tokens.get(self.index) == Some(&Token::LParen) {
                    self.index += 1;
                    let mut arguments = Vec::new();
                    if self.tokens.get(self.index) != Some(&Token::RParen) {
                        loop {
                            arguments.push(self.parse_term()?);
                            match self.tokens.get(self.index).cloned() {
                                Some(Token::Comma) => self.index += 1,
                                Some(Token::RParen) => break,
                                _ => return Err("expected `,` or `)` in argument list".to_string()),
                            }
                        }
                    }
                    self.expect_token(Token::RParen, "`)`")?;
                    Ok(AlgebraicTerm::Apply { operator, arguments })
                }
                else {
                    Ok(AlgebraicTerm::Symbol(operator))
                }
            }
            _ => Err("expected integer literal or identifier".to_string()),
        }
    }

    fn expect_identifier(&mut self, context: &str) -> Result<String, String> {
        match self.tokens.get(self.index).cloned() {
            Some(Token::Identifier(text)) => {
                self.index += 1;
                Ok(text)
            }
            _ => Err(format!("expected identifier for {context}")),
        }
    }

    fn expect_token(&mut self, expected: Token, context: &str) -> Result<(), String> {
        if self.tokens.get(self.index) == Some(&expected) {
            self.index += 1;
            Ok(())
        }
        else {
            Err(format!("expected {context}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_literal_and_symbol() {
        assert_eq!(parse_term("42").unwrap(), AlgebraicTerm::literal(42));
        assert_eq!(parse_term("graphic.dot").unwrap(), AlgebraicTerm::symbol(qualified_from_dotted("graphic.dot")));
    }

    #[test]
    fn parse_nested_application() {
        let term = parse_term("core.add(core.mul(2, 3), 4)").unwrap();
        assert!(matches!(term, AlgebraicTerm::Apply { .. }));
    }
}
