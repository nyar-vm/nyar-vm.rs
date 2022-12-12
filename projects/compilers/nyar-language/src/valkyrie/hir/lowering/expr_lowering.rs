use super::*;

pub(super) fn lower_block(body: Option<&DeclarationBody>, source_id: SourceID, fallback_span: Range<usize>) -> HirBlock {
    let Some(body) = body
    else {
        return HirBlock { statements: Vec::new(), expr: None, span: with_source(&fallback_span, source_id) };
    };

    let statements = body.statements.iter().map(|statement| lower_statement(statement, source_id, fallback_span.clone())).collect();
    let expr = body.tail_expression.as_ref().map(|expr| Box::new(lower_term_expression(expr, source_id, fallback_span.clone())));
    HirBlock { statements, expr, span: with_source(&body.span, source_id) }
}

fn lower_statement(statement: &FunctionStatement, source_id: SourceID, fallback_span: Range<usize>) -> HirStatement {
    let span_range = if statement.span().is_empty() { fallback_span } else { statement.span().clone() };
    let span = with_source(&span_range, source_id);
    let kind = match statement {
        FunctionStatement::Let(statement) => lower_let_statement(statement, source_id, span_range),
        FunctionStatement::Term { expression, .. } => {
            HirStatementKind::Expr(Box::new(lower_statement_expression(expression, source_id, span_range, span.clone())))
        }
        FunctionStatement::Function { function, .. } => HirStatementKind::Expr(Box::new(HirExpr {
            kind: HirExprKind::Path(NamePath::new(vec![function.name.name.clone()])),
            span: span.clone(),
        })),
        FunctionStatement::Break(statement) => HirStatementKind::Expr(Box::new(HirExpr {
            kind: HirExprKind::Break {
                label: statement.label.clone(),
                expr: statement
                    .value
                    .as_ref()
                    .map(|expr| Box::new(lower_term_expression_with_context(expr, source_id, span_range.clone(), false))),
            },
            span: span.clone(),
        })),
        FunctionStatement::Continue(statement) => {
            HirStatementKind::Expr(Box::new(HirExpr { kind: HirExprKind::Continue { label: statement.label.clone() }, span: span.clone() }))
        }
        FunctionStatement::Yield(statement) => HirStatementKind::Expr(Box::new(HirExpr {
            kind: HirExprKind::Yield(
                statement.value.as_ref().map(|expr| Box::new(lower_term_expression_with_context(expr, source_id, span_range.clone(), false))),
            ),
            span: span.clone(),
        })),
        FunctionStatement::YieldFrom(statement) => HirStatementKind::Expr(Box::new(HirExpr {
            kind: HirExprKind::YieldFrom(Box::new(lower_term_expression_with_context(&statement.value, source_id, span_range.clone(), false))),
            span: span.clone(),
        })),
        FunctionStatement::Return(statement) => HirStatementKind::Expr(Box::new(HirExpr {
            kind: HirExprKind::Return(
                statement.value.as_ref().map(|expr| Box::new(lower_term_expression_with_context(expr, source_id, span_range.clone(), false))),
            ),
            span: span.clone(),
        })),
        FunctionStatement::Resume(statement) => HirStatementKind::Expr(Box::new(HirExpr {
            kind: HirExprKind::Resume(Box::new(lower_optional_term_expression(
                statement.value.as_ref(),
                source_id,
                span_range.clone(),
                span.clone(),
            ))),
            span: span.clone(),
        })),
        FunctionStatement::Fallthrough(_) => HirStatementKind::Expr(Box::new(HirExpr { kind: HirExprKind::Fallthrough, span: span.clone() })),
    };
    HirStatement { kind, span }
}

fn lower_let_statement(statement: &LetStatement, source_id: SourceID, fallback_span: Range<usize>) -> HirStatementKind {
    HirStatementKind::Let {
        is_mutable: statement.is_mutable,
        pattern: lower_pattern_expression(&statement.pattern, source_id),
        initializer: statement.initializer.as_ref().map(|expr| Box::new(lower_term_expression(expr, source_id, fallback_span.clone()))),
        ty: statement.ty.as_ref().map(lower_type_expression),
    }
}

fn lower_statement_expression(expression: &TermExpression, source_id: SourceID, fallback_span: Range<usize>, span: SourceSpan) -> HirExpr {
    match expression {
        TermExpression::Match { scrutinee, arms, .. } => HirExpr {
            kind: HirExprKind::Case {
                scrutinee: Box::new(lower_term_expression_with_context(scrutinee, source_id, fallback_span.clone(), false)),
                arms: lower_match_arms(arms, source_id, fallback_span, span.clone()),
            },
            span,
        },
        _ => lower_term_expression(expression, source_id, fallback_span),
    }
}

fn lower_pattern_expression(pattern: &PatternExpression, source_id: SourceID) -> HirPattern {
    let fallback_span = pattern.span().clone();
    let span = with_source(pattern.span(), source_id);
    lower_nested_match_pattern(pattern, source_id, fallback_span, span)
}

pub(super) fn lower_term_expression(expression: &TermExpression, source_id: SourceID, fallback_span: Range<usize>) -> HirExpr {
    lower_term_expression_with_context(expression, source_id, fallback_span, false)
}

fn lower_term_expression_with_context(
    expression: &TermExpression,
    source_id: SourceID,
    fallback_span: Range<usize>,
    preserve_member_access: bool,
) -> HirExpr {
    let span_range = if expression.span().is_empty() { fallback_span } else { expression.span().clone() };
    let span = with_source(&span_range, source_id);
    let kind = match expression {
        TermExpression::Name { path, .. } => lower_name_expression(path, span.clone()),
        TermExpression::Literal { literal, .. } => lower_literal_expression(literal, source_id, span_range.clone()),
        TermExpression::Unary(term_unary) => lower_method_call_kind(
            unary_operator_method_name(&term_unary.operator),
            vec![HirCallArgument::positional(lower_term_expression_with_context(&term_unary.base, source_id, span_range.clone(), false))],
            span.clone(),
        ),
        TermExpression::Binary(term_binary) => {
            lower_binary_expression(&term_binary.operator, &term_binary.lhs, &term_binary.rhs, source_id, span_range.clone(), span.clone())
        }
        TermExpression::Call(term_call) => {
            lower_call_expression(&term_call.callee, &term_call.args, source_id, span_range.clone(), span.clone())
        }
        TermExpression::DotCall(term_dot) => {
            let object = lower_term_expression_with_context(&term_dot.base, source_id, span_range.clone(), false);
            let member = dot_member_name(&term_dot.caller);
            if preserve_member_access {
                lower_method_call_kind(member, vec![HirCallArgument::positional(object)], span.clone())
            }
            else if let Some(kind) = lower_postfix_effect_member(member, object.clone()) {
                kind
            }
            else if !term_dot.arguments.arguments.is_empty() {
                let mut args = vec![HirCallArgument::positional(object)];
                args.extend(term_dot.arguments.arguments.iter().map(|arg| HirCallArgument {
                    name: arg.key.as_ref().map(|key| Identifier::new(key)),
                    value: lower_term_expression_with_context(&arg.value, source_id, span_range.clone(), false),
                }));
                lower_method_call_kind(member, args, span.clone())
            }
            else {
                HirExprKind::FieldAccess { object: Box::new(object), field: Identifier::new(member) }
            }
        }
        TermExpression::Subscript(term_subscript) => {
            let mut args = vec![HirCallArgument::positional(lower_term_expression_with_context(
                &term_subscript.base,
                source_id,
                span_range.clone(),
                false,
            ))];
            args.extend(lower_subscript_arguments(term_subscript, source_id, span_range.clone()).into_iter().map(HirCallArgument::positional));
            lower_method_call_kind(subscript_operator_method_name(&term_subscript.kind, false), args, span.clone())
        }
        TermExpression::Dereference(term_dereference) => lower_method_call_kind(
            match term_dereference.kind {
                std_data::text::valkyrie::ast::DereferenceKind::ReadOnly => "deref_read",
                std_data::text::valkyrie::ast::DereferenceKind::Mutable => "deref_mut",
            },
            vec![HirCallArgument::positional(lower_term_expression_with_context(&term_dereference.base, source_id, span_range.clone(), false))],
            span.clone(),
        ),
        TermExpression::Tuple { items, .. } => lower_canonical_call_kind(
            HirExpr { kind: HirExprKind::Path(NamePath::new(vec![Identifier::new("tuple")])), span: span.clone() },
            items.iter().map(|item| lower_term_expression_with_context(item, source_id, span_range.clone(), false)).collect(),
        ),
        TermExpression::Array { items, .. } => HirExprKind::ArrayLiteral {
            items: items.iter().map(|item| lower_term_expression_with_context(item, source_id, span_range.clone(), false)).collect(),
        },
        TermExpression::Turbofish { expr, arguments, .. } => HirExprKind::GenericApply {
            callee: Box::new(lower_term_expression_with_context(expr, source_id, span_range.clone(), preserve_member_access)),
            arguments: arguments.iter().map(lower_type_expression).collect(),
        },
        TermExpression::Assign { target, value, .. } => lower_assignment_expression(target, value, source_id, span_range.clone(), span.clone()),
        TermExpression::As(term_as) => lower_term_expression_with_context(&term_as.base, source_id, span_range.clone(), false).kind,
        TermExpression::Is(term_is) => lower_term_expression_with_context(&term_is.base, source_id, span_range.clone(), false).kind,
        TermExpression::Loop(loop_stmt) => HirExprKind::Loop {
            label: None,
            pattern: None,
            iterator: None,
            condition: None,
            body: Box::new(lower_block(Some(&loop_stmt.body), source_id, span_range.clone())),
        },
        TermExpression::LoopIn(loop_in_stmt) => HirExprKind::Loop {
            label: loop_in_stmt.label.clone(),
            pattern: loop_in_stmt.pattern.as_ref().map(|pat| lower_pattern_expression(pat, source_id)),
            iterator: loop_in_stmt
                .iterator
                .as_ref()
                .map(|expr| Box::new(lower_term_expression_with_context(expr, source_id, span_range.clone(), false))),
            condition: loop_in_stmt
                .condition
                .as_ref()
                .map(|expr| Box::new(lower_term_expression_with_context(expr, source_id, span_range.clone(), false))),
            body: Box::new(lower_block(Some(&loop_in_stmt.body), source_id, span_range.clone())),
        },
        TermExpression::While(while_stmt) => HirExprKind::Loop {
            label: while_stmt.label.clone(),
            pattern: None,
            iterator: None,
            condition: while_stmt
                .condition
                .as_ref()
                .map(|expr| Box::new(lower_term_expression_with_context(expr, source_id, span_range.clone(), false))),
            body: Box::new(lower_block(Some(&while_stmt.body), source_id, span_range.clone())),
        },
        TermExpression::WhileLet(while_let_stmt) => HirExprKind::Loop {
            label: while_let_stmt.label.clone(),
            pattern: Some(lower_nested_match_pattern(&while_let_stmt.pattern, source_id, span_range.clone(), span.clone())),
            iterator: Some(Box::new(lower_term_expression_with_context(&while_let_stmt.scrutinee, source_id, span_range.clone(), false))),
            condition: while_let_stmt
                .guard
                .as_ref()
                .map(|expr| Box::new(lower_term_expression_with_context(expr, source_id, span_range.clone(), false))),
            body: Box::new(lower_block(Some(&while_let_stmt.body), source_id, span_range.clone())),
        },
        TermExpression::Until(until_stmt) => HirExprKind::Loop {
            label: until_stmt.label.clone(),
            pattern: until_stmt.pattern.as_ref().map(|pat| lower_pattern_expression(pat, source_id)),
            iterator: until_stmt
                .iterator
                .as_ref()
                .map(|expr| Box::new(lower_term_expression_with_context(expr, source_id, span_range.clone(), false))),
            condition: until_stmt.condition.as_ref().map(|expr| {
                Box::new(HirExpr {
                    kind: lower_method_call_kind(
                        unary_operator_method_name(&UnaryOperator::Not),
                        vec![HirCallArgument::positional(lower_term_expression_with_context(expr, source_id, span_range.clone(), false))],
                        span.clone(),
                    ),
                    span: span.clone(),
                })
            }),
            body: Box::new(lower_block(Some(&until_stmt.body), source_id, span_range.clone())),
        },
        TermExpression::UntilNot(until_not_stmt) => HirExprKind::Loop {
            label: until_not_stmt.label.clone(),
            pattern: until_not_stmt.pattern.as_ref().map(|pat| lower_pattern_expression(pat, source_id)),
            iterator: until_not_stmt
                .iterator
                .as_ref()
                .map(|expr| Box::new(lower_term_expression_with_context(expr, source_id, span_range.clone(), false))),
            condition: until_not_stmt
                .condition
                .as_ref()
                .map(|expr| Box::new(lower_term_expression_with_context(expr, source_id, span_range.clone(), false))),
            body: Box::new(lower_block(Some(&until_not_stmt.body), source_id, span_range.clone())),
        },
        TermExpression::IfLet(if_let_stmt) => HirExprKind::IfLet {
            pattern: lower_nested_match_pattern(&if_let_stmt.pattern, source_id, span_range.clone(), span.clone()),
            scrutinee: Box::new(lower_term_expression_with_context(&if_let_stmt.item, source_id, span_range.clone(), false)),
            then_branch: Box::new(lower_block(Some(&if_let_stmt.then_body), source_id, span_range.clone())),
            else_branch: if_let_stmt.else_body.as_ref().map(|body| Box::new(lower_block(Some(body), source_id, span_range.clone()))),
        },
        TermExpression::Raise { value, .. } => {
            HirExprKind::Raise(Box::new(lower_term_expression_with_context(value, source_id, span_range.clone(), false)))
        }
        TermExpression::If(if_stmt) => HirExprKind::If {
            condition: Box::new(lower_term_expression_with_context(&if_stmt.condition, source_id, span_range.clone(), false)),
            then_branch: Box::new(lower_block(Some(&if_stmt.then_body), source_id, span_range.clone())),
            else_branch: if_stmt.else_body.as_ref().map(|body| Box::new(lower_block(Some(body), source_id, span_range.clone()))),
        },
        TermExpression::Match { scrutinee, arms, .. } => {
            let scrutinee = Box::new(lower_term_expression_with_context(scrutinee, source_id, span_range.clone(), false));
            let arms = lower_match_arms(arms, source_id, span_range.clone(), span.clone());
            HirExprKind::Match { scrutinee, arms }
        }
        TermExpression::Catch { expr, arms, .. } => {
            let expr = Box::new(lower_term_expression_with_context(expr, source_id, span_range.clone(), false));
            let arms = lower_match_arms(arms, source_id, span_range.clone(), span.clone());
            HirExprKind::Catch { expr, arms }
        }
        TermExpression::Construct { path, fields, .. } => {
            let name = path.parts.last().map(|s| Identifier::new(s.as_str())).unwrap_or_else(|| Identifier::new("_"));
            let args = fields
                .iter()
                .map(|(field_name, value)| HirExpr {
                    kind: HirExprKind::FieldInit {
                        name: Identifier::new(field_name),
                        value: Box::new(lower_term_expression_with_context(value, source_id, span_range.clone(), false)),
                    },
                    span: span.clone(),
                })
                .collect();
            HirExprKind::Construct {
                path: crate::NamePath::new(path.parts.iter().map(|part| Identifier::new(part.as_str())).collect()),
                name,
                args,
                resolved: None,
            }
        }
        TermExpression::Lambda { params, return_type, body, .. } => {
            let hir_params = params.iter().map(|param| lower_param(param, source_id, span_range.clone())).collect();
            let hir_return_type = return_type.as_ref().map(lower_type_expression).unwrap_or(ValkyrieType::AutoType);
            let hir_body = lower_block(Some(body), source_id, span_range.clone());
            HirExprKind::Lambda { generics: Vec::new(), params: hir_params, return_type: hir_return_type, body: Box::new(hir_body) }
        }
        TermExpression::Block { body, .. } => {
            let hir_block = lower_block(Some(body), source_id, span_range.clone());
            HirExprKind::Block(Box::new(hir_block))
        }
        TermExpression::XmlMarkup { nodes, .. } => super::vx::lower_xml_markup_to_element_expr(nodes.len(), span.clone()).kind,
        TermExpression::Try(try_stmt) => {
            let body = lower_block(Some(&try_stmt.body), source_id, span_range.clone());
            HirExprKind::TryScope {
                is_optional: try_stmt.is_optional,
                is_forced: try_stmt.is_forced,
                result_type: try_stmt.result_type.as_ref().map(lower_type_expression),
                body: Box::new(body),
            }
        }
        TermExpression::Template { nodes, span } => {
            let expanded = super::tgrammar::expand_tg_root_for_lower(nodes, span.clone());
            lower_term_expression(&expanded, source_id, span_range.clone()).kind
        }
        TermExpression::PostfixMatch { base, arms, .. } => {
            let scrutinee = Box::new(lower_term_expression_with_context(base, source_id, span_range.clone(), false));
            let arms = lower_match_arms(arms, source_id, span_range.clone(), span.clone());
            HirExprKind::Match { scrutinee, arms }
        }
        TermExpression::PostfixCatch { base, arms, .. } => {
            let expr = Box::new(lower_term_expression_with_context(base, source_id, span_range.clone(), false));
            let arms = lower_match_arms(arms, source_id, span_range.clone(), span.clone());
            HirExprKind::Catch { expr, arms }
        }
        TermExpression::TryPropagate { base, .. } => {
            HirExprKind::TryPropagate(Box::new(lower_term_expression_with_context(base, source_id, span_range.clone(), false)))
        }
        TermExpression::MacroInvoke { path, args, .. } => {
            let callee = HirExpr { kind: HirExprKind::Path(lower_name_path(path)), span: span.clone() };
            let args = args
                .iter()
                .map(|arg| HirCallArgument::positional(lower_term_expression_with_context(arg, source_id, span_range.clone(), false)))
                .collect();
            HirExprKind::Call { callee: Box::new(callee), args, resolved: None }
        }
        TermExpression::AnonymousClass { is_value_type, parents, body, .. } => {
            lower_anonymous_class(*is_value_type, parents, body, source_id, span_range.clone())
        }
    };
    HirExpr { kind, span }
}

fn lower_anonymous_class(
    is_value_type: bool,
    parents: &[std_data::text::valkyrie::InheritanceItem],
    body: &std_data::text::valkyrie::ObjectBody,
    source_id: SourceID,
    fallback_span: Range<usize>,
) -> HirExprKind {
    HirExprKind::AnonymousClass {
        is_value_type,
        parents: parents.iter().map(lower_parent).collect(),
        fields: body
            .fields
            .iter()
            .filter_map(|field| {
                field.default_value.as_ref().map(|value| {
                    (field.name.name.clone(), Box::new(lower_term_expression_with_context(value, source_id, fallback_span.clone(), false)))
                })
            })
            .collect(),
        methods: body.methods.iter().map(|method| lower_inline_object_method(method, source_id)).collect(),
        captures: Vec::new(),
        class_name: None,
    }
}

fn lower_inline_object_method(method: &std_data::text::valkyrie::ObjectMethodDeclaration, source_id: SourceID) -> HirFunction {
    HirFunction {
        name: method.name.name.clone(),
        declaring_namespace: NamePath::default(),
        doc: lower_documentation(&method.annotations),
        annotations: method.annotations.attributes().map(|attribute| lower_attribute(attribute, source_id, method.span.clone())).collect(),
        generics: Vec::new(),
        params: lower_method_params(method, source_id),
        return_type: method.return_type.as_ref().map(lower_type_expression).unwrap_or(ValkyrieType::Unit),
        body: lower_block(method.body.as_ref(), source_id, method.span.clone()),
        span: with_source(&method.span, source_id),
        visibility: lower_visibility(&method.annotations),
        is_abstract: method.body.is_none() || has_modifier(&method.annotations, "abstract"),
        is_final: has_modifier(&method.annotations, "final"),
        is_virtual: false,
        is_override: false,
    }
}

fn lower_postfix_effect_member(member: &str, object: HirExpr) -> Option<HirExprKind> {
    match member {
        "await" => Some(HirExprKind::Await(Box::new(object))),
        "awake" => Some(HirExprKind::Awake(Box::new(object))),
        "block" => Some(HirExprKind::BlockOn(Box::new(object))),
        _ => None,
    }
}

fn lower_match_arms(
    arms: &[std_data::text::valkyrie::ast::ArmStatement],
    source_id: SourceID,
    fallback_span: Range<usize>,
    span: SourceSpan,
) -> Vec<HirMatchArm> {
    arms.iter()
        .filter_map(|arm| match arm {
            std_data::text::valkyrie::ast::ArmStatement::Case(arm) => Some((arm.pattern.as_ref(), arm.guard.as_ref(), &arm.body)),
            std_data::text::valkyrie::ast::ArmStatement::Type(arm) => Some((None, arm.guard.as_ref(), &arm.body)),
            std_data::text::valkyrie::ast::ArmStatement::Else(_) => None,
        })
        .map(|(pattern, guard, body)| {
            let pattern = match pattern {
                Some(PatternExpression::Extract(pattern)) => {
                    let name = &pattern.name;
                    let lowered_name = lower_name_path(name);
                    let fields = &pattern.fields;
                    let fields: Vec<HirPattern> = fields
                        .iter()
                        .map(|pattern| lower_nested_match_pattern(pattern, source_id, fallback_span.clone(), span.clone()))
                        .collect();
                    HirPattern::Extractor(crate::types::hir::HirExtractorPattern::Constructor {
                        name: lowered_name.clone(),
                        canonical_callee: canonical_extractor_callee(&lowered_name),
                        fields,
                        resolved: None,
                    })
                }
                Some(PatternExpression::Variable { name, .. }) => {
                    HirPattern::Variable(HirIdentifier { name: Identifier::new(name), shadow_index: 0, span: span.clone() })
                }
                Some(PatternExpression::Wildcard { .. }) => HirPattern::Wildcard,
                Some(PatternExpression::Literal { literal, .. }) => lower_match_literal_pattern(literal, source_id, fallback_span.clone()),
                Some(PatternExpression::Range { start, end, inclusive_end, .. }) => HirPattern::Range {
                    start: start.as_ref().map(|literal| lower_match_bound_literal(literal, source_id, fallback_span.clone())),
                    end: end.as_ref().map(|literal| lower_match_bound_literal(literal, source_id, fallback_span.clone())),
                    inclusive_end: *inclusive_end,
                },
                Some(PatternExpression::Array(pattern)) => {
                    let prefix = &pattern.prefix;
                    let rest = &pattern.rest;
                    let suffix = &pattern.suffix;
                    HirPattern::Extractor(crate::types::hir::HirExtractorPattern::Array {
                        canonical_callee: NamePath::new(vec![Identifier::new("array"), Identifier::new("extractor")]),
                        prefix: prefix
                            .iter()
                            .map(|pattern| lower_nested_match_pattern(pattern, source_id, fallback_span.clone(), span.clone()))
                            .collect(),
                        rest: rest.as_ref().map(|name| HirIdentifier { name: name.clone(), shadow_index: 0, span: span.clone() }),
                        suffix: suffix
                            .iter()
                            .map(|pattern| lower_nested_match_pattern(pattern, source_id, fallback_span.clone(), span.clone()))
                            .collect(),
                        resolved: None,
                    })
                }
                Some(PatternExpression::Tuple(pattern)) => HirPattern::Tuple(
                    pattern
                        .items
                        .iter()
                        .map(|pattern| lower_nested_match_pattern(pattern, source_id, fallback_span.clone(), span.clone()))
                        .collect(),
                ),
                Some(PatternExpression::Name { path, .. }) => HirPattern::Name(lower_name_path(path)),
                Some(PatternExpression::TypedBind { name, ty, .. }) => HirPattern::TypedBind {
                    identifier: HirIdentifier { name: Identifier::new(name), shadow_index: 0, span: span.clone() },
                    ty: lower_name_path(ty),
                },
                Some(PatternExpression::Or(pattern)) => HirPattern::Or(
                    pattern
                        .patterns
                        .iter()
                        .map(|pattern| lower_nested_match_pattern(pattern, source_id, fallback_span.clone(), span.clone()))
                        .collect(),
                ),
                Some(PatternExpression::Object(pattern)) => HirPattern::Object {
                    name: pattern.name.as_ref().map(lower_name_path),
                    fields: pattern
                        .fields
                        .iter()
                        .map(|field| {
                            let identifier = Identifier::new(&field.name);
                            let pattern = lower_nested_match_pattern(&field.pattern, source_id, fallback_span.clone(), span.clone());
                            (identifier, pattern)
                        })
                        .collect(),
                    rest: pattern.rest.as_ref().map(|name| HirIdentifier { name: name.clone(), shadow_index: 0, span: span.clone() }),
                },
                Some(PatternExpression::Bind { name, pattern, .. }) => HirPattern::Bind {
                    identifier: HirIdentifier { name: Identifier::new(name), shadow_index: 0, span: span.clone() },
                    pattern: Box::new(lower_nested_match_pattern(pattern, source_id, fallback_span.clone(), span.clone())),
                },
                Some(PatternExpression::Mut { pattern, .. }) => {
                    HirPattern::Mut(Box::new(lower_nested_match_pattern(pattern, source_id, fallback_span.clone(), span.clone())))
                }
                Some(PatternExpression::Pin { mutable, pattern, .. }) => HirPattern::Pin {
                    mutable: *mutable,
                    pattern: Box::new(lower_nested_match_pattern(pattern, source_id, fallback_span.clone(), span.clone())),
                },
                None => HirPattern::Else,
            };
            let guard =
                guard.map(|guard_expr| Box::new(lower_term_expression_with_context(guard_expr, source_id, fallback_span.clone(), false)));
            let body_block = lower_block(Some(body), source_id, fallback_span.clone());
            let body = Box::new(HirExpr { kind: HirExprKind::Block(Box::new(body_block)), span: span.clone() });
            HirMatchArm { pattern, guard, body }
        })
        .collect()
}

fn lower_nested_match_pattern(pattern: &PatternExpression, source_id: SourceID, fallback_span: Range<usize>, span: SourceSpan) -> HirPattern {
    match pattern {
        PatternExpression::Extract(pattern) => {
            let lowered_name = lower_name_path(&pattern.name);
            HirPattern::Extractor(crate::types::hir::HirExtractorPattern::Constructor {
                name: lowered_name.clone(),
                canonical_callee: canonical_extractor_callee(&lowered_name),
                fields: pattern
                    .fields
                    .iter()
                    .map(|pattern| lower_nested_match_pattern(pattern, source_id, fallback_span.clone(), span.clone()))
                    .collect(),
                resolved: None,
            })
        }
        PatternExpression::Variable { name, .. } => HirPattern::Variable(HirIdentifier { name: Identifier::new(name), shadow_index: 0, span }),
        PatternExpression::Wildcard { .. } => HirPattern::Wildcard,
        PatternExpression::Literal { literal, .. } => lower_match_literal_pattern(literal, source_id, fallback_span),
        PatternExpression::Range { start, end, inclusive_end, .. } => HirPattern::Range {
            start: start.as_ref().map(|literal| lower_match_bound_literal(literal, source_id, fallback_span.clone())),
            end: end.as_ref().map(|literal| lower_match_bound_literal(literal, source_id, fallback_span.clone())),
            inclusive_end: *inclusive_end,
        },
        PatternExpression::Array(pattern) => HirPattern::Extractor(crate::types::hir::HirExtractorPattern::Array {
            canonical_callee: NamePath::new(vec![Identifier::new("array"), Identifier::new("extractor")]),
            prefix: pattern
                .prefix
                .iter()
                .map(|pattern| lower_nested_match_pattern(pattern, source_id, fallback_span.clone(), span.clone()))
                .collect(),
            rest: pattern.rest.as_ref().map(|name| HirIdentifier { name: name.clone(), shadow_index: 0, span: span.clone() }),
            suffix: pattern
                .suffix
                .iter()
                .map(|pattern| lower_nested_match_pattern(pattern, source_id, fallback_span.clone(), span.clone()))
                .collect(),
            resolved: None,
        }),
        PatternExpression::Tuple(pattern) => HirPattern::Tuple(
            pattern.items.iter().map(|pattern| lower_nested_match_pattern(pattern, source_id, fallback_span.clone(), span.clone())).collect(),
        ),
        PatternExpression::Name { path, .. } => HirPattern::Name(lower_name_path(path)),
        PatternExpression::TypedBind { name, ty, .. } => HirPattern::TypedBind {
            identifier: HirIdentifier { name: Identifier::new(name), shadow_index: 0, span: span.clone() },
            ty: lower_name_path(ty),
        },
        PatternExpression::Or(pattern) => HirPattern::Or(
            pattern
                .patterns
                .iter()
                .map(|pattern| lower_nested_match_pattern(pattern, source_id, fallback_span.clone(), span.clone()))
                .collect(),
        ),
        PatternExpression::Object(pattern) => HirPattern::Object {
            name: pattern.name.as_ref().map(lower_name_path),
            fields: pattern
                .fields
                .iter()
                .map(|field| {
                    let identifier = Identifier::new(&field.name);
                    let pattern = lower_nested_match_pattern(&field.pattern, source_id, fallback_span.clone(), span.clone());
                    (identifier, pattern)
                })
                .collect(),
            rest: pattern.rest.as_ref().map(|name| HirIdentifier { name: name.clone(), shadow_index: 0, span: span.clone() }),
        },
        PatternExpression::Bind { name, pattern, .. } => HirPattern::Bind {
            identifier: HirIdentifier { name: Identifier::new(name), shadow_index: 0, span: span.clone() },
            pattern: Box::new(lower_nested_match_pattern(pattern, source_id, fallback_span.clone(), span.clone())),
        },
        PatternExpression::Mut { pattern, .. } => {
            HirPattern::Mut(Box::new(lower_nested_match_pattern(pattern, source_id, fallback_span.clone(), span.clone())))
        }
        PatternExpression::Pin { mutable, pattern, .. } => HirPattern::Pin {
            mutable: *mutable,
            pattern: Box::new(lower_nested_match_pattern(pattern, source_id, fallback_span.clone(), span.clone())),
        },
    }
}

fn canonical_extractor_callee(name: &NamePath) -> NamePath {
    let mut parts = name.parts().to_vec();
    parts.push(Identifier::new("extractor"));
    NamePath::new(parts)
}

fn lower_match_literal_pattern(literal: &LiteralExpression, source_id: SourceID, fallback_span: Range<usize>) -> HirPattern {
    match lower_literal_expression(literal, source_id, fallback_span) {
        HirExprKind::Literal(literal) => HirPattern::Literal(literal),
        _ => unreachable!("literal pattern lowering must produce literal hir expr"),
    }
}

fn lower_match_bound_literal(literal: &LiteralExpression, source_id: SourceID, fallback_span: Range<usize>) -> HirLiteral {
    match lower_match_literal_pattern(literal, source_id, fallback_span) {
        HirPattern::Literal(literal) => literal,
        _ => unreachable!("range bound lowering must produce literal hir pattern"),
    }
}

fn lower_assignment_expression(
    target: &TermExpression,
    value: &TermExpression,
    source_id: SourceID,
    fallback_span: Range<usize>,
    span: SourceSpan,
) -> HirExprKind {
    let value = lower_term_expression_with_context(value, source_id, fallback_span.clone(), false);
    match target {
        TermExpression::DotCall(term_dot) => {
            let object = lower_term_expression_with_context(&term_dot.base, source_id, fallback_span, false);
            HirExprKind::StoreField {
                object: Box::new(object),
                field: Identifier::new(dot_member_name(&term_dot.caller)),
                value: Box::new(value),
            }
        }
        TermExpression::Subscript(term_subscript) => {
            let mut args = vec![HirCallArgument::positional(lower_term_expression_with_context(
                &term_subscript.base,
                source_id,
                fallback_span.clone(),
                false,
            ))];
            args.extend(
                lower_subscript_arguments(term_subscript, source_id, fallback_span.clone()).into_iter().map(HirCallArgument::positional),
            );
            args.push(HirCallArgument::positional(value));
            lower_method_call_kind(subscript_operator_method_name(&term_subscript.kind, true), args, span)
        }
        TermExpression::Name { path, .. } if path.parts.len() == 1 => {
            HirExprKind::Assign { target: Identifier::new(&path.parts[0]), value: Box::new(value) }
        }
        _ => lower_canonical_call_kind(
            HirExpr { kind: HirExprKind::Path(NamePath::new(vec![Identifier::new("unsupported_assignment")])), span: span.clone() },
            vec![value],
        ),
    }
}

fn lower_call_expression(
    callee: &TermExpression,
    args: &std_data::text::valkyrie::ast::TermArguments,
    source_id: SourceID,
    fallback_span: Range<usize>,
    span: SourceSpan,
) -> HirExprKind {
    let lowered_args = lower_call_arguments(&args.arguments, source_id, fallback_span.clone());
    if let TermExpression::DotCall(term_dot) = callee {
        if is_self_rooted_member_chain(&term_dot.base) {
            let mut method_args =
                vec![HirCallArgument::positional(lower_term_expression_with_context(&term_dot.base, source_id, fallback_span.clone(), false))];
            method_args.extend(lowered_args);
            return lower_method_call_kind(dot_member_name(&term_dot.caller), method_args, span);
        }
    }

    if let TermExpression::Turbofish { expr, arguments, .. } = callee {
        if let TermExpression::DotCall(term_dot) = expr.as_ref() {
            if is_self_rooted_member_chain(&term_dot.base) {
                let mut method_args = vec![HirCallArgument::positional(lower_term_expression_with_context(
                    &term_dot.base,
                    source_id,
                    fallback_span.clone(),
                    false,
                ))];
                method_args.extend(lowered_args);
                return lower_canonical_call_arguments(
                    HirExpr {
                        kind: HirExprKind::GenericApply {
                            callee: Box::new(HirExpr {
                                kind: HirExprKind::Path(NamePath::new(vec![Identifier::new(dot_member_name(&term_dot.caller))])),
                                span: span.clone(),
                            }),
                            arguments: arguments.iter().map(lower_type_expression).collect(),
                        },
                        span: span.clone(),
                    },
                    method_args,
                );
            }
        }
    }

    if let Some(parts) = extract_dotted_path(callee) {
        return lower_canonical_call_arguments(
            HirExpr { kind: HirExprKind::Path(NamePath::new(parts.iter().map(|p| Identifier::new(p.as_str())).collect())), span: span.clone() },
            lowered_args,
        );
    }

    if let TermExpression::DotCall(term_dot) = callee {
        let mut method_args =
            vec![HirCallArgument::positional(lower_term_expression_with_context(&term_dot.base, source_id, fallback_span.clone(), false))];
        method_args.extend(lowered_args);
        return lower_method_call_kind(dot_member_name(&term_dot.caller), method_args, span);
    }

    if let TermExpression::Turbofish { expr, arguments, .. } = callee {
        if let TermExpression::DotCall(term_dot) = expr.as_ref() {
            let mut method_args =
                vec![HirCallArgument::positional(lower_term_expression_with_context(&term_dot.base, source_id, fallback_span.clone(), false))];
            method_args.extend(lowered_args);
            return lower_canonical_call_arguments(
                HirExpr {
                    kind: HirExprKind::GenericApply {
                        callee: Box::new(HirExpr {
                            kind: HirExprKind::Path(NamePath::new(vec![Identifier::new(dot_member_name(&term_dot.caller))])),
                            span: span.clone(),
                        }),
                        arguments: arguments.iter().map(lower_type_expression).collect(),
                    },
                    span: span.clone(),
                },
                method_args,
            );
        }
    }

    lower_canonical_call_arguments(lower_term_expression_with_context(callee, source_id, fallback_span.clone(), true), lowered_args)
}

fn lower_call_arguments(
    args: &[std_data::text::valkyrie::ast::TermCallArgument],
    source_id: SourceID,
    fallback_span: Range<usize>,
) -> Vec<HirCallArgument> {
    args.iter()
        .map(|arg| HirCallArgument {
            name: arg.key.as_ref().map(|key| Identifier::new(key)),
            value: lower_term_expression_with_context(&arg.value, source_id, fallback_span.clone(), false),
        })
        .collect()
}

fn lower_method_call_kind(member: &str, args: Vec<HirCallArgument>, span: SourceSpan) -> HirExprKind {
    lower_canonical_call_arguments(HirExpr { kind: HirExprKind::Path(NamePath::new(vec![Identifier::new(member)])), span: span.clone() }, args)
}

fn lower_binary_expression(
    op: &BinaryOperator,
    lhs: &TermExpression,
    rhs: &TermExpression,
    source_id: SourceID,
    fallback_span: Range<usize>,
    span: SourceSpan,
) -> HirExprKind {
    match op {
        BinaryOperator::And => lower_short_circuit_and(lhs, rhs, source_id, fallback_span, span),
        BinaryOperator::Or => lower_short_circuit_or(lhs, rhs, source_id, fallback_span, span),
        BinaryOperator::Pipe => lower_pipe_expression(lhs, rhs, source_id, fallback_span, span),
        _ => lower_method_call_kind(
            binary_operator_method_name(op),
            vec![
                HirCallArgument::positional(lower_term_expression_with_context(lhs, source_id, fallback_span.clone(), false)),
                HirCallArgument::positional(lower_term_expression_with_context(rhs, source_id, fallback_span, false)),
            ],
            span,
        ),
    }
}

fn lower_pipe_expression(
    lhs: &TermExpression,
    rhs: &TermExpression,
    source_id: SourceID,
    fallback_span: Range<usize>,
    _span: SourceSpan,
) -> HirExprKind {
    let arg = lower_term_expression_with_context(lhs, source_id, fallback_span.clone(), false);

    if let TermExpression::Call(term_call) = rhs {
        let callee = lower_term_expression_with_context(&term_call.callee, source_id, fallback_span.clone(), false);
        let mut all_args = vec![HirCallArgument::positional(arg)];
        all_args.extend(lower_call_arguments(&term_call.args.arguments, source_id, fallback_span.clone()));
        return lower_canonical_call_arguments(callee, all_args);
    }

    let callee = lower_term_expression_with_context(rhs, source_id, fallback_span, false);
    lower_canonical_call_arguments(callee, vec![HirCallArgument::positional(arg)])
}

fn lower_canonical_call_arguments(callee: HirExpr, args: Vec<HirCallArgument>) -> HirExprKind {
    HirExprKind::Call { callee: Box::new(callee), args, resolved: None }
}

fn lower_canonical_call_kind(callee: HirExpr, args: Vec<HirExpr>) -> HirExprKind {
    lower_canonical_call_arguments(callee, args.into_iter().map(HirCallArgument::positional).collect())
}

fn lower_short_circuit_and(
    lhs: &TermExpression,
    rhs: &TermExpression,
    source_id: SourceID,
    fallback_span: Range<usize>,
    span: SourceSpan,
) -> HirExprKind {
    let condition = lower_term_expression_with_context(lhs, source_id, fallback_span.clone(), false);
    let rhs_expr = lower_term_expression_with_context(rhs, source_id, fallback_span, false);
    HirExprKind::If {
        condition: Box::new(condition),
        then_branch: Box::new(HirBlock { statements: Vec::new(), expr: Some(Box::new(rhs_expr)), span: span.clone() }),
        else_branch: Some(Box::new(HirBlock {
            statements: Vec::new(),
            expr: Some(Box::new(HirExpr { kind: HirExprKind::Literal(HirLiteral::Bool(false)), span: span.clone() })),
            span,
        })),
    }
}

fn lower_short_circuit_or(
    lhs: &TermExpression,
    rhs: &TermExpression,
    source_id: SourceID,
    fallback_span: Range<usize>,
    span: SourceSpan,
) -> HirExprKind {
    let condition = lower_term_expression_with_context(lhs, source_id, fallback_span.clone(), false);
    let rhs_expr = lower_term_expression_with_context(rhs, source_id, fallback_span, false);
    HirExprKind::If {
        condition: Box::new(condition),
        then_branch: Box::new(HirBlock {
            statements: Vec::new(),
            expr: Some(Box::new(HirExpr { kind: HirExprKind::Literal(HirLiteral::Bool(true)), span: span.clone() })),
            span: span.clone(),
        }),
        else_branch: Some(Box::new(HirBlock { statements: Vec::new(), expr: Some(Box::new(rhs_expr)), span })),
    }
}

fn lower_name_expression(path: &AstNamePath, span: SourceSpan) -> HirExprKind {
    let path = lower_name_path(path);
    if path.parts().len() == 1 {
        HirExprKind::Variable(HirIdentifier { name: path.parts()[0].clone(), shadow_index: 0, span })
    }
    else {
        HirExprKind::Path(path)
    }
}

fn parse_integer_literal(text: &str) -> Result<i64, std::num::ParseIntError> {
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        return i64::from_str_radix(hex, 16);
    }
    if let Some(bin) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
        return i64::from_str_radix(bin, 2);
    }
    if let Some(oct) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
        return i64::from_str_radix(oct, 8);
    }
    text.parse::<i64>()
}

fn lower_literal_expression(literal: &LiteralExpression, source_id: SourceID, fallback_span: Range<usize>) -> HirExprKind {
    match literal {
        LiteralExpression::Integer(value) => parse_integer_literal(value)
            .map(HirLiteral::Integer64)
            .map(HirExprKind::Literal)
            .unwrap_or_else(|_| HirExprKind::Literal(HirLiteral::Integer64(0))),
        LiteralExpression::Float(value) => value
            .parse::<f64>()
            .map(|v| HirExprKind::Literal(HirLiteral::Float64(OrderedFloat(v))))
            .unwrap_or_else(|_| HirExprKind::Literal(HirLiteral::Float64(OrderedFloat(0.0)))),
        LiteralExpression::String(value) => HirExprKind::Literal(HirLiteral::String(lower_string_literal(value, source_id, fallback_span))),
        LiteralExpression::Bool(value) => HirExprKind::Literal(HirLiteral::Bool(*value)),
        LiteralExpression::Unit => HirExprKind::Literal(HirLiteral::Unit),
        LiteralExpression::Null => HirExprKind::Path(NamePath::new(vec![Identifier::new("null")])),
    }
}

fn lower_string_literal(literal: &AstStringLiteral, source_id: SourceID, fallback_span: Range<usize>) -> crate::types::hir::HirStringLiteral {
    crate::types::hir::HirStringLiteral {
        prefix: literal.prefix.as_deref().map(Identifier::new),
        quote_count: literal.quote_count,
        segments: literal
            .segments
            .iter()
            .map(|segment| match segment {
                AstStringSegment::Text(text) => crate::types::hir::HirStringSegment::Text(text.clone()),
                AstStringSegment::Interpolation { expression, is_fluent } => crate::types::hir::HirStringSegment::Interpolation {
                    expr: lower_term_expression_with_context(expression, source_id, fallback_span.clone(), false),
                    is_fluent: *is_fluent,
                },
            })
            .collect(),
    }
}

fn binary_operator_method_name(op: &BinaryOperator) -> &'static str {
    match op {
        BinaryOperator::And => unreachable!("&& 走短路控制流，不进入 operator method lowering"),
        BinaryOperator::Or => unreachable!("|| 走短路控制流，不进入 operator method lowering"),
        BinaryOperator::Add => "infix +",
        BinaryOperator::Sub => "infix -",
        BinaryOperator::Mul => "infix *",
        BinaryOperator::Div => "infix /",
        BinaryOperator::Rem => "infix %",
        BinaryOperator::Eq => "infix ==",
        BinaryOperator::Ne => "infix !=",
        BinaryOperator::Lt => "infix <",
        BinaryOperator::Le => "infix <=",
        BinaryOperator::Gt => "infix >",
        BinaryOperator::Ge => "infix >=",
        BinaryOperator::Shl => "infix <<",
        BinaryOperator::Shr => "infix >>",
        BinaryOperator::BitAnd => "infix &",
        BinaryOperator::BitOr => "infix |",
        BinaryOperator::Power => "infix ^",
        BinaryOperator::Range => "infix ..",
        BinaryOperator::RangeInclusive => "infix ..=",
        BinaryOperator::RangeTo => "infix ..<",
        BinaryOperator::Pipe => unreachable!("|> 管道操作符走函数调用 lowering，不进入 operator method lowering"),
    }
}

fn unary_operator_method_name(op: &UnaryOperator) -> &'static str {
    match op {
        UnaryOperator::Neg => "prefix -",
        UnaryOperator::Not => "prefix !",
    }
}

fn subscript_operator_method_name(kind: &SubscriptKind, is_assignment: bool) -> &'static str {
    match (kind, is_assignment) {
        (SubscriptKind::Ordinal, false) => "suffix []",
        (SubscriptKind::Ordinal, true) => "suffix []=",
        (SubscriptKind::Cardinal, false) => "suffix ⁅⁆",
        (SubscriptKind::Cardinal, true) => "suffix ⁅⁆=",
    }
}

pub(super) fn extract_name_path(expression: &TermExpression) -> Option<NamePath> {
    match expression {
        TermExpression::Name { path, .. } => Some(lower_name_path(path)),
        TermExpression::Literal { literal: LiteralExpression::String(text), .. } => {
            let raw = plain_string_literal_text(text)?;
            Some(NamePath::new(raw.split("::").filter(|part| !part.is_empty()).map(Identifier::new).collect()))
        }
        _ => None,
    }
}

fn extract_dotted_path(expr: &TermExpression) -> Option<Vec<String>> {
    match expr {
        TermExpression::Name { path, .. } => {
            if path.parts.is_empty() {
                None
            }
            else {
                Some(path.parts.clone())
            }
        }
        TermExpression::DotCall(term_dot) => {
            let mut parts = extract_dotted_path(&term_dot.base)?;
            parts.extend(term_dot.caller.parts.clone());
            Some(parts)
        }
        _ => None,
    }
}

fn is_self_rooted_member_chain(expr: &TermExpression) -> bool {
    match expr {
        TermExpression::Name { path, .. } => path.parts.len() == 1 && path.parts[0] == "self",
        TermExpression::DotCall(term_dot) => is_self_rooted_member_chain(&term_dot.base),
        _ => false,
    }
}

fn dot_member_name(path: &AstNamePath) -> &str {
    path.parts.last().map(|part| part.as_str()).unwrap_or("_")
}

fn lower_optional_term_expression(
    expression: Option<&TermExpression>,
    source_id: SourceID,
    fallback_span: Range<usize>,
    span: SourceSpan,
) -> HirExpr {
    expression
        .map(|expr| lower_term_expression_with_context(expr, source_id, fallback_span, false))
        .unwrap_or(HirExpr { kind: HirExprKind::Literal(HirLiteral::Unit), span })
}

fn lower_subscript_arguments(
    term_subscript: &std_data::text::valkyrie::ast::TermSubscriptExpression,
    source_id: SourceID,
    fallback_span: Range<usize>,
) -> Vec<HirExpr> {
    let mut arguments = Vec::new();
    for subscript in &term_subscript.subscripts {
        match subscript {
            std_data::text::valkyrie::ast::SubscriptItem::Index { term, .. } => {
                arguments.push(lower_term_expression_with_context(term, source_id, fallback_span.clone(), false));
            }
            std_data::text::valkyrie::ast::SubscriptItem::Slice { start, end, step, .. } => {
                for value in [start.as_ref(), end.as_ref(), step.as_ref()] {
                    arguments.push(lower_optional_term_expression(
                        value.map(|value| value),
                        source_id,
                        fallback_span.clone(),
                        with_source(&fallback_span, source_id),
                    ));
                }
            }
        }
    }
    arguments
}

fn plain_string_literal_text(literal: &AstStringLiteral) -> Option<&str> {
    if literal.segments.len() != 1 {
        return None;
    }

    match &literal.segments[0] {
        AstStringSegment::Text(text) => Some(text.as_str()),
        AstStringSegment::Interpolation { .. } => None,
    }
}
