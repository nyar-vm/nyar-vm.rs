use super::*;
use crate::{
    types::hir::{CaptureMode, CaptureStorage, HirCapture},
    valkyrie::frontend::ast::{
        AnonymousClass, AnonymousMicro, Block, Break, ClassPattern, Continue, LiteralPattern, MatchArm, Pattern, Raise, Resume, Return,
        Statement, StringLiteral, StringSegment, TermBinaryNode, TermExpression, TermUnaryNode, TextSegment, VariablePattern, WildcardPattern,
        pattern_nodes::ElsePattern,
    },
};
use nyar_types::{OperatorFixity, OperatorId, builtin_operator};
use oak_valkyrie::lexer::token_type::ValkyrieTokenType;

pub(super) fn lower_block(body: &Block, source_id: SourceID, fallback_span: Range<usize>) -> HirBlock {
    let span_range = if body.span.is_empty() { fallback_span } else { frontend::std_range(&body.span) };
    let (statements, tail) = split_block_tail(&body.statements, source_id, span_range.clone());
    let expr = tail.map(|expr| Box::new(lower_term_expression(&expr, source_id, span_range.clone())));
    HirBlock { statements, expr, span: with_source(&span_range, source_id) }
}

fn split_block_tail(statements: &[Statement], source_id: SourceID, fallback_span: Range<usize>) -> (Vec<HirStatement>, Option<TermExpression>) {
    if statements.is_empty() {
        return (Vec::new(), None);
    }
    let mut lowered = statements.iter().map(|statement| lower_statement(statement, source_id, fallback_span.clone())).collect::<Vec<_>>();
    if let Some(Statement::ExprStmt(expr_stmt)) = statements.last() {
        if !expr_stmt.semi {
            lowered.pop();
            return (lowered, Some(expr_stmt.expr.clone()));
        }
    }
    (lowered, None)
}

fn lower_statement(statement: &Statement, source_id: SourceID, fallback_span: Range<usize>) -> HirStatement {
    match statement {
        Statement::Let(binding) => {
            let span_range = if binding.span.is_empty() { fallback_span } else { frontend::std_range(&binding.span) };
            let span = with_source(&span_range, source_id);
            HirStatement {
                kind: HirStatementKind::Let {
                    is_mutable: has_modifier(&binding.annotations, "mut"),
                    pattern: lower_pattern(&binding.pattern, source_id, span.clone()),
                    initializer: Some(Box::new(lower_term_expression(&binding.expr, source_id, span_range.clone()))),
                    ty: binding.ty.as_ref().map(lower_type_expression),
                },
                span,
            }
        }
        Statement::ExprStmt(expr_stmt) => {
            let span_range = if expr_stmt.span.is_empty() { fallback_span } else { frontend::std_range(&expr_stmt.span) };
            let span = with_source(&span_range, source_id);
            let expr = lower_statement_expression(&expr_stmt.expr, source_id, span_range, span.clone());
            HirStatement { kind: HirStatementKind::Expr(Box::new(expr)), span }
        }
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

fn lower_pattern(pattern: &Pattern, source_id: SourceID, span: SourceSpan) -> HirPattern {
    match pattern {
        Pattern::Wildcard(wildcard) => HirPattern::Wildcard,
        Pattern::Variable(variable) => HirPattern::Variable(HirIdentifier {
            name: Identifier::new(&variable.name.name),
            shadow_index: 0,
            span: with_source(&frontend::std_range(&variable.span), source_id),
        }),
        Pattern::Literal(literal) => HirPattern::Literal(parse_pattern_literal(&literal.value)),
        Pattern::Type(type_pattern) => HirPattern::Name(lower_name_path(&type_pattern.name)),
        Pattern::Class(class_pattern) => lower_class_pattern(class_pattern, source_id, span),
        Pattern::Else(_) => HirPattern::Else,
    }
}

fn lower_class_pattern(pattern: &ClassPattern, source_id: SourceID, span: SourceSpan) -> HirPattern {
    let name = lower_name_path(&pattern.name);
    let fields = pattern
        .fields
        .iter()
        .map(|(field, nested)| {
            let field_pattern = nested
                .as_ref()
                .map(|nested| lower_pattern(nested, source_id, span.clone()))
                .unwrap_or(HirPattern::Variable(HirIdentifier { name: Identifier::new(&field.name), shadow_index: 0, span: span.clone() }));
            (Identifier::new(&field.name), field_pattern)
        })
        .collect();
    HirPattern::Object { name: Some(name), fields, rest: None }
}

fn parse_pattern_literal(value: &str) -> HirLiteral {
    if value == "true" || value == "false" {
        HirLiteral::Bool(value == "true")
    }
    else if let Ok(value) = value.parse::<i64>() {
        HirLiteral::Integer64(value)
    }
    else if let Ok(value) = value.parse::<f64>() {
        HirLiteral::Float64(OrderedFloat(value))
    }
    else {
        HirLiteral::String(crate::types::hir::HirStringLiteral {
            prefix: None,
            quote_count: 1,
            segments: vec![crate::types::hir::HirStringSegment::Text(value.to_string())],
        })
    }
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
    let span_range = {
        let span = expression.span();
        if span.is_empty() { fallback_span } else { frontend::std_range(&span) }
    };
    let span = with_source(&span_range, source_id);
    let kind = match expression {
        TermExpression::NamePath(path) => lower_name_expression(path, span.clone()),
        TermExpression::StringLiteral(literal) => lower_string_literal_kind(literal, source_id, span_range.clone()),
        TermExpression::IntegerLiteral { value, .. } => parse_integer_literal(value)
            .map(HirLiteral::Integer64)
            .map(HirExprKind::Literal)
            .unwrap_or_else(|_| HirExprKind::Literal(HirLiteral::Integer64(0))),
        TermExpression::FloatLiteral { value, .. } => value
            .parse::<f64>()
            .map(|v| HirExprKind::Literal(HirLiteral::Float64(OrderedFloat(v))))
            .unwrap_or_else(|_| HirExprKind::Literal(HirLiteral::Float64(OrderedFloat(0.0)))),
        TermExpression::Bool { value, .. } => HirExprKind::Literal(HirLiteral::Bool(*value)),
        TermExpression::Unary(node) => lower_unary_expression(node, source_id, span_range.clone(), span.clone()),
        TermExpression::Binary(node) => lower_binary_expression(node, source_id, span_range.clone(), span.clone()),
        TermExpression::Turbofish { expr, arguments, .. } => HirExprKind::GenericApply {
            callee: Box::new(lower_term_expression_with_context(expr, source_id, span_range.clone(), false)),
            arguments: arguments.iter().map(super::lower_type_expression).collect(),
        },
        TermExpression::ApplyCall { callee, args, .. } => lower_call_expression(callee, args, source_id, span_range.clone(), span.clone()),
        TermExpression::DotCall { receiver, field, .. } => {
            let object = lower_term_expression_with_context(receiver, source_id, span_range.clone(), false);
            let member = field.name.as_str();
            if preserve_member_access {
                lower_method_call_kind(member, vec![HirCallArgument::positional(object)], span.clone())
            }
            else if let Some(kind) = lower_postfix_effect_member(member, object.clone()) {
                kind
            }
            else {
                HirExprKind::FieldAccess { object: Box::new(object), field: Identifier::new(member) }
            }
        }
        TermExpression::Index { receiver, index, .. } => lower_operator_call_kind(
            subscript_operator_id(false),
            vec![
                HirCallArgument::positional(lower_term_expression_with_context(receiver, source_id, span_range.clone(), false)),
                HirCallArgument::positional(lower_term_expression_with_context(index, source_id, span_range.clone(), false)),
            ],
            span.clone(),
        ),
        TermExpression::Offset { receiver, offset, .. } => lower_operator_call_kind(
            registered_operator(OperatorFixity::Infix, "+"),
            vec![
                HirCallArgument::positional(lower_term_expression_with_context(receiver, source_id, span_range.clone(), false)),
                HirCallArgument::positional(lower_term_expression_with_context(offset, source_id, span_range.clone(), false)),
            ],
            span.clone(),
        ),
        TermExpression::Paren { expr, .. } => {
            lower_term_expression_with_context(expr, source_id, span_range.clone(), preserve_member_access).kind
        }
        TermExpression::Block(block) => HirExprKind::Block(Box::new(lower_block(block, source_id, span_range.clone()))),
        TermExpression::Micro(lambda) => {
            let hir_params = lambda.params.iter().map(|param| lower_param(param, source_id, span_range.clone())).collect();
            let hir_return_type = lambda.return_type.as_ref().map(lower_type_expression).unwrap_or(ValkyrieType::AutoType);
            let hir_body = lower_block(&lambda.body, source_id, span_range.clone());
            HirExprKind::Lambda { generics: Vec::new(), params: hir_params, return_type: hir_return_type, body: Box::new(hir_body) }
        }
        TermExpression::Object { callee, fields, .. } => lower_object_expression(callee, fields, source_id, span_range.clone(), span.clone()),
        TermExpression::AnonymousClass(class) => lower_anonymous_class(class, source_id, span_range.clone()),
        TermExpression::If { pattern, condition, then_branch, else_branch, .. } => {
            if let Some(pattern) = pattern {
                HirExprKind::IfLet {
                    pattern: lower_pattern(pattern, source_id, span.clone()),
                    scrutinee: Box::new(lower_term_expression_with_context(condition, source_id, span_range.clone(), false)),
                    then_branch: Box::new(lower_block(then_branch, source_id, span_range.clone())),
                    else_branch: else_branch.as_ref().map(|body| Box::new(lower_block(body, source_id, span_range.clone()))),
                }
            }
            else {
                HirExprKind::If {
                    condition: Box::new(lower_term_expression_with_context(condition, source_id, span_range.clone(), false)),
                    then_branch: Box::new(lower_block(then_branch, source_id, span_range.clone())),
                    else_branch: else_branch.as_ref().map(|body| Box::new(lower_block(body, source_id, span_range.clone()))),
                }
            }
        }
        TermExpression::Match { scrutinee, arms, .. } => HirExprKind::Match {
            scrutinee: Box::new(lower_term_expression_with_context(scrutinee, source_id, span_range.clone(), false)),
            arms: lower_match_arms(arms, source_id, span_range.clone(), span.clone()),
        },
        TermExpression::Loop { label, pattern, condition, body, .. } => HirExprKind::Loop {
            label: label.as_ref().map(|value| Identifier::new(value)),
            pattern: pattern.as_ref().map(|pat| lower_pattern(pat, source_id, span.clone())),
            iterator: None,
            condition: condition.as_ref().map(|expr| Box::new(lower_term_expression_with_context(expr, source_id, span_range.clone(), false))),
            body: Box::new(lower_block(body, source_id, span_range.clone())),
        },
        TermExpression::Return(node) => HirExprKind::Return(
            node.base.as_ref().map(|expr| Box::new(lower_term_expression_with_context(expr, source_id, span_range.clone(), false))),
        ),
        TermExpression::Break(node) => HirExprKind::Break {
            label: node.label.as_ref().map(|value| Identifier::new(value)),
            expr: node.base.as_ref().map(|expr| Box::new(lower_term_expression_with_context(expr, source_id, span_range.clone(), false))),
        },
        TermExpression::Continue(node) => HirExprKind::Continue { label: node.label.as_ref().map(|value| Identifier::new(value)) },
        TermExpression::Raise(node) => {
            HirExprKind::Raise(Box::new(lower_optional_term_expression(node.base.as_ref(), source_id, span_range.clone(), span.clone())))
        }
        TermExpression::Resume(node) => {
            HirExprKind::Resume(Box::new(lower_optional_term_expression(node.base.as_ref(), source_id, span_range.clone(), span.clone())))
        }
        TermExpression::Yield { expr, .. } => HirExprKind::Yield(
            expr.as_ref().map(|value| Box::new(lower_term_expression_with_context(value, source_id, span_range.clone(), false))),
        ),
        TermExpression::Catch { expr, arms, .. } => HirExprKind::Catch {
            expr: Box::new(lower_term_expression_with_context(expr, source_id, span_range.clone(), false)),
            arms: lower_match_arms(arms, source_id, span_range.clone(), span.clone()),
        },
        TermExpression::With { base, updates, .. } => {
            let object = lower_term_expression_with_context(base, source_id, span_range.clone(), false);
            let args = updates
                .iter()
                .map(|(field, value)| HirExpr {
                    kind: HirExprKind::FieldInit {
                        name: Identifier::new(&field.name),
                        value: Box::new(lower_term_expression_with_context(value, source_id, span_range.clone(), false)),
                    },
                    span: span.clone(),
                })
                .collect::<Vec<HirExpr>>();
            HirExprKind::Call {
                call_kind: HirCallKind::Function,
                callee: Box::new(HirExpr { kind: HirExprKind::Path(NamePath::new(vec![Identifier::new("with")])), span: span.clone() }),
                args: vec![HirCallArgument::positional(object)]
                    .into_iter()
                    .chain(args.into_iter().map(|expr| HirCallArgument::positional(expr)))
                    .collect(),
                resolved: None,
            }
        }
        TermExpression::SuperCall { method, args, .. } => {
            let callee = HirExpr {
                kind: HirExprKind::Path(NamePath::new(vec![Identifier::new("super"), Identifier::new(&method.name)])),
                span: span.clone(),
            };
            lower_canonical_call_arguments(
                callee,
                args.iter()
                    .map(|arg| HirCallArgument::positional(lower_term_expression_with_context(arg, source_id, span_range.clone(), false)))
                    .collect(),
            )
        }
    };
    HirExpr { kind, span }
}

fn lower_unary_expression(node: &TermUnaryNode, source_id: SourceID, fallback_span: Range<usize>, span: SourceSpan) -> HirExprKind {
    let folded_neg_literal = matches!(node.operator, ValkyrieTokenType::Minus)
        .then(|| {
            if let TermExpression::IntegerLiteral { value, .. } = &node.base {
                parse_integer_literal(value).ok().and_then(|value| {
                    let folded = -(value as i128);
                    (folded >= i64::MIN as i128 && folded <= i64::MAX as i128).then(|| folded as i64)
                })
            }
            else {
                None
            }
        })
        .flatten();
    if let Some(value) = folded_neg_literal {
        HirExprKind::Literal(HirLiteral::Integer64(value))
    }
    else if matches!(node.operator, ValkyrieTokenType::Star) {
        lower_method_call_kind(
            "deref_read",
            vec![HirCallArgument::positional(lower_term_expression_with_context(&node.base, source_id, fallback_span, false))],
            span,
        )
    }
    else {
        lower_operator_call_kind(
            unary_operator_id(&node.operator),
            vec![HirCallArgument::positional(lower_term_expression_with_context(&node.base, source_id, fallback_span, false))],
            span,
        )
    }
}

fn lower_binary_expression(node: &TermBinaryNode, source_id: SourceID, fallback_span: Range<usize>, span: SourceSpan) -> HirExprKind {
    let lhs = &node.lhs;
    let rhs = &node.rhs;
    match node.operator {
        ValkyrieTokenType::AndAnd => lower_short_circuit_and(lhs, rhs, source_id, fallback_span, span),
        ValkyrieTokenType::OrOr => lower_short_circuit_or(lhs, rhs, source_id, fallback_span, span),
        ValkyrieTokenType::Pipe => lower_pipe_expression(lhs, rhs, source_id, fallback_span, span),
        _ => lower_operator_call_kind(
            binary_operator_id(&node.operator),
            vec![
                HirCallArgument::positional(lower_term_expression_with_context(lhs, source_id, fallback_span.clone(), false)),
                HirCallArgument::positional(lower_term_expression_with_context(rhs, source_id, fallback_span, false)),
            ],
            span,
        ),
    }
}

fn lower_object_expression(
    callee: &TermExpression,
    fields: &[(frontend::ast::Identifier, Option<TermExpression>)],
    source_id: SourceID,
    fallback_span: Range<usize>,
    span: SourceSpan,
) -> HirExprKind {
    let path = extract_name_path(callee).unwrap_or_else(|| NamePath::new(vec![Identifier::new("_")]));
    let name = path.parts().last().cloned().unwrap_or_else(|| Identifier::new("_"));
    let args = fields
        .iter()
        .map(|(field, value)| {
            let value = value
                .as_ref()
                .map(|expr| lower_term_expression_with_context(expr, source_id, fallback_span.clone(), false))
                .unwrap_or_else(|| HirExpr {
                    kind: HirExprKind::Variable(HirIdentifier { name: Identifier::new(&field.name), shadow_index: 0, span: span.clone() }),
                    span: span.clone(),
                });
            HirExpr { kind: HirExprKind::FieldInit { name: Identifier::new(&field.name), value: Box::new(value) }, span: span.clone() }
        })
        .collect();
    HirExprKind::Construct { path, name, args, resolved: None }
}

fn lower_anonymous_class(class: &AnonymousClass, source_id: SourceID, fallback_span: Range<usize>) -> HirExprKind {
    HirExprKind::AnonymousClass {
        is_value_type: false,
        parents: class.parents.iter().map(|parent| HirParent::full(NamePath::new(vec![Identifier::new(parent)]), None, Vec::new())).collect(),
        fields: class
            .fields
            .iter()
            .filter_map(|field| {
                field.default.as_ref().map(|value| {
                    (
                        Identifier::new(&field.name.name),
                        Box::new(lower_term_expression_with_context(value, source_id, fallback_span.clone(), false)),
                    )
                })
            })
            .collect(),
        methods: class.methods.iter().map(|method| lower_inline_object_method(method, source_id)).collect(),
        captures: class
            .captures
            .iter()
            .map(|capture| HirCapture {
                identifier: HirIdentifier {
                    name: Identifier::new(&capture.name),
                    shadow_index: 0,
                    span: with_source(&frontend::std_range(&class.span), source_id),
                },
                ty: ValkyrieType::Unit,
                mode: CaptureMode::ByReference,
                is_mutable: false,
                storage_hint: CaptureStorage::default(),
            })
            .collect(),
        class_name: None,
    }
}

fn lower_inline_object_method(method: &frontend::ast::MethodDeclaration, source_id: SourceID) -> HirFunction {
    let span_range = frontend::std_range(&method.span);
    HirFunction {
        declaration: None,
        instance: None,
        name: Identifier::new(&method.name.name),
        declaring_namespace: NamePath::default(),
        doc: lower_documentation(&method.annotations),
        annotations: method.annotations.iter().map(|attribute| lower_attribute(attribute, source_id, span_range.clone())).collect(),
        generics: lower_generic_parameters(&method.generics),
        where_constraints: Vec::new(),
        params: lower_method_params(method, source_id),
        return_type: method.return_type.as_ref().map(lower_type_expression).unwrap_or(ValkyrieType::Unit),
        body: method.body.as_ref().map(|body| lower_block(body, source_id, span_range.clone())).unwrap_or_else(|| HirBlock {
            statements: Vec::new(),
            expr: None,
            span: with_source(&span_range, source_id),
        }),
        span: with_source(&span_range, source_id),
        visibility: lower_visibility(&method.annotations),
        is_abstract: method.body.is_none() || has_modifier(&method.annotations, "abstract"),
        is_final: has_modifier(&method.annotations, "final"),
        is_virtual: has_modifier(&method.annotations, "virtual"),
        is_override: has_modifier(&method.annotations, "override"),
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

fn lower_match_arms(arms: &[MatchArm], source_id: SourceID, fallback_span: Range<usize>, span: SourceSpan) -> Vec<HirMatchArm> {
    arms.iter()
        .map(|arm| {
            let pattern = lower_pattern(&arm.pattern, source_id, span.clone());
            let guard = arm
                .guard
                .as_ref()
                .map(|guard_expr| Box::new(lower_term_expression_with_context(guard_expr, source_id, fallback_span.clone(), false)));
            let body = Box::new(lower_term_expression_with_context(&arm.body, source_id, fallback_span.clone(), false));
            HirMatchArm { pattern, guard, body }
        })
        .collect()
}

fn lower_call_expression(
    callee: &TermExpression,
    args: &[TermExpression],
    source_id: SourceID,
    fallback_span: Range<usize>,
    span: SourceSpan,
) -> HirExprKind {
    let lowered_args = args
        .iter()
        .map(|arg| HirCallArgument::positional(lower_term_expression_with_context(arg, source_id, fallback_span.clone(), false)))
        .collect();
    if let TermExpression::DotCall { receiver, field, .. } = callee {
        if receiver_names_static_type(receiver) {
            let mut parts = receiver_name_path_parts(receiver);
            parts.push(Identifier::new(&field.name));
            let callee_expr = HirExpr {
                kind: HirExprKind::Path(NamePath::new(parts)),
                span: span.clone(),
            };
            return lower_canonical_call_arguments(callee_expr, lowered_args);
        }
        let object = lower_term_expression_with_context(receiver, source_id, fallback_span.clone(), false);
        let callee_expr = HirExpr {
            kind: HirExprKind::FieldAccess {
                object: Box::new(object),
                field: Identifier::new(&field.name),
            },
            span: span.clone(),
        };
        return lower_canonical_call_arguments(callee_expr, lowered_args);
    }
    lower_canonical_call_arguments(lower_term_expression_with_context(callee, source_id, fallback_span, true), lowered_args)
}

fn receiver_names_static_type(receiver: &TermExpression) -> bool {
    match receiver {
        TermExpression::NamePath(path) => path
            .parts
            .first()
            .is_some_and(|part| part.name.chars().next().is_some_and(|ch| ch.is_ascii_uppercase())),
        _ => false,
    }
}

fn receiver_name_path_parts(receiver: &TermExpression) -> Vec<Identifier> {
    match receiver {
        TermExpression::NamePath(path) => path.parts.iter().map(|part| Identifier::new(&part.name)).collect(),
        _ => Vec::new(),
    }
}

fn lower_method_call_kind(member: &str, args: Vec<HirCallArgument>, span: SourceSpan) -> HirExprKind {
    lower_canonical_call_arguments(HirExpr { kind: HirExprKind::Path(NamePath::new(vec![Identifier::new(member)])), span: span.clone() }, args)
}

fn lower_operator_call_kind(operator: OperatorId, args: Vec<HirCallArgument>, span: SourceSpan) -> HirExprKind {
    HirExprKind::Call {
        call_kind: HirCallKind::Operator(operator),
        callee: Box::new(HirExpr { kind: HirExprKind::Path(NamePath::new(vec![Identifier::new("operator")])), span }),
        args,
        resolved: None,
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
    if let TermExpression::ApplyCall { callee, args, .. } = rhs {
        let callee = lower_term_expression_with_context(callee, source_id, fallback_span.clone(), false);
        let mut all_args = vec![HirCallArgument::positional(arg)];
        all_args.extend(
            args.iter()
                .map(|value| HirCallArgument::positional(lower_term_expression_with_context(value, source_id, fallback_span.clone(), false))),
        );
        return lower_canonical_call_arguments(callee, all_args);
    }
    let callee = lower_term_expression_with_context(rhs, source_id, fallback_span, false);
    lower_canonical_call_arguments(callee, vec![HirCallArgument::positional(arg)])
}

fn lower_canonical_call_arguments(callee: HirExpr, args: Vec<HirCallArgument>) -> HirExprKind {
    HirExprKind::Call { call_kind: HirCallKind::Function, callee: Box::new(callee), args, resolved: None }
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

fn lower_name_expression(path: &frontend::ast::NamePath, span: SourceSpan) -> HirExprKind {
    let path = lower_name_path(path);
    if path.parts().len() == 1 {
        HirExprKind::Variable(HirIdentifier { name: path.parts()[0].clone(), shadow_index: 0, span })
    }
    else {
        HirExprKind::Path(path)
    }
}

fn lower_string_literal_kind(literal: &StringLiteral, source_id: SourceID, fallback_span: Range<usize>) -> HirExprKind {
    HirExprKind::Literal(HirLiteral::String(lower_string_literal(literal, source_id, fallback_span)))
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

fn lower_string_literal(literal: &StringLiteral, source_id: SourceID, fallback_span: Range<usize>) -> crate::types::hir::HirStringLiteral {
    crate::types::hir::HirStringLiteral {
        prefix: literal.prefix.as_ref().map(|prefix| Identifier::new(&prefix.name)),
        quote_count: literal.quote_count,
        segments: literal
            .segments
            .iter()
            .map(|segment| match segment {
                StringSegment::Text(text) => crate::types::hir::HirStringSegment::Text(text.content.clone()),
                StringSegment::Interpolation(interpolation) => crate::types::hir::HirStringSegment::Interpolation {
                    expr: lower_term_expression_with_context(&interpolation.expr, source_id, fallback_span.clone(), false),
                    is_fluent: interpolation.is_locale,
                },
            })
            .collect(),
    }
}

fn registered_operator(fixity: OperatorFixity, lexeme: &str) -> OperatorId {
    builtin_operator::lookup(fixity, lexeme).expect("parser operator must exist in the builtin registry")
}

fn binary_operator_id(op: &ValkyrieTokenType) -> OperatorId {
    match op {
        ValkyrieTokenType::Plus => registered_operator(OperatorFixity::Infix, "+"),
        ValkyrieTokenType::Minus => registered_operator(OperatorFixity::Infix, "-"),
        ValkyrieTokenType::Star => registered_operator(OperatorFixity::Infix, "*"),
        ValkyrieTokenType::Slash => registered_operator(OperatorFixity::Infix, "/"),
        ValkyrieTokenType::Percent => registered_operator(OperatorFixity::Infix, "%"),
        ValkyrieTokenType::EqEq => registered_operator(OperatorFixity::Infix, "=="),
        ValkyrieTokenType::NotEq => registered_operator(OperatorFixity::Infix, "!="),
        ValkyrieTokenType::LessThan => registered_operator(OperatorFixity::Infix, "<"),
        ValkyrieTokenType::LessEq => registered_operator(OperatorFixity::Infix, "<="),
        ValkyrieTokenType::GreaterThan => registered_operator(OperatorFixity::Infix, ">"),
        ValkyrieTokenType::GreaterEq => registered_operator(OperatorFixity::Infix, ">="),
        ValkyrieTokenType::LeftShift => registered_operator(OperatorFixity::Infix, "<<"),
        ValkyrieTokenType::RightShift => registered_operator(OperatorFixity::Infix, ">>"),
        ValkyrieTokenType::Ampersand => registered_operator(OperatorFixity::Infix, "&"),
        ValkyrieTokenType::Pipe => registered_operator(OperatorFixity::Infix, "|"),
        ValkyrieTokenType::Caret => registered_operator(OperatorFixity::Infix, "^"),
        ValkyrieTokenType::DotDot => registered_operator(OperatorFixity::Infix, ".."),
        _ => registered_operator(OperatorFixity::Infix, "+"),
    }
}

fn unary_operator_id(op: &ValkyrieTokenType) -> OperatorId {
    match op {
        ValkyrieTokenType::Minus => registered_operator(OperatorFixity::Prefix, "-"),
        ValkyrieTokenType::Bang => registered_operator(OperatorFixity::Prefix, "!"),
        _ => registered_operator(OperatorFixity::Prefix, "-"),
    }
}

fn subscript_operator_id(is_assignment: bool) -> OperatorId {
    if is_assignment { registered_operator(OperatorFixity::Postfix, "[]=") } else { registered_operator(OperatorFixity::Postfix, "[]") }
}

pub(super) fn extract_name_path(expression: &TermExpression) -> Option<NamePath> {
    match expression {
        TermExpression::NamePath(path) => Some(lower_name_path(path)),
        TermExpression::StringLiteral(literal) => {
            let raw = plain_string_literal_text(literal)?;
            Some(NamePath::new(raw.split("::").filter(|part| !part.is_empty()).map(Identifier::new).collect()))
        }
        _ => None,
    }
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

fn plain_string_literal_text(literal: &StringLiteral) -> Option<&str> {
    if literal.segments.len() != 1 {
        return None;
    }
    match &literal.segments[0] {
        StringSegment::Text(text) => Some(text.content.as_str()),
        StringSegment::Interpolation(_) => None,
    }
}
