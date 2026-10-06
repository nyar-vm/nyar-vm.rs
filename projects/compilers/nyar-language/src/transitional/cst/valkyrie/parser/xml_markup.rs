use crate::transitional::cst::valkyrie::{ParseError, ast::TermExpression, lexer::TokenKind, xml::parse_xgrammar_markup};

use super::Parser;

impl<'a> Parser<'a> {
    pub(super) fn peek_xml_markup_start(&self) -> bool {
        matches!(self.current().kind, TokenKind::LAngle)
            && self.tokens.get(self.index + 1).is_some_and(|token| matches!(token.kind, TokenKind::Identifier))
    }

    pub(super) fn parse_xml_markup_expression(&mut self) -> Result<TermExpression, ParseError> {
        let start = self.current().span.start;
        let slice = &self.source[start..];
        let (nodes, consumed) = parse_xgrammar_markup(slice).map_err(|error| ParseError::invalid_at(error.message, error.span))?;
        let end = start + consumed;
        self.advance_to_byte(end);
        Ok(TermExpression::XmlMarkup { nodes, span: start..end })
    }

    fn advance_to_byte(&mut self, byte: usize) {
        while self.index < self.tokens.len() && self.tokens[self.index].span.start < byte {
            self.index += 1;
        }
    }
}
