use crate::transitional::cst::valkyrie::{
    ast::{
        DereferenceKind, NamePath, SubscriptItem, SubscriptKind, TermArguments, TermCallExpression, TermDereferenceExpression,
        TermDotExpression, TermExpression, TermSubscriptExpression,
    },
    lexer::{Keyword, TokenKind},
};

use super::{ParseError, Parser, span};

impl<'a> Parser<'a> {
    pub(super) fn parse_raise_expression(&mut self) -> Result<TermExpression, ParseError> {
        let start = self.expect_token_keyword(Keyword::Raise)?.span.start;
        let value = Box::new(self.parse_value_expression_bp(0)?);
        let end = value.span().end;
        Ok(TermExpression::Raise { value, span: span(start, end) })
    }

    pub(super) fn parse_catch_expression(&mut self) -> Result<TermExpression, ParseError> {
        let start = self.expect_token_keyword(Keyword::Catch)?.span.start;
        self.suppress_struct_constructor = true;
        let expr = Box::new(self.parse_value_expression_bp(0)?);
        self.suppress_struct_constructor = false;
        self.expect_symbol(TokenKind::LBrace)?;

        let mut arms = Vec::new();
        while !self.check_symbol(TokenKind::RBrace) {
            if self.is_eof() {
                return Err(ParseError::invalid("unterminated catch body"));
            }
            if self.match_symbol(TokenKind::Semicolon) {
                continue;
            }
            let arm = self.parse_match_arm()?;
            arms.push(arm);
        }
        let close = self.expect_symbol(TokenKind::RBrace)?;
        Ok(TermExpression::Catch { expr, arms, span: span(start, close.span.end) })
    }

    /// 解析 lambda 表达式 `micro(params) -> return_type { body }`。
    ///
    /// 与顶层 `micro name(params) -> T { body }` 函数声明不同，lambda 作为表达式
    /// 出现在调用参数等位置时没有函数名，直接以 `micro(` 起始。
    pub(super) fn parse_lambda_expression(&mut self) -> Result<TermExpression, ParseError> {
        let start = self.expect_token_keyword(Keyword::Micro)?.span.start;
        let params = self.parse_lambda_parameter_list()?;
        let return_type = if self.match_symbol(TokenKind::Arrow) { Some(self.parse_intersection_type_expression()?) } else { None };
        let body = if self.check_symbol(TokenKind::LBrace) {
            self.parse_block_body()?
        }
        else {
            return Err(self.error_here("expected lambda body '{'"));
        };
        let end = body.span.end;
        Ok(TermExpression::Lambda { params, return_type, body: Box::new(body), span: span(start, end) })
    }

    pub(super) fn expression_postfix_binding_power(&self) -> Option<(u8, u8)> {
        match self.current().kind {
            TokenKind::Dot => Some((95, 96)),
            TokenKind::DoubleColon if self.nth_is_identifier(1) => Some((95, 96)),
            TokenKind::DoubleColon if self.nth_is_symbol(1, TokenKind::LAngle) => Some((93, 94)),
            TokenKind::DoubleColon if self.nth_is_symbol(1, TokenKind::LBracket) => Some((90, 91)),
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LOffsetBracket => Some((90, 91)),
            TokenKind::LBrace if !self.suppress_struct_constructor => Some((85, 86)),
            TokenKind::Question => Some((97, 98)),
            _ => None,
        }
    }

    pub(super) fn parse_expression_postfix(&mut self, lhs: TermExpression) -> Result<TermExpression, ParseError> {
        match self.current().kind {
            TokenKind::LParen => self.parse_call_expression(lhs),
            TokenKind::Dot => self.parse_member_expression(lhs),
            TokenKind::LBracket => self.parse_subscript_expression(lhs, SubscriptKind::Ordinal),
            TokenKind::LOffsetBracket => self.parse_offset_subscript_expression(lhs),
            TokenKind::DoubleColon if self.nth_is_symbol(1, TokenKind::LBracket) => self.parse_offset_alias_subscript_expression(lhs),
            TokenKind::DoubleColon if self.nth_is_symbol(1, TokenKind::LAngle) => self.parse_turbofish_expression(lhs),
            TokenKind::DoubleColon if self.nth_is_identifier(1) => self.parse_path_member_expression(lhs),
            TokenKind::LBrace => self.parse_construct_expression(lhs),
            TokenKind::Question => {
                let start = lhs.span().start;
                self.bump();
                let end = self.previous().span.end;
                Ok(TermExpression::TryPropagate { base: Box::new(lhs), span: span(start, end) })
            }
            _ => Err(self.error_here("expected postfix operator")),
        }
    }

    fn parse_call_expression(&mut self, callee: TermExpression) -> Result<TermExpression, ParseError> {
        let start = callee.span().start;
        self.expect_symbol(TokenKind::LParen)?;
        let args = self.parse_comma_separated_until(TokenKind::RParen, |parser| parser.parse_call_argument())?;
        let close = self.expect_symbol(TokenKind::RParen)?;
        Ok(TermExpression::Call(Box::new(TermCallExpression {
            callee,
            args: TermArguments { arguments: args },
            span: span(start, close.span.end),
        })))
    }

    fn parse_member_expression(&mut self, object: TermExpression) -> Result<TermExpression, ParseError> {
        let start = object.span().start;
        self.expect_symbol(TokenKind::Dot)?;
        if self.match_symbol(TokenKind::HollowDiamond) {
            let end = self.previous().span.end;
            return Ok(TermExpression::Dereference(Box::new(TermDereferenceExpression {
                base: object,
                kind: DereferenceKind::ReadOnly,
                span: span(start, end),
            })));
        }
        if self.match_symbol(TokenKind::SolidDiamond) {
            let end = self.previous().span.end;
            return Ok(TermExpression::Dereference(Box::new(TermDereferenceExpression {
                base: object,
                kind: DereferenceKind::Mutable,
                span: span(start, end),
            })));
        }
        let member = self.expect_member_name_text()?;
        if self.check_symbol(TokenKind::LBrace) && (member == "match" || member == "catch") {
            let (arms, end) = self.parse_postfix_match_arm_block()?;
            let span = span(start, end);
            let base = Box::new(object);
            return if member == "match" {
                Ok(TermExpression::PostfixMatch { base, arms, span })
            }
            else {
                Ok(TermExpression::PostfixCatch { base, arms, span })
            };
        }
        let end = self.previous().span.end;
        let caller = NamePath { parts: vec![member], span: span(start, end) };
        Ok(TermExpression::DotCall(Box::new(TermDotExpression {
            base: object,
            caller,
            arguments: TermArguments { arguments: vec![] },
            span: span(start, end),
        })))
    }

    fn parse_path_member_expression(&mut self, object: TermExpression) -> Result<TermExpression, ParseError> {
        let start = object.span().start;
        self.expect_symbol(TokenKind::DoubleColon)?;
        let member = self.expect_member_name_text()?;
        let end = self.previous().span.end;
        let caller = NamePath { parts: vec![member], span: span(start, end) };
        Ok(TermExpression::DotCall(Box::new(TermDotExpression {
            base: object,
            caller,
            arguments: TermArguments { arguments: vec![] },
            span: span(start, end),
        })))
    }

    fn parse_subscript_expression(&mut self, object: TermExpression, kind: SubscriptKind) -> Result<TermExpression, ParseError> {
        let start = object.span().start;
        self.expect_symbol(TokenKind::LBracket)?;
        let index = self.parse_value_expression_bp(0)?;
        let close = self.expect_symbol(TokenKind::RBracket)?;
        Ok(TermExpression::Subscript(Box::new(TermSubscriptExpression {
            base: object,
            subscripts: vec![SubscriptItem::Index { term: index, span: span(start, close.span.end) }],
            kind,
            span: span(start, close.span.end),
        })))
    }

    fn parse_offset_subscript_expression(&mut self, object: TermExpression) -> Result<TermExpression, ParseError> {
        let start = object.span().start;
        self.expect_symbol(TokenKind::LOffsetBracket)?;
        let index = self.parse_value_expression_bp(0)?;
        let close = self.expect_symbol(TokenKind::ROffsetBracket)?;
        Ok(TermExpression::Subscript(Box::new(TermSubscriptExpression {
            base: object,
            subscripts: vec![SubscriptItem::Index { term: index, span: span(start, close.span.end) }],
            kind: SubscriptKind::Cardinal,
            span: span(start, close.span.end),
        })))
    }

    fn parse_offset_alias_subscript_expression(&mut self, object: TermExpression) -> Result<TermExpression, ParseError> {
        let start = object.span().start;
        self.expect_symbol(TokenKind::DoubleColon)?;
        self.expect_symbol(TokenKind::LBracket)?;
        let index = self.parse_value_expression_bp(0)?;
        let close = self.expect_symbol(TokenKind::RBracket)?;
        Ok(TermExpression::Subscript(Box::new(TermSubscriptExpression {
            base: object,
            subscripts: vec![SubscriptItem::Index { term: index, span: span(start, close.span.end) }],
            kind: SubscriptKind::Cardinal,
            span: span(start, close.span.end),
        })))
    }

    fn parse_turbofish_expression(&mut self, expr: TermExpression) -> Result<TermExpression, ParseError> {
        let start = expr.span().start;
        self.expect_symbol(TokenKind::DoubleColon)?;
        let arguments = self.parse_type_argument_clause()?;
        let end = self.previous().span.end;
        Ok(TermExpression::Turbofish { expr: Box::new(expr), arguments, span: span(start, end) })
    }

    fn parse_construct_expression(&mut self, lhs: TermExpression) -> Result<TermExpression, ParseError> {
        // `Foo { ... }` and qualified `std.math.random.Random { ... }` both land here.
        // Dotted names parse as empty-arg DotCall chains (member bp > LBrace), so flatten
        // them back into a NamePath instead of rejecting with "requires a type name".
        let path = match construct_type_name_path(lhs) {
            Some(path) => path,
            None => return Err(self.error_here("struct constructor requires a type name")),
        };
        let start = path.span.start;
        self.expect_symbol(TokenKind::LBrace)?;
        let mut fields = Vec::new();
        while !self.check_symbol(TokenKind::RBrace) {
            if self.is_eof() {
                return Err(ParseError::invalid("unterminated struct constructor"));
            }
            let field_name = self.expect_member_name_text()?;
            let value = if self.match_symbol(TokenKind::Colon) {
                self.parse_value_expression_bp(0)?
            }
            else {
                let name_span = self.previous().span.clone();
                TermExpression::Name { path: NamePath { parts: vec![field_name.clone()], span: name_span.clone() }, span: name_span }
            };
            fields.push((field_name, value));
            if !self.match_symbol(TokenKind::Comma) {
                break;
            }
        }
        let close = self.expect_symbol(TokenKind::RBrace)?;
        Ok(TermExpression::Construct { path, fields, span: span(start, close.span.end) })
    }

    pub(super) fn parse_anonymous_class_expression(&mut self) -> Result<TermExpression, ParseError> {
        self.parse_anonymous_object_expression(false)
    }

    pub(super) fn parse_anonymous_structure_expression(&mut self) -> Result<TermExpression, ParseError> {
        self.parse_anonymous_object_expression(true)
    }

    fn parse_anonymous_object_expression(&mut self, is_value_type: bool) -> Result<TermExpression, ParseError> {
        let keyword = if is_value_type { Keyword::Structure } else { Keyword::Class };
        let kind_name = keyword.as_str();
        let start = self.expect_token_keyword(keyword)?.span.start;
        let parents = if self.match_symbol(TokenKind::Colon) {
            if self.check_symbol(TokenKind::LBrace) {
                return Err(ParseError::invalid(format!(
                    "anonymous {kind_name} with ':' requires at least one trait bound; use `{kind_name} {{ ... }}` for a fully anonymous {kind_name}",
                )));
            }
            self.parse_trait_inheritance_list()?
        }
        else if self.match_symbol(TokenKind::LParen) {
            let items = self.parse_comma_separated_until(TokenKind::RParen, |parser| parser.parse_inheritance_item())?;
            self.expect_symbol(TokenKind::RParen)?;
            items
        }
        else {
            Vec::new()
        };
        let body = self.parse_anonymous_class_body()?;
        let end = self.previous().span.end;
        Ok(TermExpression::AnonymousClass { is_value_type, parents, body, span: span(start, end) })
    }

    fn parse_anonymous_class_body(&mut self) -> Result<crate::transitional::cst::valkyrie::ast::ObjectBody, ParseError> {
        use crate::transitional::cst::valkyrie::ast::{IdentifierNode, ObjectBody, ObjectFieldDeclaration};
        self.expect_symbol(TokenKind::LBrace)?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();

        while !self.check_symbol(TokenKind::RBrace) {
            if self.is_eof() {
                return Err(ParseError::invalid("unterminated anonymous class body"));
            }
            let annotations = self.parse_annotations()?;
            if self.check_symbol(TokenKind::RBrace) {
                break;
            }

            if self.check_token_keyword(Keyword::Micro)
                || self.check_identifier_text_eq("get")
                || self.check_identifier_text_eq("set")
                || self.check_identifier_text_eq("on")
            {
                methods.push(self.parse_object_method_declaration(annotations)?);
                continue;
            }

            let field_start = self.current().span.start;
            let name_start = self.current().span.start;
            let name_text = self.expect_member_name_text()?;
            let name = IdentifierNode::new(nyar_types::Identifier::new(&name_text), span(name_start, self.previous().span.end));
            self.expect_symbol(TokenKind::Colon)?;
            let value = self.parse_expression_bp(0)?;
            let auto_type = crate::transitional::cst::valkyrie::ast::TypeExpression::Path(crate::transitional::cst::valkyrie::ast::TypePath {
                name: crate::transitional::cst::valkyrie::ast::NamePath { parts: vec!["_".to_string()], span: span(name_start, name_start) },
                arguments: Vec::new(),
                span: span(name_start, name_start),
            });
            let (field_type, default_value) = (auto_type, Some(value));
            self.match_symbol(TokenKind::Comma);
            self.match_symbol(TokenKind::Semicolon);
            fields.push(ObjectFieldDeclaration {
                annotations,
                name,
                field_type,
                default_value,
                span: span(field_start, self.previous().span.end),
            });
        }

        self.expect_symbol(TokenKind::RBrace)?;
        Ok(ObjectBody {
            fields,
            methods,
            associated_types: Vec::new(),
            associated_constants: Vec::new(),
            variants: Vec::new(),
            script_statements: Vec::new(),
        })
    }
}

/// Flatten `Name` / empty-arg `DotCall` chains into a type `NamePath` for `T { ... }`.
fn construct_type_name_path(expr: TermExpression) -> Option<NamePath> {
    match expr {
        TermExpression::Name { path, .. } => Some(path),
        TermExpression::Turbofish { expr, .. } => construct_type_name_path(*expr),
        TermExpression::DotCall(dot) => {
            if !dot.arguments.arguments.is_empty() {
                return None;
            }
            let mut path = construct_type_name_path(dot.base)?;
            let end = path.span.end.max(dot.caller.span.end);
            path.parts.extend(dot.caller.parts);
            path.span.end = end;
            Some(path)
        }
        _ => None,
    }
}
