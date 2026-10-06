//! Extract AWSL component ABI from vx / widget AST.

use std::ops::Range;

use crate::transitional::cst::valkyrie::{
    ast::{
        Annotations, ClassDeclaration, ClassLikeKind, DeclarationBody, FunctionStatement, LetStatement, ObjectMethodDeclaration,
        PatternExpression, RootStatement, TermExpression, ValkyrieRoot,
    },
    parser::AstParser,
};

use super::{AbiDerived, AbiEffect, AbiEvent, AbiMemo, AbiParam, AbiProperty, AbiState, ComponentAbi};

/// Result of ABI extraction, including diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbiExtractResult {
    /// Extracted ABI (may be partial when errors are present).
    pub abi: ComponentAbi,
    /// Semantic issues encountered during extraction.
    pub issues: Vec<AbiIssue>,
}

/// One ABI diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbiIssue {
    /// Issue classification.
    pub kind: AbiIssueKind,
    /// Human-readable message.
    pub message: String,
    /// Source span when available.
    pub span: Option<Range<usize>>,
    /// Warning vs hard error.
    pub severity: AbiSeverity,
}

/// ABI issue codes aligned with the RFC diagnostic table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbiIssueKind {
    /// `[property]` on non-`let`.
    PropertyOnNonLet,
    /// `[event]` on non-`micro`.
    EventOnNonMicro,
    /// `[memoize]` on `let mut`.
    MemoizeOnMut,
    /// `[event] micro` has non-empty body.
    EventNonEmptyBody,
    /// `emit` target is not an event micro.
    EmitInvalidTarget,
    /// `emit` arity mismatch.
    EmitArityMismatch,
    /// Attribute combination forbidden.
    AttributeConflict,
    /// ABI name is not snake_case.
    NotSnakeCase,
    /// `[event] micro` declared but never emitted.
    EventNeverEmitted,
    /// `[property] let` never read.
    PropertyNeverRead,
    /// `[memoize]` on trivial expression.
    MemoizeTrivial,
    /// `effect` dependency unused in block.
    EffectUnusedDep,
    /// Legacy string `emit("name", ...)`.
    LegacyStringEmit,
}

/// Warning vs error severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbiSeverity {
    /// Hard error.
    Error,
    /// Non-fatal warning.
    Warning,
}

/// Extract ABI from raw `<script>` text by wrapping it in a synthetic widget.
pub fn extract_component_abi_from_script(script: &str, widget_name: &str) -> AbiExtractResult {
    let synthetic = format!("widget {widget_name} {{\n{script}\n}}\n");
    extract_component_abi_from_vx(&synthetic, widget_name)
}

/// Extract ABI from vx source containing a widget declaration.
pub fn extract_component_abi_from_vx(source: &str, widget_name: &str) -> AbiExtractResult {
    let mut issues = Vec::new();
    let Ok(root) = AstParser::parse_vx_root(source)
    else {
        return AbiExtractResult {
            abi: ComponentAbi { widget_name: widget_name.to_string(), ..Default::default() },
            issues: vec![AbiIssue {
                kind: AbiIssueKind::PropertyOnNonLet,
                message: "failed to parse script as vx".into(),
                span: None,
                severity: AbiSeverity::Error,
            }],
        };
    };

    let Some(widget) = find_widget(&root, widget_name)
    else {
        return AbiExtractResult {
            abi: ComponentAbi { widget_name: widget_name.to_string(), ..Default::default() },
            issues: vec![AbiIssue {
                kind: AbiIssueKind::PropertyOnNonLet,
                message: format!("widget `{widget_name}` not found in vx source"),
                span: None,
                severity: AbiSeverity::Error,
            }],
        };
    };

    let mut abi = ComponentAbi { widget_name: widget_name.to_string(), ..Default::default() };
    let mut emit_targets = Vec::new();

    for statement in &widget.body.script_statements {
        match statement {
            FunctionStatement::Let(let_stmt) => {
                extract_let_binding(source, let_stmt, &mut abi, &mut issues);
            }
            FunctionStatement::Term { expression, span } => {
                collect_call_site(source, expression, span.clone(), &mut emit_targets, &mut abi, &mut issues);
            }
            FunctionStatement::Function { function, .. } => {
                if has_attr(&function.annotations, "event") {
                    issues.push(error(
                        AbiIssueKind::EventOnNonMicro,
                        "`[event]` must annotate a `micro` declaration",
                        Some(function.span.clone()),
                    ));
                }
            }
            _ => {}
        }
    }

    for method in &widget.body.methods {
        if has_attr(&method.annotations, "event") {
            extract_event_micro(source, method, &mut abi, &mut issues);
        }
    }

    validate_emit_targets(&abi, &emit_targets, &mut issues);
    warn_unused_events(&abi, &emit_targets, &mut issues);

    AbiExtractResult { abi, issues }
}

fn find_widget<'a>(root: &'a ValkyrieRoot, widget_name: &str) -> Option<&'a ClassDeclaration> {
    root.statements.iter().find_map(|stmt| match stmt {
        RootStatement::Class(class) if class.kind == ClassLikeKind::Widget && class.name.as_str() == widget_name => Some(class),
        _ => None,
    })
}

fn extract_let_binding(source: &str, let_stmt: &LetStatement, abi: &mut ComponentAbi, issues: &mut Vec<AbiIssue>) {
    let has_property = has_attr(&let_stmt.annotations, "property");
    let has_memoize = has_attr(&let_stmt.annotations, "memoize");
    let has_event = has_attr(&let_stmt.annotations, "event");

    if has_event {
        issues.push(error(AbiIssueKind::EventOnNonMicro, "`[event]` must annotate a `micro` declaration", Some(let_stmt.span.clone())));
    }
    if has_property && has_memoize {
        issues.push(error(
            AbiIssueKind::AttributeConflict,
            "`[property]` and `[memoize]` cannot be used together",
            Some(let_stmt.span.clone()),
        ));
    }
    if has_memoize && let_stmt.is_mutable {
        issues.push(error(AbiIssueKind::MemoizeOnMut, "`[memoize]` cannot be used on `let mut`", Some(let_stmt.span.clone())));
    }
    if has_property && let_stmt.is_mutable {
        issues.push(error(AbiIssueKind::AttributeConflict, "`[property]` cannot be used with `let mut`", Some(let_stmt.span.clone())));
    }

    let Some(name) = pattern_binding_name(&let_stmt.pattern)
    else {
        return;
    };

    let init_expr = let_stmt.initializer.as_ref().map(|expr| slice_span(source, expr.span().clone()));
    let type_hint = let_stmt.ty.as_ref().map(|ty| slice_span(source, ty.span().clone()));

    if has_property {
        abi.properties.push(AbiProperty {
            name: name.clone(),
            type_hint,
            required: init_expr.is_none(),
            default_expr: init_expr,
            span: let_stmt.span.clone(),
        });
        return;
    }

    if has_memoize {
        if init_expr.is_none() {
            issues.push(error(AbiIssueKind::MemoizeOnMut, "`[memoize]` requires an initializer expression", Some(let_stmt.span.clone())));
            return;
        }
        let expr = init_expr.unwrap_or_default();
        if expr.len() < 12 {
            issues.push(warn(
                AbiIssueKind::MemoizeTrivial,
                format!("`[memoize]` on `{name}` may have little benefit for simple expressions"),
                Some(let_stmt.span.clone()),
            ));
        }
        abi.memoized.push(AbiMemo { name, expr, span: let_stmt.span.clone() });
        return;
    }

    if let_stmt.is_mutable {
        abi.states.push(AbiState { name, init_expr, span: let_stmt.span.clone() });
        return;
    }

    if let Some(expr) = init_expr {
        abi.derived.push(AbiDerived { name, expr, span: let_stmt.span.clone() });
    }
}

fn extract_event_micro(source: &str, method: &ObjectMethodDeclaration, abi: &mut ComponentAbi, issues: &mut Vec<AbiIssue>) {
    if !method.body.as_ref().is_some_and(is_empty_body) {
        issues.push(error(
            AbiIssueKind::EventNonEmptyBody,
            format!("`[event] micro {}` must have an empty body", method.name.as_str()),
            Some(method.span.clone()),
        ));
    }
    if abi.events.iter().any(|event| event.name == method.name.as_str()) {
        issues.push(error(AbiIssueKind::AttributeConflict, format!("duplicate event `{}`", method.name.as_str()), Some(method.span.clone())));
    }
    let params: Vec<AbiParam> = method
        .params
        .iter()
        .map(|param| AbiParam {
            name: param.name.as_str().to_string(),
            type_hint: param.parameter_type.as_ref().map(|ty| slice_span(source, ty.span().clone())),
        })
        .collect();
    abi.events.push(AbiEvent { name: method.name.as_str().to_string(), params, span: method.span.clone() });
}

fn collect_call_site(
    source: &str,
    expression: &TermExpression,
    span: Range<usize>,
    emit_targets: &mut Vec<(String, Range<usize>, usize)>,
    abi: &mut ComponentAbi,
    issues: &mut Vec<AbiIssue>,
) {
    let TermExpression::Call(call) = expression
    else {
        return;
    };
    let Some(callee) = callee_name(&call.callee)
    else {
        return;
    };
    if callee == "emit" {
        let args = &call.args.arguments;
        if args.is_empty() {
            issues.push(error(AbiIssueKind::EmitInvalidTarget, "`emit` requires an event target", Some(span)));
            return;
        }
        if let Some(event_name) = arg_identifier(&args[0].value) {
            emit_targets.push((event_name, span, args.len().saturating_sub(1)));
            return;
        }
        if arg_string_literal(&args[0].value).is_some() {
            issues.push(warn(
                AbiIssueKind::LegacyStringEmit,
                "string `emit(\"name\", ...)` is deprecated; declare `[event] micro` and use `emit(name, ...)`",
                Some(span),
            ));
            return;
        }
        issues.push(error(AbiIssueKind::EmitInvalidTarget, "`emit` first argument must be an event symbol", Some(span)));
        return;
    }
    if callee == "effect" {
        let deps = call.args.arguments.iter().filter_map(|arg| arg_identifier(&arg.value)).collect();
        abi.effects.push(AbiEffect { deps, span });
    }
}

fn validate_emit_targets(abi: &ComponentAbi, emit_targets: &[(String, Range<usize>, usize)], issues: &mut Vec<AbiIssue>) {
    for (event_name, span, arg_count) in emit_targets {
        let Some(event) = abi.event(event_name)
        else {
            issues.push(error(
                AbiIssueKind::EmitInvalidTarget,
                format!("`emit({event_name}, ...)` target is not a declared `[event] micro`"),
                Some(span.clone()),
            ));
            continue;
        };
        if *arg_count != event.params.len() {
            issues.push(error(
                AbiIssueKind::EmitArityMismatch,
                format!("`emit({event_name}, ...)` expects {} argument(s), got {arg_count}", event.params.len()),
                Some(span.clone()),
            ));
        }
    }
}

fn warn_unused_events(abi: &ComponentAbi, emit_targets: &[(String, Range<usize>, usize)], issues: &mut Vec<AbiIssue>) {
    for event in &abi.events {
        if !emit_targets.iter().any(|(name, _, _)| name == &event.name) {
            issues.push(warn(
                AbiIssueKind::EventNeverEmitted,
                format!("event `{}` is declared but never emitted", event.name),
                Some(event.span.clone()),
            ));
        }
    }
}

fn has_attr(annotations: &Annotations, name: &str) -> bool {
    annotations.attributes().any(|attr| attr.name.parts.last().is_some_and(|part| part == name))
}

fn is_empty_body(body: &DeclarationBody) -> bool {
    body.statements.is_empty() && body.tail_expression.is_none()
}

fn pattern_binding_name(pattern: &PatternExpression) -> Option<String> {
    match pattern {
        PatternExpression::Variable { name, .. } => Some(name.clone()),
        PatternExpression::Name { path, .. } => path.parts.last().cloned(),
        _ => None,
    }
}

fn callee_name(expression: &TermExpression) -> Option<&str> {
    match expression {
        TermExpression::Name { path, .. } => path.parts.last().map(String::as_str),
        _ => None,
    }
}

fn arg_identifier(expression: &TermExpression) -> Option<String> {
    match expression {
        TermExpression::Name { path, .. } => path.parts.last().cloned(),
        _ => None,
    }
}

fn arg_string_literal(expression: &TermExpression) -> Option<String> {
    match expression {
        TermExpression::Literal { literal: crate::transitional::cst::valkyrie::ast::LiteralExpression::String(string), .. } => Some(
            string
                .segments
                .iter()
                .filter_map(|seg| match seg {
                    crate::transitional::cst::valkyrie::ast::StringSegment::Text(text) => Some(text.as_str()),
                    _ => None,
                })
                .collect(),
        ),
        _ => None,
    }
}

fn slice_span(source: &str, span: Range<usize>) -> String {
    source.get(span.clone()).unwrap_or("").trim().to_string()
}

fn error(kind: AbiIssueKind, message: impl Into<String>, span: Option<Range<usize>>) -> AbiIssue {
    AbiIssue { kind, message: message.into(), span, severity: AbiSeverity::Error }
}

fn warn(kind: AbiIssueKind, message: impl Into<String>, span: Option<Range<usize>>) -> AbiIssue {
    AbiIssue { kind, message: message.into(), span, severity: AbiSeverity::Warning }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_property_event_and_state_from_widget_script() {
        let source = r#"widget theme_switcher {
    [property]
    let theme;

    [property]
    let mode = "auto";

    let mut open = false;

    let resolved_mode = mode;

    [memoize]
    let visible_themes = filter(themes);

    [event]
    micro theme_change(theme: string) { }

    micro on_select(next_theme: string) {
        emit(theme_change, next_theme);
    }
}"#;
        let result = extract_component_abi_from_vx(source, "theme_switcher");
        assert!(result.issues.iter().all(|issue| issue.severity != AbiSeverity::Error), "{:?}", result.issues);
        assert_eq!(result.abi.properties.len(), 2);
        assert!(result.abi.properties[0].required);
        assert!(!result.abi.properties[1].required);
        assert_eq!(result.abi.states.len(), 1);
        assert_eq!(result.abi.derived.len(), 1);
        assert_eq!(result.abi.memoized.len(), 1);
        assert_eq!(result.abi.events.len(), 1);
    }

    #[test]
    fn rejects_non_empty_event_body() {
        let source = r#"widget bad {
    [event]
    micro theme_change(theme: string) { log(theme); }
}"#;
        let result = extract_component_abi_from_vx(source, "bad");
        assert!(result.issues.iter().any(|issue| issue.kind == AbiIssueKind::EventNonEmptyBody));
    }
}
