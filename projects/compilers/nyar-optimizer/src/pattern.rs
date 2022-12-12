//! 模式匹配与替换实例化。

use crate::term::AlgebraicTerm;
use nyar_types::{Identifier, QualifiedName};
use std::collections::HashMap;

/// 可进入 `E-Graph` 的模式项。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TermPattern {
    /// 模式变量，例如 `?x`。
    Variable(Identifier),
    /// 整数常量模式。
    Literal(i64),
    /// 符号模式。
    Symbol(QualifiedName),
    /// 操作应用模式。
    Apply { operator: QualifiedName, arguments: Vec<TermPattern> },
}

impl TermPattern {
    /// 从结构化项构造精确模式。
    pub fn exact(term: &AlgebraicTerm) -> Self {
        match term {
            AlgebraicTerm::Literal(value) => Self::Literal(*value),
            AlgebraicTerm::Symbol(name) => Self::Symbol(name.clone()),
            AlgebraicTerm::Apply { operator, arguments } => {
                Self::Apply { operator: operator.clone(), arguments: arguments.iter().map(Self::exact).collect() }
            }
        }
    }
}

/// 将 flat 限定名视为原子模式。
pub fn atom_pattern(name: QualifiedName) -> TermPattern {
    TermPattern::Symbol(name)
}

/// 模式变量绑定。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Substitution {
    bindings: HashMap<Identifier, MatchValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MatchValue {
    Literal(i64),
    EClass(usize),
}

impl Substitution {
    /// 绑定一个模式变量。
    pub fn bind(&mut self, variable: Identifier, value: MatchValue) -> bool {
        match self.bindings.get(&variable) {
            Some(existing) if *existing != value => false,
            _ => {
                self.bindings.insert(variable, value);
                true
            }
        }
    }

    /// 读取变量绑定的等价类。
    pub fn class_for(&self, variable: &Identifier) -> Option<usize> {
        match self.bindings.get(variable)? {
            MatchValue::EClass(class) => Some(*class),
            MatchValue::Literal(_) => None,
        }
    }

    /// 读取变量绑定的字面量。
    pub fn literal_for(&self, variable: &Identifier) -> Option<i64> {
        match self.bindings.get(variable)? {
            MatchValue::Literal(value) => Some(*value),
            MatchValue::EClass(_) => None,
        }
    }
}

/// 在 `E-Graph` 宿主上匹配模式所需的只读视图。
pub trait EGraphMatchView {
    /// 解析等价类代表元。
    fn find(&self, class: usize) -> usize;

    /// 返回等价类中的全部 `E-Node` 索引。
    fn enodes_in_class(&self, class: usize) -> Vec<usize>;

    /// 读取 `E-Node` 字面量。
    fn enode_literal(&self, enode: usize) -> Option<i64>;

    /// 读取 `E-Node` 符号。
    fn enode_symbol(&self, enode: usize) -> Option<&QualifiedName>;

    /// 读取 `E-Node` 应用操作符。
    fn enode_apply(&self, enode: usize) -> Option<(&QualifiedName, &[usize])>;
}

/// 尝试将模式匹配到某个等价类。
pub fn match_pattern<V: EGraphMatchView>(view: &V, pattern: &TermPattern, class: usize) -> Option<Substitution> {
    let mut substitution = Substitution::default();
    if match_pattern_into(view, pattern, view.find(class), &mut substitution) { Some(substitution) } else { None }
}

fn match_pattern_into<V: EGraphMatchView>(view: &V, pattern: &TermPattern, class: usize, substitution: &mut Substitution) -> bool {
    let canonical = view.find(class);
    match pattern {
        TermPattern::Variable(name) => substitution.bind(name.clone(), MatchValue::EClass(canonical)),
        TermPattern::Literal(expected) => view.enodes_in_class(canonical).into_iter().any(|enode| view.enode_literal(enode) == Some(*expected)),
        TermPattern::Symbol(expected) => {
            view.enodes_in_class(canonical).into_iter().any(|enode| view.enode_symbol(enode).is_some_and(|symbol| symbol == expected))
        }
        TermPattern::Apply { operator, arguments } => view.enodes_in_class(canonical).into_iter().any(|enode| {
            let Some((found_operator, children)) = view.enode_apply(enode)
            else {
                return false;
            };
            if found_operator != operator || children.len() != arguments.len() {
                return false;
            }
            arguments.iter().zip(children.iter()).all(|(argument, child)| match_pattern_into(view, argument, *child, substitution))
        }),
    }
}

/// 将模式实例化为结构化项，供重新插入 `E-Graph`。
pub fn instantiate_pattern(substitution: &Substitution, pattern: &TermPattern) -> Option<AlgebraicTerm> {
    match pattern {
        TermPattern::Variable(name) => {
            if let Some(value) = substitution.literal_for(name) {
                Some(AlgebraicTerm::Literal(value))
            }
            else {
                substitution.class_for(name).map(|_| AlgebraicTerm::Symbol(QualifiedName::new(vec![name.clone()])))
            }
        }
        TermPattern::Literal(value) => Some(AlgebraicTerm::Literal(*value)),
        TermPattern::Symbol(name) => Some(AlgebraicTerm::Symbol(name.clone())),
        TermPattern::Apply { operator, arguments } => {
            let mut resolved = Vec::with_capacity(arguments.len());
            for argument in arguments {
                resolved.push(instantiate_pattern(substitution, argument)?);
            }
            Some(AlgebraicTerm::Apply { operator: operator.clone(), arguments: resolved })
        }
    }
}

/// 解析模式文本。
///
/// 变量使用 `?name` 前缀，其余语法与 [`crate::term::parse_term`] 相同。
pub fn parse_pattern(input: &str) -> Result<TermPattern, String> {
    let tokens = tokenize_pattern(input)?;
    let mut parser = PatternParser::new(tokens);
    let pattern = parser.parse_pattern()?;
    if parser.has_remaining() {
        return Err(format!("unexpected trailing input near `{}`", parser.peek()));
    }
    Ok(pattern)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PatternToken {
    Variable(String),
    Integer(i64),
    Identifier(String),
    Dot,
    LParen,
    RParen,
    Comma,
}

fn tokenize_pattern(input: &str) -> Result<Vec<PatternToken>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.peek().copied() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }
        match ch {
            '?' => {
                chars.next();
                let mut name = String::new();
                while chars.peek().copied().is_some_and(|next| next.is_ascii_alphanumeric() || next == '_') {
                    name.push(chars.next().expect("variable name"));
                }
                if name.is_empty() {
                    return Err("pattern variable name must not be empty".to_string());
                }
                tokens.push(PatternToken::Variable(name));
            }
            '(' => {
                chars.next();
                tokens.push(PatternToken::LParen);
            }
            ')' => {
                chars.next();
                tokens.push(PatternToken::RParen);
            }
            ',' => {
                chars.next();
                tokens.push(PatternToken::Comma);
            }
            '.' => {
                chars.next();
                tokens.push(PatternToken::Dot);
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
                tokens.push(PatternToken::Integer(value));
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let mut text = String::new();
                while chars.peek().copied().is_some_and(|next| next.is_ascii_alphanumeric() || next == '_') {
                    text.push(chars.next().expect("identifier"));
                }
                tokens.push(PatternToken::Identifier(text));
            }
            other => return Err(format!("unexpected character `{other}`")),
        }
    }
    Ok(tokens)
}

struct PatternParser {
    tokens: Vec<PatternToken>,
    index: usize,
}

impl PatternParser {
    fn new(tokens: Vec<PatternToken>) -> Self {
        Self { tokens, index: 0 }
    }

    fn has_remaining(&self) -> bool {
        self.index < self.tokens.len()
    }

    fn peek(&self) -> String {
        self.tokens.get(self.index).cloned().map(|token| format!("{token:?}")).unwrap_or_else(|| "<eof>".to_string())
    }

    fn parse_pattern(&mut self) -> Result<TermPattern, String> {
        match self.tokens.get(self.index).cloned() {
            Some(PatternToken::Variable(name)) => {
                self.index += 1;
                Ok(TermPattern::Variable(Identifier::new(&name)))
            }
            Some(PatternToken::Integer(value)) => {
                self.index += 1;
                Ok(TermPattern::Literal(value))
            }
            Some(PatternToken::Identifier(start)) => {
                self.index += 1;
                let mut name = start;
                while self.tokens.get(self.index) == Some(&PatternToken::Dot) {
                    self.index += 1;
                    let segment = self.expect_identifier("qualified name segment")?;
                    name.push('.');
                    name.push_str(&segment);
                }
                let operator = QualifiedName::new(name.split('.').map(Identifier::new).collect());
                if self.tokens.get(self.index) == Some(&PatternToken::LParen) {
                    self.index += 1;
                    let mut arguments = Vec::new();
                    if self.tokens.get(self.index) != Some(&PatternToken::RParen) {
                        loop {
                            arguments.push(self.parse_pattern()?);
                            match self.tokens.get(self.index).cloned() {
                                Some(PatternToken::Comma) => self.index += 1,
                                Some(PatternToken::RParen) => break,
                                _ => return Err("expected `,` or `)` in argument list".to_string()),
                            }
                        }
                    }
                    self.expect_token(PatternToken::RParen, "`)`")?;
                    Ok(TermPattern::Apply { operator, arguments })
                }
                else {
                    Ok(TermPattern::Symbol(operator))
                }
            }
            _ => Err("expected pattern variable, integer literal, or identifier".to_string()),
        }
    }

    fn expect_identifier(&mut self, context: &str) -> Result<String, String> {
        match self.tokens.get(self.index).cloned() {
            Some(PatternToken::Identifier(text)) => {
                self.index += 1;
                Ok(text)
            }
            _ => Err(format!("expected identifier for {context}")),
        }
    }

    fn expect_token(&mut self, expected: PatternToken, context: &str) -> Result<(), String> {
        if self.tokens.get(self.index) == Some(&expected) {
            self.index += 1;
            Ok(())
        }
        else {
            Err(format!("expected {context}"))
        }
    }
}
