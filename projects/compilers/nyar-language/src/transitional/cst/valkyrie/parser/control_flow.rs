use crate::transitional::cst::valkyrie::{
    ast::{
        Annotations, BreakStatement, ContinueStatement, DeclarationBody, FallthroughStatement, FunctionStatement, IfLetStatement, IfStatement,
        LetStatement, LoopInStatement, LoopStatement, PatternExpression, ResumeStatement, ReturnStatement, TermExpression, TryStatement,
        UntilNotStatement, UntilStatement, WhileLetStatement, WhileStatement, YieldFromStatement, YieldStatement,
    },
    lexer::{Keyword, TokenKind},
};
use nyar_types::Identifier;

use super::{ParseError, Parser, expression_requires_statement, span};

impl<'a> Parser<'a> {
    pub(super) fn parse_return_statement(&mut self) -> Result<FunctionStatement, ParseError> {
        let start = self.expect_token_keyword(Keyword::Return)?.span.start;
        let value = if self.is_expression_terminator() { None } else { Some(Box::new(self.parse_value_expression_bp(0)?)) };
        let end = value.as_ref().map_or(self.previous().span.end, |expr| expr.span().end);
        Ok(FunctionStatement::Return(ReturnStatement { value: value.map(|value| *value), span: span(start, end) }))
    }

    pub(super) fn parse_break_statement(&mut self) -> Result<FunctionStatement, ParseError> {
        let start = self.expect_token_keyword(Keyword::Break)?.span.start;
        let label = if self.check_symbol(TokenKind::Apostrophe) { Some(Identifier::new(&self.parse_label_name()?)) } else { None };
        let value = if self.is_expression_terminator() { None } else { Some(Box::new(self.parse_value_expression_bp(0)?)) };
        let end = value.as_ref().map_or(self.previous().span.end, |expr| expr.span().end);
        Ok(FunctionStatement::Break(BreakStatement { label, value: value.map(|value| *value), span: span(start, end) }))
    }

    pub(super) fn parse_continue_statement(&mut self) -> Result<FunctionStatement, ParseError> {
        let start = self.expect_token_keyword(Keyword::Continue)?.span.start;
        let label = if self.check_symbol(TokenKind::Apostrophe) { Some(Identifier::new(&self.parse_label_name()?)) } else { None };
        let end = self.previous().span.end;
        Ok(FunctionStatement::Continue(ContinueStatement { label, span: span(start, end) }))
    }

    pub(super) fn parse_yield_statement(&mut self) -> Result<FunctionStatement, ParseError> {
        let start = self.expect_token_keyword(Keyword::Yield)?.span.start;
        if self.match_identifier_text_eq("from") {
            let value = self.parse_value_expression_bp(0)?;
            let end = value.span().end;
            return Ok(FunctionStatement::YieldFrom(YieldFromStatement { value, span: span(start, end) }));
        }

        let value = if self.is_expression_terminator() { None } else { Some(self.parse_value_expression_bp(0)?) };
        let end = value.as_ref().map_or(self.previous().span.end, |expr| expr.span().end);
        Ok(FunctionStatement::Yield(YieldStatement { value, span: span(start, end) }))
    }

    pub(super) fn parse_resume_statement(&mut self) -> Result<FunctionStatement, ParseError> {
        let start = self.expect_token_keyword(Keyword::Resume)?.span.start;
        let value = self.parse_value_expression_bp(0)?;
        let end = value.span().end;
        Ok(FunctionStatement::Resume(ResumeStatement { value: Some(value), span: span(start, end) }))
    }

    pub(super) fn is_expression_terminator(&self) -> bool {
        matches!(self.current().kind, TokenKind::Semicolon | TokenKind::RBrace | TokenKind::Comma | TokenKind::RParen | TokenKind::RBracket)
            || matches!(
                self.current().kind,
                TokenKind::Keyword(
                    Keyword::Case
                        | Keyword::Else
                        | Keyword::Let
                        | Keyword::Mut
                        | Keyword::Return
                        | Keyword::Break
                        | Keyword::Continue
                        | Keyword::Yield
                        | Keyword::Resume
                        | Keyword::Fallthrough
                )
            )
    }

    pub(super) fn parse_control_flow_statement(&mut self) -> Result<FunctionStatement, ParseError> {
        match self.current().kind {
            TokenKind::Keyword(Keyword::Return) => self.parse_return_statement(),
            TokenKind::Keyword(Keyword::Break) => self.parse_break_statement(),
            TokenKind::Keyword(Keyword::Continue) => self.parse_continue_statement(),
            TokenKind::Keyword(Keyword::Yield) => self.parse_yield_statement(),
            TokenKind::Keyword(Keyword::Resume) => self.parse_resume_statement(),
            TokenKind::Keyword(Keyword::Fallthrough) => self.parse_fallthrough_statement(),
            _ => Err(self.error_here("expected control flow statement")),
        }
    }

    pub(super) fn parse_fallthrough_statement(&mut self) -> Result<FunctionStatement, ParseError> {
        let span = self.expect_token_keyword(Keyword::Fallthrough)?.span;
        Ok(FunctionStatement::Fallthrough(FallthroughStatement { span }))
    }

    /// 解析 `if condition { then } else { else }` 表达式。
    ///
    /// `else` 分支可选；当存在时既可以是块体，也可以是嵌套的 `if` 表达式。
    ///
    /// `else:`（后跟冒号）是 match catch-all 臂，不是 if-else——不可在此吞掉。
    pub(super) fn parse_if_expression(&mut self) -> Result<TermExpression, ParseError> {
        enum IfBranch {
            Condition { start: usize, condition: TermExpression, then_body: DeclarationBody },
            Let { start: usize, pattern: PatternExpression, item: TermExpression, then_body: DeclarationBody },
        }

        let mut branches = Vec::new();
        let final_else_body = loop {
            let start = self.expect_token_keyword(Keyword::If)?.span.start;
            if self.match_token_keyword(Keyword::Let) {
                let pattern = self.parse_pattern_expression_full()?;
                self.expect_symbol(TokenKind::Equal)?;
                self.suppress_struct_constructor = true;
                let item = self.parse_expression_bp(0)?;
                self.suppress_struct_constructor = false;
                let then_body = self.parse_block_body()?;
                branches.push(IfBranch::Let { start, pattern, item, then_body });
            }
            else {
                // Suppress struct-constructor postfix parsing so the following block is the if body.
                self.suppress_struct_constructor = true;
                let condition = self.parse_expression_bp(0)?;
                self.suppress_struct_constructor = false;
                let then_body = self.parse_block_body()?;
                branches.push(IfBranch::Condition { start, condition, then_body });
            }

            if !self.match_if_else_keyword() {
                break None;
            }
            if !self.check_token_keyword(Keyword::If) {
                break Some(self.parse_block_body()?);
            }
        };

        let mut nested = None;
        for branch in branches.into_iter().rev() {
            let else_body = nested
                .take()
                .map(|expression: TermExpression| {
                    let nested_span = expression.span().clone();
                    DeclarationBody { statements: Vec::new(), tail_expression: Some(expression), span: nested_span }
                })
                .or_else(|| final_else_body.clone());
            let end = else_body.as_ref().map_or_else(
                || match &branch {
                    IfBranch::Condition { then_body, .. } | IfBranch::Let { then_body, .. } => then_body.span.end,
                },
                |body| body.span.end,
            );
            nested = Some(match branch {
                IfBranch::Condition { start, condition, then_body } => {
                    TermExpression::If(Box::new(IfStatement { condition, then_body, else_body, span: span(start, end) }))
                }
                IfBranch::Let { start, pattern, item, then_body } => {
                    TermExpression::IfLet(Box::new(IfLetStatement { pattern, item, then_body, else_body, span: span(start, end) }))
                }
            });
        }
        Ok(nested.expect("if expression has an initial branch"))
    }

    /// Match `else` only when it starts an if-else (`else {` / `else if`), not match `else:`.
    fn match_if_else_keyword(&mut self) -> bool {
        if !self.check_token_keyword(Keyword::Else) {
            return false;
        }
        // `else:` / `else :` is the match catch-all arm terminator — leave it for the match parser.
        if self.nth_is_symbol(1, TokenKind::Colon) {
            return false;
        }
        self.match_token_keyword(Keyword::Else)
    }

    /// 解析 `loop` 表达式。
    ///
    /// 支持两种语法：
    /// - 无限循环：`loop { body }`
    /// - 迭代循环：`loop pattern in iterator { body }`
    pub(super) fn parse_loop_expression(&mut self) -> Result<TermExpression, ParseError> {
        let start = self.expect_token_keyword(Keyword::Loop)?.span.start;
        self.parse_loop_expression_after_keyword(start, None)
    }

    pub(super) fn parse_loop_expression_after_keyword(
        &mut self,
        start: usize,
        label: Option<Identifier>,
    ) -> Result<TermExpression, ParseError> {
        if self.check_symbol(TokenKind::LBrace) {
            let body = self.parse_block_body()?;
            let end = body.span.end;
            if let Some(label) = label {
                return Ok(TermExpression::LoopIn(Box::new(LoopInStatement {
                    label: Some(label),
                    pattern: None,
                    iterator: None,
                    condition: None,
                    body,
                    span: span(start, end),
                })));
            }
            return Ok(TermExpression::Loop(Box::new(LoopStatement { body, span: span(start, end) })));
        }

        let checkpoint = self.index;
        if matches!(self.current().kind, TokenKind::Identifier | TokenKind::LParen) {
            if let Ok(pattern) = self.parse_pattern_expression() {
                if self.match_token_keyword(Keyword::In) {
                    // 抑制结构体构造 postfix，避免误吞 loop 块体。
                    self.suppress_struct_constructor = true;
                    let iterator = Some(self.parse_expression_bp(0)?);
                    self.suppress_struct_constructor = false;
                    let body = self.parse_block_body()?;
                    let end = body.span.end;
                    return Ok(TermExpression::LoopIn(Box::new(LoopInStatement {
                        label,
                        pattern: Some(pattern),
                        iterator,
                        condition: None,
                        body,
                        span: span(start, end),
                    })));
                }
            }
            self.index = checkpoint;
        }

        Err(ParseError::invalid_at("loop 只支持 `loop {}` 或 `loop pattern in iterator {}`", self.current().span.clone()))
    }

    /// 解析 `while condition { body }` 表达式。
    ///
    /// `while condition { body }` 复用 `TermExpression::Loop`。
    pub(super) fn parse_while_expression(&mut self) -> Result<TermExpression, ParseError> {
        let start = self.expect_token_keyword(Keyword::While)?.span.start;
        self.parse_while_expression_after_keyword(start, None)
    }

    pub(super) fn parse_while_expression_after_keyword(
        &mut self,
        start: usize,
        label: Option<Identifier>,
    ) -> Result<TermExpression, ParseError> {
        if self.match_token_keyword(Keyword::Let) {
            let pattern = self.parse_pattern_expression_full()?;
            self.expect_symbol(TokenKind::Equal)?;
            self.suppress_struct_constructor = true;
            let scrutinee = self.parse_expression_bp(0)?;
            self.suppress_struct_constructor = false;
            let guard = if self.match_token_keyword(Keyword::If) { Some(self.parse_expression_bp(0)?) } else { None };
            let body = self.parse_block_body()?;
            let end = body.span.end;
            return Ok(TermExpression::WhileLet(Box::new(WhileLetStatement {
                label,
                pattern,
                scrutinee,
                guard,
                body,
                span: span(start, end),
            })));
        }
        // 抑制结构体构造 postfix，避免误吞 while 块体。
        self.suppress_struct_constructor = true;
        let condition = Some(self.parse_expression_bp(0)?);
        self.suppress_struct_constructor = false;
        let body = self.parse_block_body()?;
        let end = body.span.end;
        Ok(TermExpression::While(Box::new(WhileStatement { label, condition, body, span: span(start, end) })))
    }

    pub(super) fn parse_labeled_loop_expression(&mut self) -> Result<TermExpression, ParseError> {
        let start = self.current().span.start;
        let label = Identifier::new(&self.parse_label_name()?);
        self.expect_symbol(TokenKind::Colon)?;
        if self.check_token_keyword(Keyword::Loop) {
            self.expect_token_keyword(Keyword::Loop)?;
            self.parse_loop_expression_after_keyword(start, Some(label))
        }
        else if self.check_token_keyword(Keyword::While) {
            self.expect_token_keyword(Keyword::While)?;
            self.parse_while_expression_after_keyword(start, Some(label))
        }
        else if self.check_token_keyword(Keyword::Until) {
            self.expect_token_keyword(Keyword::Until)?;
            self.parse_until_expression_after_keyword(start, Some(label))
        }
        else {
            Err(ParseError::invalid_at("label 只允许附着在 `loop`、`while` 或 `until` 上", self.current().span.clone()))
        }
    }

    /// 解析 `until condition { body }` 或 `until not pattern = expr { body }`。
    pub(super) fn parse_until_expression(&mut self) -> Result<TermExpression, ParseError> {
        let start = self.expect_token_keyword(Keyword::Until)?.span.start;
        self.parse_until_expression_after_keyword(start, None)
    }

    fn parse_until_expression_after_keyword(&mut self, start: usize, label: Option<Identifier>) -> Result<TermExpression, ParseError> {
        if self.match_token_keyword(Keyword::Not) {
            let pattern = Some(self.parse_pattern_expression()?);
            let iterator = if self.match_symbol(TokenKind::Equal) {
                self.suppress_struct_constructor = true;
                let value = self.parse_value_expression_bp(0)?;
                self.suppress_struct_constructor = false;
                Some(value)
            }
            else {
                None
            };
            let condition = if self.match_token_keyword(Keyword::If) { Some(self.parse_expression_bp(0)?) } else { None };
            let body = self.parse_block_body()?;
            let end = body.span.end;
            return Ok(TermExpression::UntilNot(Box::new(UntilNotStatement {
                label,
                pattern,
                iterator,
                condition,
                body,
                span: span(start, end),
            })));
        }

        self.suppress_struct_constructor = true;
        let condition = Some(self.parse_expression_bp(0)?);
        self.suppress_struct_constructor = false;
        let extra = if self.match_token_keyword(Keyword::If) { Some(self.parse_expression_bp(0)?) } else { None };
        let body = self.parse_block_body()?;
        let end = body.span.end;
        Ok(TermExpression::Until(Box::new(UntilStatement { label, pattern: None, iterator: extra, condition, body, span: span(start, end) })))
    }

    /// 解析 `try { body }` / `try? { body }` / `try! { body }` / `try Type { body }`。
    pub(super) fn parse_try_expression(&mut self) -> Result<TermExpression, ParseError> {
        let start = self.expect_token_keyword(Keyword::Try)?.span.start;
        let is_optional = self.match_symbol(TokenKind::Question);
        let is_forced = !is_optional && self.match_symbol(TokenKind::Bang);
        let result_type = if !self.check_symbol(TokenKind::LBrace) { Some(self.parse_intersection_type_expression()?) } else { None };
        let body = self.parse_block_body()?;
        let end = body.span.end;
        Ok(TermExpression::Try(Box::new(TryStatement { is_optional, is_forced, result_type, body, span: span(start, end) })))
    }

    /// 解析 `assert condition` / `assert condition, message` 语句。
    pub(super) fn parse_assert_statement(&mut self) -> Result<FunctionStatement, ParseError> {
        let start = self.expect_token_keyword(Keyword::Assert)?.span.start;
        let condition = self.parse_value_expression_bp(0)?;
        let message = if self.match_symbol(TokenKind::Comma) { Some(self.parse_value_expression_bp(0)?) } else { None };
        let end = message.as_ref().map_or(condition.span().end, |expr| expr.span().end);
        Ok(FunctionStatement::Term {
            expression: TermExpression::Call(Box::new(crate::transitional::cst::valkyrie::ast::TermCallExpression {
                callee: TermExpression::Name {
                    path: crate::transitional::cst::valkyrie::ast::NamePath { parts: vec!["assert".to_string()], span: span(start, start) },
                    span: span(start, start),
                },
                args: crate::transitional::cst::valkyrie::ast::TermArguments {
                    arguments: {
                        let mut arguments = vec![crate::transitional::cst::valkyrie::ast::TermCallArgument { key: None, value: condition }];
                        if let Some(msg) = message {
                            arguments.push(crate::transitional::cst::valkyrie::ast::TermCallArgument { key: None, value: msg });
                        }
                        arguments
                    },
                },
                span: span(start, end),
            })),
            span: span(start, end),
        })
    }

    /// 判断当前 token 是否为 match arm 体的终止边界。
    pub(super) fn is_match_arm_terminator(&self) -> bool {
        if self.check_symbol(TokenKind::RBrace) || self.is_eof() {
            return true;
        }
        if matches!(
            self.current().kind,
            TokenKind::Keyword(crate::transitional::cst::valkyrie::lexer::Keyword::Case | crate::transitional::cst::valkyrie::lexer::Keyword::Else)
        ) {
            return true;
        }
        // `default:` 是 `else:` 的别名，用于 match catch-all arm。
        if self.check_identifier_text_eq("default") {
            return true;
        }
        if self.check_symbol(TokenKind::Comma) {
            return true;
        }
        false
    }

    pub(super) fn parse_pattern_expression(&mut self) -> Result<PatternExpression, ParseError> {
        self.parse_pattern_expression_full()
    }

    pub(super) fn parse_block_body(&mut self) -> Result<DeclarationBody, ParseError> {
        let open = self.expect_symbol(TokenKind::LBrace)?;
        let mut statements = Vec::new();
        let mut tail_expression = None;

        while !self.check_symbol(TokenKind::RBrace) {
            if self.is_eof() {
                return Err(ParseError::invalid("unterminated block body"));
            }
            if self.match_symbol(TokenKind::Semicolon) {
                continue;
            }

            let annotations = self.parse_annotations()?;

            if self.check_token_keyword(Keyword::Let) || self.check_token_keyword(Keyword::Mut) {
                statements.push(self.parse_let_statement(annotations)?);
                continue;
            }
            if self.check_function_decl_keyword() {
                let function = self.parse_function_declaration(annotations)?;
                let span = function.span.clone();
                statements.push(FunctionStatement::Function { function, span });
                continue;
            }
            if !annotations.attribute_lists.is_empty() || !annotations.modifiers.is_empty() {
                return Err(self.error_here("dangling attributes before statement"));
            }
            if self.check_symbol(TokenKind::LBrace) {
                let expr = self.parse_block_expression()?;
                statements.push(FunctionStatement::Term { span: expr.span().clone(), expression: expr });
                continue;
            }
            if self.check_token_keyword(Keyword::Assert) {
                statements.push(self.parse_assert_statement()?);
                self.match_symbol(TokenKind::Semicolon);
                continue;
            }
            if matches!(
                self.current().kind,
                TokenKind::Keyword(
                    Keyword::Return | Keyword::Break | Keyword::Continue | Keyword::Yield | Keyword::Resume | Keyword::Fallthrough
                )
            ) {
                statements.push(self.parse_control_flow_statement()?);
                self.match_symbol(TokenKind::Semicolon);
                continue;
            }

            let expr = self.parse_expression_bp(0)?;
            if self.match_symbol(TokenKind::Semicolon) {
                statements.push(FunctionStatement::Term { span: expr.span().clone(), expression: expr });
                continue;
            }

            // `if`/`match` 等控制流表达式可作为语句使用，不需要分号终止。
            // `loop`/`while`/`until` 等循环表达式不在列表中：当它们是块体最后一个表达式时，
            // 应作为 tail_expression（值上下文），使 `break expr` 的值收敛校验能够生效。
            if matches!(expr, TermExpression::If(_) | TermExpression::Match { .. }) {
                statements.push(FunctionStatement::Term { span: expr.span().clone(), expression: expr });
                continue;
            }

            // 换行隐式终止：当下一个 token 是新语句起始关键字时，当前表达式作为语句结束。
            if self.is_statement_start() {
                statements.push(FunctionStatement::Term { span: expr.span().clone(), expression: expr });
                continue;
            }

            if !self.check_symbol(TokenKind::RBrace) {
                return Err(self.error_here("expected ';' or '}' after expression"));
            }

            if expression_requires_statement(&expr) {
                statements.push(FunctionStatement::Term { span: expr.span().clone(), expression: expr });
            }
            else {
                tail_expression = Some(expr);
            }
            break;
        }

        let close = self.expect_symbol(TokenKind::RBrace)?;
        Ok(DeclarationBody { statements, tail_expression, span: span(open.span.end, close.span.start) })
    }

    pub(super) fn parse_let_statement(&mut self, annotations: Annotations) -> Result<FunctionStatement, ParseError> {
        let start = self.current().span.start;
        let saw_let = self.match_token_keyword(Keyword::Let);
        let is_mutable = self.match_token_keyword(Keyword::Mut);
        if !saw_let && !is_mutable {
            return Err(self.error_here("expected let binding"));
        }

        let pattern = self.parse_pattern_expression()?;
        let ty = if self.match_symbol(TokenKind::Colon) { Some(self.parse_intersection_type_expression()?) } else { None };
        let initializer = if self.match_symbol(TokenKind::Equal) { Some(self.parse_fluent_chain_expression()?) } else { None };
        if !self.match_symbol(TokenKind::Semicolon) && !self.check_symbol(TokenKind::RBrace) && !self.is_eof() && !self.is_statement_start() {
            return Err(self.error_here("expected ';' or statement boundary after let binding"));
        }

        let end = initializer.as_ref().map(|expression| expression.span().end).unwrap_or_else(|| self.previous().span.end);
        Ok(FunctionStatement::Let(LetStatement { annotations, is_mutable, pattern, ty, initializer, span: span(start, end) }))
    }

    pub(super) fn parse_fluent_chain_expression(&mut self) -> Result<TermExpression, ParseError> {
        let mut lhs = self.parse_expression_bp(0)?;
        loop {
            if self.check_symbol(TokenKind::Dot) {
                lhs = self.parse_expression_postfix(lhs)?;
                continue;
            }
            if self.check_symbol(TokenKind::Equal) {
                let start = lhs.span().start;
                self.bump();
                let value = self.parse_value_expression_bp(0)?;
                let end = value.span().end;
                lhs = TermExpression::Assign { target: Box::new(lhs), value: Box::new(value), span: span(start, end) };
                continue;
            }
            break;
        }
        Ok(lhs)
    }
}
