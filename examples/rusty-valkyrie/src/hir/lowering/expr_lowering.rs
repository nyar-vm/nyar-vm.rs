use super::*;
use crate::{
    types::hir::{CaptureMode, CaptureStorage, HirCapture},
    {
        frontend::ast::{
            AnonymousClass, AnonymousMicro, AnonymousStructure, Block, Break, ClassPattern, Continue, LiteralPattern, MatchArm, Pattern, Raise,
            Resume, Return, Statement, StringLiteral, StringSegment, TermBinaryNode, TermExpression, TermUnaryNode, TextSegment,
            VariablePattern, WildcardPattern, pattern_nodes::ElsePattern,
        },
        operator_members::{
            CARDINAL_SUBSCRIPT_ASSIGN_MEMBER, CARDINAL_SUBSCRIPT_MEMBER, DEREF_READ_MEMBER, SUBSCRIPT_ASSIGN_MEMBER, SUBSCRIPT_MEMBER,
            infix_member_name, prefix_member_name,
        },
    },
};

use oak_valkyrie::lexer::token_type::ValkyrieTokenType;

pub(super) fn lower_block(body: &Block, source_id: SourceID, fallback_span: Range<usize>) -> Result<HirBlock, HirError> {
    let span_range = if body.span.is_empty() { fallback_span } else { frontend::std_range(&body.span) };
    let (statements, tail) = split_block_tail(&body.statements, source_id, span_range.clone())?;
    let expr = tail.map(|expr| lower_term_expression(&expr, source_id, span_range.clone())).transpose()?.map(Box::new);
    Ok(HirBlock { statements, expr, span: with_source(&span_range, source_id) })
}

fn split_block_tail(
    statements: &[Statement],
    source_id: SourceID,
    fallback_span: Range<usize>,
) -> Result<(Vec<HirStatement>, Option<TermExpression>), HirError> {
    if statements.is_empty() {
        return Ok((Vec::new(), None));
    }
    let mut lowered: Vec<HirStatement> = Vec::with_capacity(statements.len());
    for statement in statements {
        lowered.push(lower_statement(statement, source_id, fallback_span.clone())?);
    }
    if let Some(Statement::Expression(expr_stmt)) = statements.last() {
        if !expr_stmt.semi {
            lowered.pop();
            return Ok((lowered, Some(expr_stmt.expr.clone())));
        }
    }
    Ok((lowered, None))
}

fn lower_statement(statement: &Statement, source_id: SourceID, fallback_span: Range<usize>) -> Result<HirStatement, HirError> {
    match statement {
        Statement::Let(binding) => {
            let span_range = if binding.span.is_empty() { fallback_span } else { frontend::std_range(&binding.span) };
            let span = with_source(&span_range, source_id);
            Ok(HirStatement {
                kind: HirStatementKind::Let {
                    is_mutable: has_modifier(&binding.annotations, "mut"),
                    pattern: lower_pattern(&binding.pattern, source_id, span.clone()),
                    initializer: binding
                        .expr
                        .as_ref()
                        .map(|expr| lower_term_expression(expr, source_id, span_range.clone()))
                        .transpose()?
                        .map(Box::new),
                    ty: binding.ty.as_ref().map(lower_type_expression).transpose()?,
                },
                span,
            })
        }
        Statement::Expression(expr_stmt) => {
            let span_range = if expr_stmt.span.is_empty() { fallback_span } else { frontend::std_range(&expr_stmt.span) };
            let span = with_source(&span_range, source_id);
            let expr = lower_statement_expression(&expr_stmt.expr, source_id, span_range, span.clone())?;
            Ok(HirStatement { kind: HirStatementKind::Expr(Box::new(expr)), span })
        }
        Statement::Template(_) => {
            panic!("TGrammar 模板必须在 HIR lowering 前按 arch 展开")
        }
    }
}

fn lower_statement_expression(
    expression: &TermExpression,
    source_id: SourceID,
    fallback_span: Range<usize>,
    span: SourceSpan,
) -> Result<HirExpr, HirError> {
    match expression {
        TermExpression::Match { scrutinee, arms, span: match_span, .. } => Ok(HirExpr {
            kind: HirExprKind::Case {
                scrutinee: Box::new(lower_term_expression_with_context(scrutinee, source_id, fallback_span.clone(), false)?),
                arms: lower_match_arms(arms, source_id, fallback_span, span.clone(), frontend::std_range(match_span))?,
            },
            span,
        }),
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
        .collect::<Vec<_>>();
    let field_ids = vec![None; fields.len()];
    HirPattern::Object { name: Some(name), type_declaration: None, fields, field_ids, rest: None }
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

#[doc(hidden)]
pub fn lower_term_expression(expression: &TermExpression, source_id: SourceID, fallback_span: Range<usize>) -> Result<HirExpr, HirError> {
    lower_term_expression_with_context(expression, source_id, fallback_span, false)
}

fn lower_term_expression_with_context(
    expression: &TermExpression,
    source_id: SourceID,
    fallback_span: Range<usize>,
    preserve_member_access: bool,
) -> Result<HirExpr, HirError> {
    let span_range = {
        let span = expression.span();
        if span.is_empty() { fallback_span } else { frontend::std_range(&span) }
    };
    let span = with_source(&span_range, source_id);
    let kind = match expression {
        TermExpression::ArrayLiteral { items, .. } => HirExprKind::ArrayLiteral {
            items: items
                .iter()
                .map(|item| lower_term_expression_with_context(item, source_id, span_range.clone(), false))
                .collect::<Result<Vec<_>, _>>()?,
        },
        TermExpression::TupleLiteral { items, .. } => HirExprKind::TupleLiteral {
            items: items
                .iter()
                .map(|item| lower_term_expression_with_context(item, source_id, span_range.clone(), false))
                .collect::<Result<Vec<_>, _>>()?,
        },
        TermExpression::NamePath(path) => lower_name_expression(path, span.clone()),
        TermExpression::StringLiteral(literal) => lower_string_literal_kind(literal, source_id, span_range.clone())?,
        TermExpression::IntegerLiteral { value, .. } => {
            let parsed = parse_integer_literal(value).unwrap_or_else(|error| panic!("illegal integer literal `{value}`: {error}"));
            HirExprKind::Literal(HirLiteral::Integer64(parsed))
        }
        TermExpression::FloatLiteral { value, .. } => {
            let parsed = value.parse::<f64>().unwrap_or_else(|error| panic!("illegal float literal `{value}`: {error}"));
            HirExprKind::Literal(HirLiteral::Float64(OrderedFloat(parsed)))
        }
        TermExpression::BooleanLiteral { value, .. } => HirExprKind::Literal(HirLiteral::Bool(*value)),
        TermExpression::Unary(node) => lower_unary_expression(node, source_id, span_range.clone(), span.clone())?,
        TermExpression::Binary(node) => lower_binary_expression(node, source_id, span_range.clone(), span.clone())?,
        TermExpression::Cast { expr, ty, .. } => HirExprKind::Cast {
            expr: Box::new(lower_term_expression_with_context(expr, source_id, span_range.clone(), false)?),
            ty: super::lower_type_expression(ty)?,
        },
        TermExpression::Turbofish { expr, arguments, .. } => HirExprKind::GenericApply {
            callee: Box::new(lower_term_expression_with_context(expr, source_id, span_range.clone(), false)?),
            arguments: arguments.iter().map(super::lower_type_expression).collect::<Result<Vec<_>, _>>()?,
        },
        TermExpression::ApplyCall { callee, args, .. } => lower_call_expression(callee, args, source_id, span_range.clone(), span.clone())?,
        TermExpression::DotCall { receiver, field, .. } => {
            let object = lower_term_expression_with_context(receiver, source_id, span_range.clone(), false)?;
            let member = field.name.as_str();
            if let Some(kind) = lower_postfix_effect_member(member, object.clone()) {
                kind
            }
            else {
                HirExprKind::FieldAccess { object: Box::new(object), field: Identifier::new(member), field_id: None }
            }
        }
        TermExpression::Index { receiver, index, .. } => {
            let receiver = lower_term_expression_with_context(receiver, source_id, span_range.clone(), false)?;
            let index = lower_term_expression_with_context(index, source_id, span_range.clone(), false)?;
            lower_operator_member_call(
                SUBSCRIPT_MEMBER,
                vec![HirCallArgument::positional(receiver), HirCallArgument::positional(index)],
                span.clone(),
            )
        }
        TermExpression::Offset { receiver, offset, .. } => {
            let receiver = lower_term_expression_with_context(receiver, source_id, span_range.clone(), false)?;
            let offset = lower_term_expression_with_context(offset, source_id, span_range.clone(), false)?;
            lower_operator_member_call(
                CARDINAL_SUBSCRIPT_MEMBER,
                vec![HirCallArgument::positional(receiver), HirCallArgument::positional(offset)],
                span.clone(),
            )
        }
        TermExpression::Group { expr, .. } => {
            lower_term_expression_with_context(expr, source_id, span_range.clone(), preserve_member_access)?.kind
        }
        TermExpression::Block(block) => HirExprKind::Block(Box::new(lower_block(block, source_id, span_range.clone())?)),
        TermExpression::Micro(lambda) => {
            let hir_params =
                lambda.params.iter().map(|param| lower_param(param, source_id, span_range.clone())).collect::<Result<Vec<_>, _>>()?;
            let hir_return_type = lambda.return_type.as_ref().map(lower_type_expression).transpose()?.unwrap_or(ValkyrieType::AutoType);
            let hir_body = lower_block(&lambda.body, source_id, span_range.clone())?;
            HirExprKind::Lambda { generics: Vec::new(), params: hir_params, return_type: hir_return_type, body: Box::new(hir_body) }
        }
        TermExpression::Object { callee, fields, .. } => lower_object_expression(callee, fields, source_id, span_range.clone(), span.clone())?,
        TermExpression::AnonymousClass(literal) => lower_anonymous_class(literal, source_id, span_range.clone())?,
        TermExpression::AnonymousStructure(literal) => lower_anonymous_structure(literal, source_id, span_range.clone())?,
        TermExpression::If { pattern, condition, then_branch, else_branch, .. } => {
            if let Some(pattern) = pattern {
                HirExprKind::IfLet {
                    pattern: lower_pattern(pattern, source_id, span.clone()),
                    scrutinee: Box::new(lower_term_expression_with_context(condition, source_id, span_range.clone(), false)?),
                    then_branch: Box::new(lower_block(then_branch, source_id, span_range.clone())?),
                    else_branch: else_branch.as_ref().map(|body| lower_block(body, source_id, span_range.clone())).transpose()?.map(Box::new),
                }
            }
            else {
                HirExprKind::If {
                    condition: Box::new(lower_term_expression_with_context(condition, source_id, span_range.clone(), false)?),
                    then_branch: Box::new(lower_block(then_branch, source_id, span_range.clone())?),
                    else_branch: else_branch.as_ref().map(|body| lower_block(body, source_id, span_range.clone())).transpose()?.map(Box::new),
                }
            }
        }
        TermExpression::Match { scrutinee, arms, span: match_span, .. } => HirExprKind::Match {
            scrutinee: Box::new(lower_term_expression_with_context(scrutinee, source_id, span_range.clone(), false)?),
            arms: lower_match_arms(arms, source_id, span_range.clone(), span.clone(), frontend::std_range(match_span))?,
        },
        TermExpression::Loop { label, pattern, condition, body, .. } => HirExprKind::Loop {
            label: label.as_ref().map(|value| Identifier::new(value)),
            pattern: pattern.as_ref().map(|pat| lower_pattern(pat, source_id, span.clone())),
            iterator: if pattern.is_some() {
                condition
                    .as_ref()
                    .map(|expr| lower_term_expression_with_context(expr, source_id, span_range.clone(), false))
                    .transpose()?
                    .map(|e| Box::new(e))
            }
            else {
                None
            },
            condition: if pattern.is_none() {
                condition
                    .as_ref()
                    .map(|expr| lower_term_expression_with_context(expr, source_id, span_range.clone(), false))
                    .transpose()?
                    .map(|e| Box::new(e))
            }
            else {
                None
            },
            body: Box::new(lower_block(body, source_id, span_range.clone())?),
        },
        TermExpression::Return(node) => HirExprKind::Return(
            node.base
                .as_ref()
                .map(|expr| lower_term_expression_with_context(expr, source_id, span_range.clone(), false))
                .transpose()?
                .map(|e| Box::new(e)),
        ),
        TermExpression::Break(node) => HirExprKind::Break {
            label: node.label.as_ref().map(|value| Identifier::new(value)),
            expr: node
                .base
                .as_ref()
                .map(|expr| lower_term_expression_with_context(expr, source_id, span_range.clone(), false))
                .transpose()?
                .map(|e| Box::new(e)),
        },
        TermExpression::Continue(node) => HirExprKind::Continue { label: node.label.as_ref().map(|value| Identifier::new(value)) },
        TermExpression::Raise(node) => {
            HirExprKind::Raise(Box::new(lower_optional_term_expression(node.base.as_ref(), source_id, span_range.clone(), span.clone())?))
        }
        TermExpression::Resume(node) => {
            HirExprKind::Resume(Box::new(lower_optional_term_expression(node.base.as_ref(), source_id, span_range.clone(), span.clone())?))
        }
        TermExpression::Yield { expr, .. } => HirExprKind::Yield(
            expr.as_ref()
                .map(|value| lower_term_expression_with_context(value, source_id, span_range.clone(), false))
                .transpose()?
                .map(|e| Box::new(e)),
        ),
        TermExpression::Catch { expr, arms, span: catch_span, .. } => HirExprKind::Catch {
            expr: Box::new(lower_term_expression_with_context(expr, source_id, span_range.clone(), false)?),
            arms: lower_match_arms(arms, source_id, span_range.clone(), span.clone(), frontend::std_range(catch_span))?,
        },
        TermExpression::With { base, updates, .. } => {
            let object = lower_term_expression_with_context(base, source_id, span_range.clone(), false)?;
            let mut hir_updates: Vec<HirExpr> = Vec::with_capacity(updates.len());
            for (field, value) in updates {
                let value = lower_term_expression_with_context(value, source_id, span_range.clone(), false)?;
                hir_updates.push(HirExpr {
                    kind: HirExprKind::FieldInit { name: Identifier::new(&field.name), value: Box::new(value), field_id: None },
                    span: span.clone(),
                });
            }
            HirExprKind::Call {
                callee: Box::new(HirExpr { kind: HirExprKind::Path(NamePath::new(vec![Identifier::new("with")])), span: span.clone() }),
                args: vec![HirCallArgument::positional(object)]
                    .into_iter()
                    .chain(hir_updates.into_iter().map(|expr| HirCallArgument::positional(expr)))
                    .collect(),
                resolved: None,
            }
        }
        TermExpression::SuperCall { method, args, .. } => {
            let callee = HirExpr {
                kind: HirExprKind::Path(NamePath::new(vec![Identifier::new("super"), Identifier::new(&method.name)])),
                span: span.clone(),
            };
            let arg_exprs: Vec<_> = args
                .iter()
                .map(|arg| lower_term_expression_with_context(arg, source_id, span_range.clone(), false))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .map(HirCallArgument::positional)
                .collect();
            lower_canonical_call_arguments(callee, arg_exprs)
        }
    };
    Ok(HirExpr { kind, span })
}

fn lower_unary_expression(
    node: &TermUnaryNode,
    source_id: SourceID,
    fallback_span: Range<usize>,
    span: SourceSpan,
) -> Result<HirExprKind, HirError> {
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
        Ok(HirExprKind::Literal(HirLiteral::Integer64(value)))
    }
    else if matches!(node.operator, ValkyrieTokenType::Star) {
        let base = lower_term_expression_with_context(&node.base, source_id, fallback_span, false)?;
        Ok(lower_method_call_kind(DEREF_READ_MEMBER, vec![HirCallArgument::positional(base)], span))
    }
    else {
        let base = lower_term_expression_with_context(&node.base, source_id, fallback_span, false)?;
        Ok(lower_operator_member_call(&prefix_member_name(&node.operator), vec![HirCallArgument::positional(base)], span))
    }
}

fn lower_binary_expression(
    node: &TermBinaryNode,
    source_id: SourceID,
    fallback_span: Range<usize>,
    span: SourceSpan,
) -> Result<HirExprKind, HirError> {
    let lhs = &node.lhs;
    let rhs = &node.rhs;
    match node.operator {
        ValkyrieTokenType::Eq => {
            if let TermExpression::Index { receiver, index, .. } | TermExpression::Offset { receiver, offset: index, .. } = lhs {
                let receiver = lower_term_expression_with_context(receiver, source_id, fallback_span.clone(), false)?;
                let index = lower_term_expression_with_context(index, source_id, fallback_span.clone(), false)?;
                let rhs = lower_term_expression_with_context(rhs, source_id, fallback_span, false)?;
                return Ok(lower_operator_member_call(
                    if matches!(lhs, TermExpression::Index { .. }) { SUBSCRIPT_ASSIGN_MEMBER } else { CARDINAL_SUBSCRIPT_ASSIGN_MEMBER },
                    vec![HirCallArgument::positional(receiver), HirCallArgument::positional(index), HirCallArgument::positional(rhs)],
                    span,
                ));
            }
            let target = lower_term_expression_with_context(lhs, source_id, fallback_span.clone(), true)?;
            let value = Box::new(lower_term_expression_with_context(rhs, source_id, fallback_span, false)?);
            match target.kind {
                HirExprKind::Variable(identifier) => Ok(HirExprKind::Assign { target: identifier.name, value }),
                HirExprKind::FieldAccess { object, field, field_id } => Ok(HirExprKind::StoreField { object, field, value, field_id }),
                _ => panic!("Oak 已验证的赋值位置不能降低为其他表达式"),
            }
        }
        ValkyrieTokenType::AndAnd => Ok(lower_short_circuit_and(lhs, rhs, source_id, fallback_span, span)?),
        ValkyrieTokenType::OrOr => Ok(lower_short_circuit_or(lhs, rhs, source_id, fallback_span, span)?),
        _ => {
            let lhs = lower_term_expression_with_context(lhs, source_id, fallback_span.clone(), false)?;
            let rhs = lower_term_expression_with_context(rhs, source_id, fallback_span, false)?;
            Ok(lower_operator_member_call(
                &infix_member_name(&node.operator),
                vec![HirCallArgument::positional(lhs), HirCallArgument::positional(rhs)],
                span,
            ))
        }
    }
}

fn lower_object_expression(
    callee: &TermExpression,
    fields: &[(frontend::ast::Identifier, Option<TermExpression>)],
    source_id: SourceID,
    fallback_span: Range<usize>,
    span: SourceSpan,
) -> Result<HirExprKind, HirError> {
    let path = extract_name_path(callee).unwrap_or_else(|| NamePath::new(vec![Identifier::new("_")]));
    let args: Vec<_> = fields
        .iter()
        .map(|(field, value)| -> Result<_, HirError> {
            let value = match value.as_ref() {
                Some(expr) => lower_term_expression_with_context(expr, source_id, fallback_span.clone(), false)?,
                None => HirExpr {
                    kind: HirExprKind::Variable(HirIdentifier { name: Identifier::new(&field.name), shadow_index: 0, span: span.clone() }),
                    span: span.clone(),
                },
            };
            Ok(HirExpr {
                kind: HirExprKind::FieldInit { name: Identifier::new(&field.name), value: Box::new(value), field_id: None },
                span: span.clone(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(HirExprKind::Construct { path, args, resolved: None })
}

fn lower_anonymous_class(literal: &AnonymousClass, source_id: SourceID, fallback_span: Range<usize>) -> Result<HirExprKind, HirError> {
    lower_anonymous_literal_fields(false, &literal.span, &literal.parents, &literal.body, &literal.captures, source_id, fallback_span)
}

fn lower_anonymous_structure(literal: &AnonymousStructure, source_id: SourceID, fallback_span: Range<usize>) -> Result<HirExprKind, HirError> {
    lower_anonymous_literal_fields(true, &literal.span, &literal.parents, &literal.body, &literal.captures, source_id, fallback_span)
}

fn lower_anonymous_literal_fields(
    is_value_type: bool,
    literal_span: &frontend::ast::Span,
    parents: &[frontend::ast::Parent],
    body: &frontend::ast::StructureBody,
    captures: &[frontend::ast::Identifier],
    source_id: SourceID,
    fallback_span: Range<usize>,
) -> Result<HirExprKind, HirError> {
    let default_fields: Vec<_> = body
        .fields
        .iter()
        .filter_map(|field| {
            field.default.as_ref().map(|value| -> Result<_, HirError> {
                Ok((
                    Identifier::new(&field.name.name),
                    Box::new(lower_term_expression_with_context(value, source_id, fallback_span.clone(), false)?),
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let methods = body.methods.iter().map(|method| lower_inline_object_method(method, source_id)).collect::<Result<Vec<_>, _>>()?;
    Ok(HirExprKind::AnonymousClass {
        is_value_type,
        parents: parents.iter().map(lower_parent).collect(),
        fields: default_fields,
        methods,
        captures: captures
            .iter()
            .map(|capture| HirCapture {
                identifier: HirIdentifier {
                    name: Identifier::new(&capture.name),
                    shadow_index: 0,
                    span: with_source(&frontend::std_range(literal_span), source_id),
                },
                ty: ValkyrieType::Unit,
                mode: CaptureMode::ByReference,
                is_mutable: false,
                storage_hint: CaptureStorage::default(),
            })
            .collect(),
        class_name: None,
    })
}

fn lower_inline_object_method(method: &frontend::ast::MethodDeclaration, source_id: SourceID) -> Result<HirFunction, HirError> {
    let span_range = frontend::std_range(&method.span);
    Ok(HirFunction {
        declaration: None,
        instance: None,
        name: Identifier::new(&method.name.name),
        declaring_namespace: NamePath::default(),
        doc: lower_documentation(&method.annotations),
        annotations: method
            .annotations
            .iter()
            .map(|attribute| lower_attribute(attribute, source_id, span_range.clone()))
            .collect::<Result<Vec<_>, _>>()?,
        generics: lower_generic_parameters(&method.generics),
        where_constraints: Vec::new(),
        params: lower_method_params(method, source_id)?,
        return_type: method.return_type.as_ref().map(lower_type_expression).transpose()?.unwrap_or(ValkyrieType::Unit),
        body: method.body.as_ref().map(|body| lower_block(body, source_id, span_range.clone())).transpose()?.unwrap_or_else(|| HirBlock {
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
    })
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
    arms: &[MatchArm],
    source_id: SourceID,
    fallback_span: Range<usize>,
    span: SourceSpan,
    match_span: Range<usize>,
) -> Result<Vec<HirMatchArm>, HirError> {
    if let Some(source) = super::source_context::hir_lower_source() {
        if super::match_pattern_recovery::match_needs_source_recovery(&source, match_span.clone(), arms) {
            if let Ok(recovered) = super::match_pattern_recovery::recover_match_arms(&source, match_span) {
                let result: Result<Vec<_>, _> = recovered
                    .into_iter()
                    .map(|(pattern, body)| -> Result<_, HirError> {
                        Ok(HirMatchArm {
                            pattern,
                            guard: None,
                            body: Box::new(lower_term_expression_with_context(&body, source_id, fallback_span.clone(), false)?),
                        })
                    })
                    .collect();
                return result;
            }
        }
    }
    arms.iter()
        .map(|arm| -> Result<_, HirError> {
            let pattern = lower_pattern(&arm.pattern, source_id, span.clone());
            let guard = arm
                .guard
                .as_ref()
                .map(|guard_expr| lower_term_expression_with_context(guard_expr, source_id, fallback_span.clone(), false))
                .transpose()?
                .map(Box::new);
            let body = Box::new(lower_term_expression_with_context(&arm.body, source_id, fallback_span.clone(), false)?);
            Ok(HirMatchArm { pattern, guard, body })
        })
        .collect()
}

fn lower_call_expression(
    callee: &TermExpression,
    args: &[TermExpression],
    source_id: SourceID,
    fallback_span: Range<usize>,
    span: SourceSpan,
) -> Result<HirExprKind, HirError> {
    let mut lowered_args: Vec<_> = args
        .iter()
        .map(|arg| -> Result<_, HirError> {
            Ok(HirCallArgument::positional(lower_term_expression_with_context(arg, source_id, fallback_span.clone(), false)?))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(path) = extract_name_path(callee) {
        if path.parts().len() == 1 && lowered_args.len() == 1 {
            if let Some(ty) = crate::hir::numeric_builtin_cast_type(path.parts()[0].as_str()) {
                let expr = lowered_args.pop().expect("one arg").value;
                return Ok(HirExprKind::Cast { expr: Box::new(expr), ty });
            }
        }
        let callee_expr = HirExpr { kind: HirExprKind::Path(path), span: span.clone() };
        return Ok(lower_canonical_call_arguments(callee_expr, lowered_args));
    }
    let callee_expr = lower_term_expression_with_context(callee, source_id, fallback_span, true)?;
    Ok(lower_canonical_call_arguments(callee_expr, lowered_args))
}

fn lower_method_call_kind(member: &str, args: Vec<HirCallArgument>, span: SourceSpan) -> HirExprKind {
    lower_canonical_call_arguments(HirExpr { kind: HirExprKind::Path(NamePath::new(vec![Identifier::new(member)])), span: span.clone() }, args)
}

/// AST 运算符语法糖在 HIR 早期展开为接收者的普通成员调用。
///
/// 成员名与运算符声明的 HIR 成员名同为 `{fixity} {lexeme}`（见 `operator_member_name`）：
/// `a + b` 展开为接收者 `a` 的 `infix +` 调用，`a[i]` 展开为 `a` 的 `suffix []` 调用。
/// HIR/MIR/LIR 不再持有 unary/binary/operator 语义类别，也不存在运算符身份表。
fn lower_operator_member_call(member: &str, args: Vec<HirCallArgument>, span: SourceSpan) -> HirExprKind {
    lower_canonical_call_arguments(HirExpr { kind: HirExprKind::Path(NamePath::new(vec![Identifier::new(member)])), span }, args)
}

fn lower_canonical_call_arguments(callee: HirExpr, args: Vec<HirCallArgument>) -> HirExprKind {
    HirExprKind::Call { callee: Box::new(callee), args, resolved: None }
}

fn lower_short_circuit_and(
    lhs: &TermExpression,
    rhs: &TermExpression,
    source_id: SourceID,
    fallback_span: Range<usize>,
    span: SourceSpan,
) -> Result<HirExprKind, HirError> {
    let condition = lower_term_expression_with_context(lhs, source_id, fallback_span.clone(), false)?;
    let rhs_expr = lower_term_expression_with_context(rhs, source_id, fallback_span, false)?;
    Ok(HirExprKind::If {
        condition: Box::new(condition),
        then_branch: Box::new(HirBlock { statements: Vec::new(), expr: Some(Box::new(rhs_expr)), span: span.clone() }),
        else_branch: Some(Box::new(HirBlock {
            statements: Vec::new(),
            expr: Some(Box::new(HirExpr { kind: HirExprKind::Literal(HirLiteral::Bool(false)), span: span.clone() })),
            span,
        })),
    })
}

fn lower_short_circuit_or(
    lhs: &TermExpression,
    rhs: &TermExpression,
    source_id: SourceID,
    fallback_span: Range<usize>,
    span: SourceSpan,
) -> Result<HirExprKind, HirError> {
    let condition = lower_term_expression_with_context(lhs, source_id, fallback_span.clone(), false)?;
    let rhs_expr = lower_term_expression_with_context(rhs, source_id, fallback_span, false)?;
    Ok(HirExprKind::If {
        condition: Box::new(condition),
        then_branch: Box::new(HirBlock {
            statements: Vec::new(),
            expr: Some(Box::new(HirExpr { kind: HirExprKind::Literal(HirLiteral::Bool(true)), span: span.clone() })),
            span: span.clone(),
        }),
        else_branch: Some(Box::new(HirBlock { statements: Vec::new(), expr: Some(Box::new(rhs_expr)), span })),
    })
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

fn lower_string_literal_kind(literal: &StringLiteral, source_id: SourceID, fallback_span: Range<usize>) -> Result<HirExprKind, HirError> {
    Ok(HirExprKind::Literal(HirLiteral::String(lower_string_literal(literal, source_id, fallback_span)?)))
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

fn lower_string_literal(
    literal: &StringLiteral,
    source_id: SourceID,
    fallback_span: Range<usize>,
) -> Result<crate::types::hir::HirStringLiteral, HirError> {
    let segments = literal
        .segments
        .iter()
        .map(|segment| -> Result<_, HirError> {
            match segment {
                StringSegment::Text(text) => Ok(crate::types::hir::HirStringSegment::Text(text.content.clone())),
                StringSegment::Interpolation(interpolation) => Ok(crate::types::hir::HirStringSegment::Interpolation {
                    expr: lower_term_expression_with_context(&interpolation.expr, source_id, fallback_span.clone(), false)?,
                    is_fluent: interpolation.is_locale,
                }),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(crate::types::hir::HirStringLiteral {
        prefix: literal.prefix.as_ref().map(|prefix| Identifier::new(&prefix.name)),
        quote_count: literal.quote_count,
        segments,
    })
}

pub(super) fn extract_name_path(expression: &TermExpression) -> Option<NamePath> {
    match expression {
        TermExpression::NamePath(path) => Some(lower_name_path(path)),
        TermExpression::DotCall { receiver, field, .. } => {
            let mut parts = extract_name_path(receiver)?.parts().to_vec();
            parts.push(Identifier::new(&field.name));
            Some(NamePath::new(parts))
        }
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
) -> Result<HirExpr, HirError> {
    match expression {
        Some(expr) => lower_term_expression_with_context(expr, source_id, fallback_span, false),
        None => Ok(HirExpr { kind: HirExprKind::Literal(HirLiteral::Unit), span }),
    }
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
