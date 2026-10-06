//! Stable binary encoding for Valkyrie token streams (cache payloads).

use super::{Keyword, Token, TokenKind};

/// Encode tokens as little-endian records.
///
/// Layout: `count:u32`, then per token `kind_tag:u32`, `keyword_tag:u32`, `start:u32`, `end:u32`.
pub fn encode_tokens(tokens: &[Token]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + tokens.len() * 16);
    out.extend_from_slice(&(tokens.len() as u32).to_le_bytes());
    for token in tokens {
        let (kind_tag, keyword_tag) = encode_kind(token.kind);
        out.extend_from_slice(&kind_tag.to_le_bytes());
        out.extend_from_slice(&keyword_tag.to_le_bytes());
        out.extend_from_slice(&(token.span.start as u32).to_le_bytes());
        out.extend_from_slice(&(token.span.end as u32).to_le_bytes());
    }
    out
}

/// Decode tokens produced by [`encode_tokens`].
pub fn decode_tokens(data: &[u8]) -> Option<Vec<Token>> {
    if data.len() < 4 {
        return None;
    }
    let count = u32::from_le_bytes(data[0..4].try_into().ok()?) as usize;
    let mut pos = 4usize;
    let mut tokens = Vec::with_capacity(count);
    for _ in 0..count {
        if pos + 16 > data.len() {
            return None;
        }
        let kind_tag = u32::from_le_bytes(data[pos..pos + 4].try_into().ok()?);
        let keyword_tag = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().ok()?);
        let start = u32::from_le_bytes(data[pos + 8..pos + 12].try_into().ok()?) as usize;
        let end = u32::from_le_bytes(data[pos + 12..pos + 16].try_into().ok()?) as usize;
        pos += 16;
        let kind = decode_kind(kind_tag, keyword_tag)?;
        tokens.push(Token { kind, span: start..end });
    }
    Some(tokens)
}

fn encode_kind(kind: TokenKind) -> (u32, u32) {
    match kind {
        TokenKind::Identifier => (1, 0),
        TokenKind::Keyword(kw) => (2, keyword_tag(kw)),
        TokenKind::StringLiteral => (3, 0),
        TokenKind::IntegerLiteral => (4, 0),
        TokenKind::FloatLiteral => (5, 0),
        TokenKind::LParen => (6, 0),
        TokenKind::RParen => (7, 0),
        TokenKind::LBrace => (8, 0),
        TokenKind::RBrace => (9, 0),
        TokenKind::LBracket => (10, 0),
        TokenKind::RBracket => (11, 0),
        TokenKind::LOffsetBracket => (12, 0),
        TokenKind::ROffsetBracket => (13, 0),
        TokenKind::LAngle => (14, 0),
        TokenKind::RAngle => (15, 0),
        TokenKind::Comma => (16, 0),
        TokenKind::Colon => (17, 0),
        TokenKind::Apostrophe => (18, 0),
        TokenKind::Semicolon => (19, 0),
        TokenKind::Equal => (20, 0),
        TokenKind::FatArrow => (21, 0),
        TokenKind::Arrow => (22, 0),
        TokenKind::LArrow => (63, 0),
        TokenKind::DoubleColon => (23, 0),
        TokenKind::Dot => (24, 0),
        TokenKind::DotDot => (25, 0),
        TokenKind::DotDotEq => (26, 0),
        TokenKind::DotDotLt => (27, 0),
        TokenKind::Ellipsis => (28, 0),
        TokenKind::PlusEqual => (29, 0),
        TokenKind::MinusEqual => (30, 0),
        TokenKind::StarEqual => (31, 0),
        TokenKind::SlashEqual => (32, 0),
        TokenKind::PercentEqual => (33, 0),
        TokenKind::Plus => (34, 0),
        TokenKind::Minus => (35, 0),
        TokenKind::Star => (36, 0),
        TokenKind::Slash => (37, 0),
        TokenKind::Percent => (38, 0),
        TokenKind::Bang => (39, 0),
        TokenKind::AndAnd => (40, 0),
        TokenKind::OrOr => (41, 0),
        TokenKind::EqEq => (42, 0),
        TokenKind::NotEq => (43, 0),
        TokenKind::LessEq => (44, 0),
        TokenKind::GreaterEq => (45, 0),
        TokenKind::BacktickSymbol => (46, 0),
        TokenKind::Question => (47, 0),
        TokenKind::QuestionDot => (48, 0),
        TokenKind::Pipe => (49, 0),
        TokenKind::PipeGt => (50, 0),
        TokenKind::Ampersand => (51, 0),
        TokenKind::Tilde => (52, 0),
        TokenKind::Caret => (53, 0),
        TokenKind::Shl => (54, 0),
        TokenKind::Shr => (55, 0),
        TokenKind::At => (56, 0),
        TokenKind::HollowDiamond => (57, 0),
        TokenKind::SolidDiamond => (58, 0),
        TokenKind::Eof => (59, 0),
        TokenKind::LineComment => (60, 0),
        TokenKind::Whitespace => (61, 0),
        TokenKind::TemplateDirective => (62, 0),
    }
}

fn decode_kind(kind_tag: u32, keyword_tag: u32) -> Option<TokenKind> {
    Some(match kind_tag {
        1 => TokenKind::Identifier,
        2 => TokenKind::Keyword(decode_keyword(keyword_tag)?),
        3 => TokenKind::StringLiteral,
        4 => TokenKind::IntegerLiteral,
        5 => TokenKind::FloatLiteral,
        6 => TokenKind::LParen,
        7 => TokenKind::RParen,
        8 => TokenKind::LBrace,
        9 => TokenKind::RBrace,
        10 => TokenKind::LBracket,
        11 => TokenKind::RBracket,
        12 => TokenKind::LOffsetBracket,
        13 => TokenKind::ROffsetBracket,
        14 => TokenKind::LAngle,
        15 => TokenKind::RAngle,
        16 => TokenKind::Comma,
        17 => TokenKind::Colon,
        18 => TokenKind::Apostrophe,
        19 => TokenKind::Semicolon,
        20 => TokenKind::Equal,
        21 => TokenKind::FatArrow,
        22 => TokenKind::Arrow,
        63 => TokenKind::LArrow,
        23 => TokenKind::DoubleColon,
        24 => TokenKind::Dot,
        25 => TokenKind::DotDot,
        26 => TokenKind::DotDotEq,
        27 => TokenKind::DotDotLt,
        28 => TokenKind::Ellipsis,
        29 => TokenKind::PlusEqual,
        30 => TokenKind::MinusEqual,
        31 => TokenKind::StarEqual,
        32 => TokenKind::SlashEqual,
        33 => TokenKind::PercentEqual,
        34 => TokenKind::Plus,
        35 => TokenKind::Minus,
        36 => TokenKind::Star,
        37 => TokenKind::Slash,
        38 => TokenKind::Percent,
        39 => TokenKind::Bang,
        40 => TokenKind::AndAnd,
        41 => TokenKind::OrOr,
        42 => TokenKind::EqEq,
        43 => TokenKind::NotEq,
        44 => TokenKind::LessEq,
        45 => TokenKind::GreaterEq,
        46 => TokenKind::BacktickSymbol,
        47 => TokenKind::Question,
        48 => TokenKind::QuestionDot,
        49 => TokenKind::Pipe,
        50 => TokenKind::PipeGt,
        51 => TokenKind::Ampersand,
        52 => TokenKind::Tilde,
        53 => TokenKind::Caret,
        54 => TokenKind::Shl,
        55 => TokenKind::Shr,
        56 => TokenKind::At,
        57 => TokenKind::HollowDiamond,
        58 => TokenKind::SolidDiamond,
        59 => TokenKind::Eof,
        60 => TokenKind::LineComment,
        61 => TokenKind::Whitespace,
        62 => TokenKind::TemplateDirective,
        _ => return None,
    })
}

fn keyword_tag(kw: Keyword) -> u32 {
    match kw {
        Keyword::Namespace => 1,
        Keyword::Using => 2,
        Keyword::Micro => 3,
        Keyword::Class => 4,
        Keyword::Structure => 5,
        Keyword::Trait => 6,
        Keyword::Imply => 7,
        Keyword::Unite => 8,
        Keyword::Type => 9,
        Keyword::Const => 10,
        Keyword::Mut => 11,
        Keyword::Ref => 12,
        Keyword::Own => 13,
        Keyword::Where => 14,
        Keyword::In => 15,
        Keyword::As => 16,
        Keyword::True => 17,
        Keyword::False => 18,
        Keyword::Return => 19,
        Keyword::Break => 20,
        Keyword::Continue => 21,
        Keyword::Yield => 22,
        Keyword::Raise => 23,
        Keyword::Resume => 24,
        Keyword::Catch => 25,
        Keyword::If => 26,
        Keyword::Else => 27,
        Keyword::Loop => 28,
        Keyword::While => 29,
        Keyword::Match => 30,
        Keyword::Case => 31,
        Keyword::Fallthrough => 32,
        Keyword::Let => 33,
        Keyword::Lazy => 34,
        Keyword::Null => 35,
        Keyword::Widget => 36,
        Keyword::Singleton => 37,
        Keyword::Mezzo => 38,
        Keyword::Macro => 39,
        Keyword::Neural => 40,
        Keyword::Flags => 41,
        Keyword::Union => 42,
        Keyword::Enums => 43,
        Keyword::Until => 44,
        Keyword::Try => 45,
        Keyword::When => 46,
        Keyword::Assert => 47,
        Keyword::Scope => 48,
        Keyword::Tests => 49,
        Keyword::Constructor => 50,
        Keyword::Not => 51,
        Keyword::Is => 52,
        Keyword::Nil => 53,
        Keyword::KwSelf => 54,
        Keyword::KwSelfType => 55,
        Keyword::KwSome => 56,
        Keyword::KwNone => 57,
    }
}

fn decode_keyword(tag: u32) -> Option<Keyword> {
    Some(match tag {
        1 => Keyword::Namespace,
        2 => Keyword::Using,
        3 => Keyword::Micro,
        4 => Keyword::Class,
        5 => Keyword::Structure,
        6 => Keyword::Trait,
        7 => Keyword::Imply,
        8 => Keyword::Unite,
        9 => Keyword::Type,
        10 => Keyword::Const,
        11 => Keyword::Mut,
        12 => Keyword::Ref,
        13 => Keyword::Own,
        14 => Keyword::Where,
        15 => Keyword::In,
        16 => Keyword::As,
        17 => Keyword::True,
        18 => Keyword::False,
        19 => Keyword::Return,
        20 => Keyword::Break,
        21 => Keyword::Continue,
        22 => Keyword::Yield,
        23 => Keyword::Raise,
        24 => Keyword::Resume,
        25 => Keyword::Catch,
        26 => Keyword::If,
        27 => Keyword::Else,
        28 => Keyword::Loop,
        29 => Keyword::While,
        30 => Keyword::Match,
        31 => Keyword::Case,
        32 => Keyword::Fallthrough,
        33 => Keyword::Let,
        34 => Keyword::Lazy,
        35 => Keyword::Null,
        36 => Keyword::Widget,
        37 => Keyword::Singleton,
        38 => Keyword::Mezzo,
        39 => Keyword::Macro,
        40 => Keyword::Neural,
        41 => Keyword::Flags,
        42 => Keyword::Union,
        43 => Keyword::Enums,
        44 => Keyword::Until,
        45 => Keyword::Try,
        46 => Keyword::When,
        47 => Keyword::Assert,
        48 => Keyword::Scope,
        49 => Keyword::Tests,
        50 => Keyword::Constructor,
        51 => Keyword::Not,
        52 => Keyword::Is,
        53 => Keyword::Nil,
        54 => Keyword::KwSelf,
        55 => Keyword::KwSelfType,
        56 => Keyword::KwSome,
        57 => Keyword::KwNone,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transitional::cst::valkyrie::lexer::Lexer;

    #[test]
    fn token_codec_round_trip() {
        let source = "micro main() -> i64 { return 0; }";
        let tokens = Lexer::tokenize(source).unwrap();
        let encoded = encode_tokens(&tokens);
        let decoded = decode_tokens(&encoded).unwrap();
        assert_eq!(decoded, tokens);
    }

    #[test]
    fn parse_tokens_matches_parse_root() {
        use crate::transitional::cst::valkyrie::parser::AstParser;
        let source = "micro main(): i64 { return 0; }";
        let tokens = Lexer::tokenize(source).unwrap();
        let via_tokens = AstParser::parse_tokens(source, tokens).unwrap();
        let via_root = AstParser::parse_root(source).unwrap();
        assert_eq!(format!("{via_tokens:?}"), format!("{via_root:?}"));
    }
}
