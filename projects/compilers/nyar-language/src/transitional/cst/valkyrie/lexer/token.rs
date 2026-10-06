//! 词法记号定义。

use std::ops::Range;

/// 词法记号类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// 普通标识符。
    Identifier,
    /// 关键字。
    Keyword(Keyword),
    /// 字符串字面量。
    StringLiteral,
    /// 整数字面量。
    IntegerLiteral,
    /// 浮点数字面量。
    FloatLiteral,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `⁅`
    LOffsetBracket,
    /// `⁆`
    ROffsetBracket,
    /// `<`
    LAngle,
    /// `>`
    RAngle,
    /// `,`
    Comma,
    /// `:`
    Colon,
    /// `'`
    Apostrophe,
    /// `;`
    Semicolon,
    /// `=`
    Equal,
    /// `=>`
    FatArrow,
    /// `->`
    Arrow,
    /// `<-`，模式绑定运算符（用于 `whole <- subpat`）。
    LArrow,
    /// `::`
    DoubleColon,
    /// `.`
    Dot,
    /// `..`
    DotDot,
    /// `..=`
    DotDotEq,
    /// `..<`
    DotDotLt,
    /// `...`
    Ellipsis,
    /// `+=`
    PlusEqual,
    /// `-=`
    MinusEqual,
    /// `*=`
    StarEqual,
    /// `/=`
    SlashEqual,
    /// `%=`
    PercentEqual,
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `%`
    Percent,
    /// `!`
    Bang,
    /// `&&`
    AndAnd,
    /// `||`
    OrOr,
    /// `==`
    EqEq,
    /// `!=`
    NotEq,
    /// `<=`
    LessEq,
    /// `>=`
    GreaterEq,
    /// 反引号包裹的符号字面量（如 `` `+=` ``、`` `!` ``）。
    ///
    /// 用于运算符重载定义，文本通过 `span` 从源码中提取。
    BacktickSymbol,
    /// `?`
    Question,
    /// `?.`
    QuestionDot,
    /// `|`
    Pipe,
    /// `|>`，管道操作符，将左侧表达式的值作为右侧函数的参数。
    PipeGt,
    /// `&`
    Ampersand,
    /// `~`，按位取反运算符。
    Tilde,
    /// `^`，按位异或运算符。
    Caret,
    /// `<<`，左移位运算符。
    Shl,
    /// `>>`，右移位运算符。
    Shr,
    /// `@`，编译器指令前缀。
    At,
    /// `◇`
    HollowDiamond,
    /// `◆`
    SolidDiamond,
    /// 行注释（`#` / `⍝`）。
    LineComment,
    /// 空白（含换行）。
    Whitespace,
    /// 模板指令 `<% ... %>`（源码级 trivia）。
    TemplateDirective,
    /// 文件结束。
    Eof,
}

impl TokenKind {
    /// 是否为 trivia（不参与语法解析）。
    pub fn is_trivia(self) -> bool {
        matches!(self, Self::LineComment | Self::Whitespace | Self::TemplateDirective)
    }
}

/// 关键字类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    /// `namespace`
    Namespace,
    /// `using`
    Using,
    /// `micro`
    Micro,
    /// `class`
    Class,
    /// `structure`
    Structure,
    /// `trait`
    Trait,
    /// `imply`
    Imply,
    /// `unite`
    Unite,
    /// `type`
    Type,
    /// `const`
    Const,
    /// `mut`
    Mut,
    /// `ref`
    Ref,
    /// `own`
    Own,
    /// `where`
    Where,
    /// `in`
    In,
    /// `as`
    As,
    /// `true`
    True,
    /// `false`
    False,
    /// `return`
    Return,
    /// `break`
    Break,
    /// `continue`
    Continue,
    /// `yield`
    Yield,
    /// `raise`
    Raise,
    /// `resume`
    Resume,
    /// `catch`
    Catch,
    /// `if`
    If,
    /// `else`
    Else,
    /// `loop`
    Loop,
    /// `while`
    While,
    /// `match`
    Match,
    /// `case`
    Case,
    /// `fallthrough`
    Fallthrough,
    /// `let`
    Let,
    /// `lazy`
    Lazy,
    /// `null`
    Null,
    /// `widget`
    Widget,
    /// `singleton`
    Singleton,
    /// `mezzo`（类型函数）
    Mezzo,
    /// `macro`
    Macro,
    /// `neural`
    Neural,
    /// `flags`
    Flags,
    /// `union`
    Union,
    /// `enums` / `eidos`
    Enums,
    /// `until`
    Until,
    /// `try`
    Try,
    /// `when`
    When,
    /// `assert`
    Assert,
    /// `scope`
    Scope,
    /// `tests`
    Tests,
    /// `constructor`
    Constructor,
    /// `not`
    Not,
    /// `is`
    Is,
    /// `nil`
    Nil,
    /// `self`
    KwSelf,
    /// `Self`
    KwSelfType,
    /// `Some`
    KwSome,
    /// `None`
    KwNone,
}

impl Keyword {
    /// 由源码文本映射为关键字。
    pub fn from_str(text: &str) -> Option<Self> {
        Some(match text {
            "namespace" => Self::Namespace,
            "using" => Self::Using,
            "micro" | "function" | "func" | "fun" | "fn" => Self::Micro,
            "class" => Self::Class,
            "structure" | "struct" => Self::Structure,
            "trait" => Self::Trait,
            "imply" => Self::Imply,
            "unite" => Self::Unite,
            "type" | "typus" => Self::Type,
            "const" => Self::Const,
            "mut" => Self::Mut,
            "ref" => Self::Ref,
            "own" => Self::Own,
            "where" => Self::Where,
            "in" => Self::In,
            "as" => Self::As,
            "true" => Self::True,
            "false" => Self::False,
            "return" => Self::Return,
            "break" => Self::Break,
            "continue" => Self::Continue,
            "yield" => Self::Yield,
            "raise" => Self::Raise,
            "resume" => Self::Resume,
            "catch" => Self::Catch,
            "if" => Self::If,
            "else" => Self::Else,
            "loop" | "for" => Self::Loop,
            "while" => Self::While,
            "match" => Self::Match,
            "case" => Self::Case,
            "fallthrough" => Self::Fallthrough,
            "let" => Self::Let,
            "lazy" => Self::Lazy,
            "null" => Self::Null,
            "widget" => Self::Widget,
            "singleton" => Self::Singleton,
            "mezzo" => Self::Mezzo,
            "macro" => Self::Macro,
            "neural" => Self::Neural,
            "flags" => Self::Flags,
            "union" => Self::Union,
            "enums" | "eidos" => Self::Enums,
            "until" => Self::Until,
            "try" => Self::Try,
            "when" => Self::When,
            "assert" => Self::Assert,
            "scope" => Self::Scope,
            "tests" => Self::Tests,
            "constructor" => Self::Constructor,
            "not" => Self::Not,
            "is" => Self::Is,
            "nil" => Self::Nil,
            "self" => Self::KwSelf,
            "Self" => Self::KwSelfType,
            "Some" => Self::KwSome,
            "None" => Self::KwNone,
            _ => return None,
        })
    }

    /// 以源码文本形式返回关键字。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Namespace => "namespace",
            Self::Using => "using",
            Self::Micro => "micro",
            Self::Class => "class",
            Self::Structure => "structure",
            Self::Trait => "trait",
            Self::Imply => "imply",
            Self::Unite => "unite",
            Self::Type => "type",
            Self::Const => "const",
            Self::Mut => "mut",
            Self::Ref => "ref",
            Self::Own => "own",
            Self::Where => "where",
            Self::In => "in",
            Self::As => "as",
            Self::True => "true",
            Self::False => "false",
            Self::Return => "return",
            Self::Break => "break",
            Self::Continue => "continue",
            Self::Yield => "yield",
            Self::Raise => "raise",
            Self::Resume => "resume",
            Self::Catch => "catch",
            Self::If => "if",
            Self::Else => "else",
            Self::Loop => "loop",
            Self::While => "while",
            Self::Match => "match",
            Self::Case => "case",
            Self::Fallthrough => "fallthrough",
            Self::Let => "let",
            Self::Lazy => "lazy",
            Self::Null => "null",
            Self::Widget => "widget",
            Self::Singleton => "singleton",
            Self::Mezzo => "mezzo",
            Self::Macro => "macro",
            Self::Neural => "neural",
            Self::Flags => "flags",
            Self::Union => "union",
            Self::Enums => "enums",
            Self::Until => "until",
            Self::Try => "try",
            Self::When => "when",
            Self::Assert => "assert",
            Self::Scope => "scope",
            Self::Tests => "tests",
            Self::Constructor => "constructor",
            Self::Not => "not",
            Self::Is => "is",
            Self::Nil => "nil",
            Self::KwSelf => "self",
            Self::KwSelfType => "Self",
            Self::KwSome => "Some",
            Self::KwNone => "None",
        }
    }
}

/// 词法记号。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// 记号类型。
    pub kind: TokenKind,
    /// 在源文本中的字节范围。
    pub span: Range<usize>,
}
