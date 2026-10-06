use crate::transitional::cst::valkyrie::{
    ast::{
        GenericParameterDeclaration, IdentifierNode, InheritanceItem, NamePath, PointerKind, RowMethodTypeExpression, TypeExpression, TypePath,
        WhereConstraintDeclaration,
    },
    lexer::{Keyword, TokenKind},
};

use super::{ParseError, Parser, span, with_type_span};

impl<'a> Parser<'a> {
    pub(super) fn parse_intersection_type_expression(&mut self) -> Result<TypeExpression, ParseError> {
        self.parse_union_type_expression()
    }

    fn parse_union_type_expression(&mut self) -> Result<TypeExpression, ParseError> {
        let start = self.current().span.start;
        let mut items = vec![self.parse_intersection_type_items()?];
        while self.match_symbol(TokenKind::Pipe) {
            items.push(self.parse_intersection_type_items()?);
        }
        let end = self.previous().span.end;
        if items.len() == 1 {
            Ok(items.into_iter().next().expect("union type list is non-empty"))
        }
        else {
            Ok(TypeExpression::Union { items, span: span(start, end) })
        }
    }

    fn parse_intersection_type_items(&mut self) -> Result<TypeExpression, ParseError> {
        let start = self.current().span.start;
        let items = self.parse_separated_while(|parser| parser.parse_type_expression_bp(0), &[TokenKind::Ampersand])?;
        let end = self.previous().span.end;
        if items.len() == 1 {
            Ok(items.into_iter().next().expect("intersection type list is non-empty"))
        }
        else {
            Ok(TypeExpression::Intersection { items, span: span(start, end) })
        }
    }

    pub(super) fn parse_trait_inheritance_list(&mut self) -> Result<Vec<InheritanceItem>, ParseError> {
        self.parse_separated_while(|parser| parser.parse_inheritance_item(), &[TokenKind::Comma, TokenKind::Plus])
    }

    pub(super) fn parse_trait_bound_list(&mut self) -> Result<Vec<TypeExpression>, ParseError> {
        // trait bounds 只由 `+` 分隔（如 `T: Iterator + Clone`）。
        // 不包含 `,`，因为 `,` 用于分隔不同的约束或参数。
        self.parse_separated_while(|parser| parser.parse_type_expression_bp(0), &[TokenKind::Plus])
    }

    pub(super) fn parse_name_path(&mut self) -> Result<NamePath, ParseError> {
        self.parse_name_path_with(false)
    }

    /// 解析 `using` 语句中的点分路径。
    ///
    /// Valkyrie 的 `using` 使用 `.` 作为模块路径分隔符
    /// （如 `using std.data.text.von;`），与表达式中的成员访问 `.` 不同。
    pub(super) fn parse_dotted_name_path(&mut self) -> Result<NamePath, ParseError> {
        self.parse_name_path_with(true)
    }

    fn parse_name_path_with(&mut self, allow_dot: bool) -> Result<NamePath, ParseError> {
        let start = self.current().span.start;
        // 路径段允许关键字（如 `std::collections::BitSet::union`）。
        let mut parts = vec![self.expect_member_name_text()?];
        loop {
            let is_colon = self.check_symbol(TokenKind::DoubleColon) && self.nth_is_type_name(1);
            let is_dot = allow_dot && self.check_symbol(TokenKind::Dot) && self.nth_is_type_name(1);
            if !is_colon && !is_dot {
                break;
            }
            self.bump();
            parts.push(self.expect_member_name_text()?);
        }
        Ok(NamePath { parts, span: span(start, self.previous().span.end) })
    }

    pub(super) fn parse_type_expression_bp(&mut self, min_bp: u8) -> Result<TypeExpression, ParseError> {
        let mut lhs = self.parse_type_expression_prefix()?;
        loop {
            let Some((left_bp, _right_bp)) = self.type_postfix_binding_power()
            else {
                break;
            };
            if left_bp < min_bp {
                break;
            }
            lhs = self.parse_type_expression_postfix(lhs)?;
        }
        Ok(lhs)
    }

    /// 解析可能带命名字段的类型表达式。
    ///
    /// 支持 `name: Type` 形式（跳过字段名和 `:`，只解析类型），
    /// 以及普通类型表达式 `Type`。
    fn parse_named_or_plain_type(&mut self) -> Result<TypeExpression, ParseError> {
        // 检查 `identifier :` 模式（命名字段）。
        if self.current_identifier_text().is_some() && self.nth_is_symbol(1, TokenKind::Colon) {
            self.bump(); // 跳过字段名
            self.bump(); // 跳过 `:`
        }
        self.parse_type_expression_bp(0)
    }

    fn parse_type_expression_prefix(&mut self) -> Result<TypeExpression, ParseError> {
        // `&Type` 引用类型：跳过 `&`，解析底层类型（自举阶段不区分值/引用）。
        if self.match_symbol(TokenKind::Ampersand) {
            return self.parse_type_expression_bp(0);
        }

        if self.match_symbol(TokenKind::HollowDiamond) {
            let start = self.previous().span.start;
            let item = self.parse_type_expression_bp(100)?;
            let end = item.span().end;
            return Ok(TypeExpression::Pointer { kind: PointerKind::ReadOnly, item: Box::new(item), span: span(start, end) });
        }

        if self.match_symbol(TokenKind::SolidDiamond) {
            let start = self.previous().span.start;
            let item = self.parse_type_expression_bp(100)?;
            let end = item.span().end;
            return Ok(TypeExpression::Pointer { kind: PointerKind::Mutable, item: Box::new(item), span: span(start, end) });
        }

        if self.match_symbol(TokenKind::LBracket) {
            let open = self.previous().span.clone();
            let first = self.parse_type_expression_bp(0)?;
            // `[T1, T2, ...]` — 元组类型（多个类型用逗号分隔）。
            if self.match_symbol(TokenKind::Comma) {
                let mut items = vec![first];
                loop {
                    items.push(self.parse_type_expression_bp(0)?);
                    if !self.match_symbol(TokenKind::Comma) {
                        break;
                    }
                }
                let close = self.expect_symbol(TokenKind::RBracket)?;
                return Ok(TypeExpression::Tuple { items, span: span(open.start, close.span.end) });
            }
            if self.match_symbol(TokenKind::Semicolon) {
                let length = self.parse_type_array_length()?;
                let close = self.expect_symbol(TokenKind::RBracket)?;
                return Ok(TypeExpression::FixedArray { item: Box::new(first), length, span: span(open.start, close.span.end) });
            }
            let close = self.expect_symbol(TokenKind::RBracket)?;
            return Ok(TypeExpression::Array { item: Box::new(first), span: span(open.start, close.span.end) });
        }

        if self.match_symbol(TokenKind::LParen) {
            let open = self.previous().span.clone();
            if self.match_symbol(TokenKind::RParen) {
                return Ok(TypeExpression::Tuple { items: Vec::new(), span: span(open.start, self.previous().span.end) });
            }

            // 支持带命名字段的元组类型 `(name: Type, name2: Type2)`。
            // 自举阶段跳过字段名，只解析类型。
            let first = self.parse_named_or_plain_type()?;
            if self.match_symbol(TokenKind::Comma) {
                let mut items = vec![first];
                loop {
                    items.push(self.parse_named_or_plain_type()?);
                    if !self.match_symbol(TokenKind::Comma) {
                        break;
                    }
                }
                let close = self.expect_symbol(TokenKind::RParen)?;
                return Ok(TypeExpression::Tuple { items, span: span(open.start, close.span.end) });
            }

            self.expect_symbol(TokenKind::RParen)?;
            return Ok(with_type_span(first, span(open.start, self.previous().span.end)));
        }

        if self.match_symbol(TokenKind::LBrace) {
            let open = self.previous().span.clone();
            let methods = self.parse_comma_separated_until(TokenKind::RBrace, |parser| parser.parse_row_method_type())?;
            let close = self.expect_symbol(TokenKind::RBrace)?;
            return Ok(TypeExpression::Row { methods, span: span(open.start, close.span.end) });
        }

        // 函数类型：`micro(P1, P2) -> R`。
        if self.check_token_keyword(Keyword::Micro) && self.nth_is_symbol(1, TokenKind::LParen) {
            let start = self.current().span.start;
            self.bump(); // 消费 `micro`
            self.expect_symbol(TokenKind::LParen)?;
            let params = self.parse_comma_separated_until(TokenKind::RParen, |parser| parser.parse_type_expression_bp(0))?;
            self.expect_symbol(TokenKind::RParen)?;
            let return_type = if self.match_symbol(TokenKind::Arrow) || self.match_symbol(TokenKind::Colon) {
                self.parse_type_expression_bp(0)?
            }
            else {
                TypeExpression::Tuple { items: Vec::new(), span: span(self.previous().span.end, self.previous().span.end) }
            };
            let end = self.previous().span.end;
            return Ok(TypeExpression::Function { params, return_type: Box::new(return_type), span: span(start, end) });
        }

        let path = self.parse_type_path()?;
        Ok(TypeExpression::Path(path))
    }

    fn parse_row_method_type(&mut self) -> Result<RowMethodTypeExpression, ParseError> {
        let start = self.current().span.start;
        let name_start = self.current().span.start;
        let name_text = self.expect_identifier_text()?.to_string();
        let name = IdentifierNode::new(nyar_types::Identifier::new(&name_text), span(name_start, self.previous().span.end));
        self.expect_symbol(TokenKind::LParen)?;
        let params = self.parse_comma_separated_until(TokenKind::RParen, |parser| parser.parse_named_or_plain_type())?;
        self.expect_symbol(TokenKind::RParen)?;
        if !(self.match_symbol(TokenKind::Arrow) || self.match_symbol(TokenKind::Colon)) {
            return Err(self.error_here("expected `->` after anonymous row method signature"));
        }
        let return_type = self.parse_type_expression_bp(0)?;
        let end = return_type.span().end;
        Ok(RowMethodTypeExpression { name, params, return_type: Box::new(return_type), span: span(start, end) })
    }

    fn parse_type_expression_postfix(&mut self, lhs: TypeExpression) -> Result<TypeExpression, ParseError> {
        match self.current().kind {
            TokenKind::LAngle => match lhs {
                TypeExpression::Path(mut path) => {
                    path.arguments = self.parse_type_argument_clause()?;
                    path.span = span(path.span.start, self.previous().span.end);
                    Ok(TypeExpression::Path(path))
                }
                _ => Err(self.error_here("generic arguments can only follow a path type")),
            },
            // 后缀 `[]` 数组类型：`utf16[]` 表示 `Array<utf16>`。
            TokenKind::LBracket => {
                let start = self.current().span.start;
                self.expect_symbol(TokenKind::LBracket)?;
                self.expect_symbol(TokenKind::RBracket)?;
                let end = self.previous().span.end;
                Ok(TypeExpression::Array { item: Box::new(lhs), span: span(start, end) })
            }
            // 后缀 `?` 可空类型：`T?` 表示 `T` 或 `null`。
            TokenKind::Question => {
                let start = lhs.span().start;
                self.bump();
                let end = self.previous().span.end;
                Ok(TypeExpression::Nullable { item: Box::new(lhs), span: span(start, end) })
            }
            _ => Err(self.error_here("expected generic arguments `<...>` or array suffix `[]` after type")),
        }
    }

    fn type_postfix_binding_power(&self) -> Option<(u8, u8)> {
        match self.current().kind {
            TokenKind::LAngle => Some((90, 91)),
            // 仅当 `[` 后紧跟 `]` 时才视为数组后缀 `[]`，
            // 避免将属性列表 `[clr(...)]` 误认为数组类型。
            TokenKind::LBracket if self.nth_is_symbol(1, TokenKind::RBracket) => Some((90, 91)),
            // `?` 可空类型后缀，绑定力高于 `<` 和 `[]`。
            TokenKind::Question => Some((95, 96)),
            _ => None,
        }
    }

    fn parse_type_path(&mut self) -> Result<TypePath, ParseError> {
        let start = self.current().span.start;
        let mut parts = vec![self.expect_type_name_text()?];
        // 同时接受 `::` 和 `.` 作为类型路径分隔符（源码中两种写法均有使用）。
        loop {
            let is_colon = self.check_symbol(TokenKind::DoubleColon) && self.nth_is_type_name(1);
            let is_dot = self.check_symbol(TokenKind::Dot) && self.nth_is_type_name(1);
            if !is_colon && !is_dot {
                break;
            }
            self.bump();
            parts.push(self.expect_type_name_text()?);
        }
        let name_span = span(start, self.previous().span.end);
        Ok(TypePath { name: NamePath { parts, span: name_span.clone() }, arguments: Vec::new(), span: name_span })
    }

    pub(super) fn parse_type_argument_clause(&mut self) -> Result<Vec<TypeExpression>, ParseError> {
        self.expect_symbol(TokenKind::LAngle)?;
        let arguments = self.parse_comma_separated_until(TokenKind::RAngle, |parser| parser.parse_type_argument())?;
        self.expect_symbol(TokenKind::RAngle)?;
        Ok(arguments)
    }

    /// 解析类型参数：可能是普通类型表达式，也可能是 `Name = Type` 关联类型绑定。
    fn parse_type_argument(&mut self) -> Result<TypeExpression, ParseError> {
        let start = self.current().span.start;
        // 尝试解析 `Name = Type` 形式的关联类型绑定。
        if matches!(self.current().kind, TokenKind::Identifier)
            && !self.check_token_keyword(Keyword::Where)
            && self.peek().kind == TokenKind::Equal
        {
            let name_start = self.current().span.start;
            let name_text = self.expect_identifier_text()?.to_string();
            let name = IdentifierNode::new(nyar_types::Identifier::new(&name_text), span(name_start, self.previous().span.end));
            self.expect_symbol(TokenKind::Equal)?;
            let ty = self.parse_type_expression_bp(0)?;
            let end = self.previous().span.end;
            return Ok(TypeExpression::Associated { name, ty: Box::new(ty), span: span(start, end) });
        }
        self.parse_type_expression_bp(0)
    }

    pub(super) fn parse_generic_parameter_clause(&mut self) -> Result<Vec<String>, ParseError> {
        if !self.check_symbol(TokenKind::LAngle) {
            return Ok(Vec::new());
        }

        let open = self.expect_symbol(TokenKind::LAngle)?;
        let mut depth = 1i32;
        let mut segment_start = open.span.end;
        let mut parameters = Vec::new();

        while !self.is_eof() {
            let token = self.bump().clone();
            match token.kind {
                TokenKind::LAngle => depth += 1,
                TokenKind::RAngle => {
                    depth -= 1;
                    if depth == 0 {
                        let text = self.slice(segment_start..token.span.start).trim();
                        if !text.is_empty() {
                            parameters.push(text.to_string());
                        }
                        return Ok(parameters);
                    }
                }
                TokenKind::Comma if depth == 1 => {
                    let text = self.slice(segment_start..token.span.start).trim();
                    if !text.is_empty() {
                        parameters.push(text.to_string());
                    }
                    segment_start = token.span.end;
                }
                _ => {}
            }
        }

        Err(ParseError::invalid("unterminated generic parameter clause"))
    }

    pub(super) fn parse_structured_generic_parameter_clause(&mut self) -> Result<Vec<GenericParameterDeclaration>, ParseError> {
        if !self.check_symbol(TokenKind::LAngle) {
            return Ok(Vec::new());
        }

        self.expect_symbol(TokenKind::LAngle)?;
        let parameters = self.parse_comma_separated_until(TokenKind::RAngle, |parser| parser.parse_generic_parameter_declaration())?;
        self.expect_symbol(TokenKind::RAngle)?;
        Ok(parameters)
    }

    fn parse_generic_parameter_declaration(&mut self) -> Result<GenericParameterDeclaration, ParseError> {
        let start = self.current().span.start;
        let name = self.expect_identifier_text()?.to_string();
        let bounds = if self.match_symbol(TokenKind::Colon) { self.parse_trait_bound_list()? } else { Vec::new() };
        let default_type = if self.match_symbol(TokenKind::Equal) { Some(self.parse_intersection_type_expression()?) } else { None };
        Ok(GenericParameterDeclaration {
            name: IdentifierNode::new(nyar_types::Identifier::new(&name), span(start, self.previous().span.end)),
            bounds,
            default_type,
            span: span(start, self.previous().span.end),
        })
    }

    pub(super) fn parse_where_constraints(&mut self) -> Result<Vec<WhereConstraintDeclaration>, ParseError> {
        if !self.check_token_keyword(Keyword::Where) {
            return Ok(Vec::new());
        }

        self.expect_token_keyword(Keyword::Where)?;
        let constraints = self.parse_comma_separated_until(TokenKind::LBrace, |parser| parser.parse_where_constraint())?;
        Ok(constraints)
    }

    fn parse_where_constraint(&mut self) -> Result<WhereConstraintDeclaration, ParseError> {
        let start = self.current().span.start;
        let target_type = self.parse_intersection_type_expression()?;
        self.expect_symbol(TokenKind::Colon)?;
        let bounds = self.parse_trait_bound_list()?;
        Ok(WhereConstraintDeclaration { target_type, bounds, span: span(start, self.previous().span.end) })
    }

    pub(super) fn skip_generic_parameter_clause(&mut self) -> Result<(), ParseError> {
        if !self.check_symbol(TokenKind::LAngle) {
            return Ok(());
        }

        let mut depth = 0i32;
        while !self.is_eof() {
            match self.current().kind {
                TokenKind::LAngle => {
                    self.bump();
                    depth += 1;
                }
                TokenKind::RAngle | TokenKind::Shr => {
                    self.bump();
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                    if self.previous().kind == TokenKind::Shr {
                        depth -= 1;
                        if depth == 0 {
                            return Ok(());
                        }
                    }
                }
                _ => {
                    self.bump();
                }
            }
        }

        Err(ParseError::invalid("unterminated generic parameter clause"))
    }

    fn parse_type_array_length(&mut self) -> Result<u64, ParseError> {
        match self.current().kind {
            TokenKind::IntegerLiteral => {
                let span = self.current().span.clone();
                let text = self.slice(span).to_string();
                self.bump();
                text.parse::<u64>().map_err(|_| ParseError::invalid(format!("invalid fixed array length `{text}`")))
            }
            _ => Err(ParseError::invalid("expected integer literal for fixed array length")),
        }
    }
}
