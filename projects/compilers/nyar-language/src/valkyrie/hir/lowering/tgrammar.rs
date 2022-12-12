//! T-Grammar 编译期展开：将 `Template` / `XgNode::Meta` 降为普通 Valkyrie AST。

use std::ops::Range;

use std_data::text::valkyrie::{
    AstParser, BinaryOperator, ClassDeclaration, DeclarationBody, FunctionStatement, LiteralExpression, ObjectMethodDeclaration, ParseError,
    RootStatement, TermExpression, ValkyrieRoot,
    ast::{IfStatement, LoopInStatement},
    tgrammar::{TgIf, TgNode, TgRoot, TgTextPart},
    xml::{XgNode, XgRoot, parse_xgrammar_template},
};

/// 将 `TgRoot` 展开为普通 `TermExpression`（供 expr lowering 兜底）。
pub fn expand_tg_root_for_lower(nodes: &TgRoot, span: Range<usize>) -> TermExpression {
    expand_tg_root(nodes, span)
}

/// 在 HIR 降级前展开根 AST 中的 T-Grammar 节点。
pub fn expand_tgrammar_in_root(root: &mut ValkyrieRoot) {
    for statement in &mut root.statements {
        expand_in_root_statement(statement);
    }
}

fn expand_in_root_statement(statement: &mut RootStatement) {
    match statement {
        RootStatement::Function(function) => {
            if let Some(body) = &mut function.body {
                expand_in_declaration_body(body);
            }
        }
        RootStatement::Class(class) => expand_in_class(class),
        _ => {}
    }
}

fn expand_in_class(class: &mut ClassDeclaration) {
    for method in &mut class.body.methods {
        expand_in_method(method);
    }
    for field in &mut class.body.fields {
        if let Some(value) = &mut field.default_value {
            *value = expand_term_expression(value.clone());
        }
    }
}

fn expand_in_method(method: &mut ObjectMethodDeclaration) {
    if let Some(body) = &mut method.body {
        expand_in_declaration_body(body);
    }
}

fn expand_in_declaration_body(body: &mut DeclarationBody) {
    for statement in &mut body.statements {
        if let FunctionStatement::Term { expression, .. } = statement {
            *expression = expand_term_expression(expression.clone());
        }
    }
    if let Some(tail) = &body.tail_expression {
        body.tail_expression = Some(expand_term_expression(tail.clone()));
    }
}

fn expand_term_expression(expression: TermExpression) -> TermExpression {
    match expression {
        TermExpression::Template { nodes, span } => expand_tg_root(&nodes, span),
        TermExpression::XmlMarkup { mut nodes, span } => {
            if let Some(replacement) = try_expand_xml_meta_root(&mut nodes, span.clone()) {
                return expand_term_expression(replacement);
            }
            TermExpression::XmlMarkup { nodes, span }
        }
        TermExpression::Unary(inner) => {
            let mut unary = *inner;
            unary.base = expand_term_expression(unary.base);
            TermExpression::Unary(Box::new(unary))
        }
        TermExpression::Binary(inner) => {
            let mut binary = *inner;
            binary.lhs = expand_term_expression(binary.lhs);
            binary.rhs = expand_term_expression(binary.rhs);
            TermExpression::Binary(Box::new(binary))
        }
        TermExpression::Block { body, is_unsafe, span } => {
            let mut block = *body;
            expand_in_declaration_body(&mut block);
            TermExpression::Block { body: Box::new(block), is_unsafe, span }
        }
        other => other,
    }
}

fn try_expand_xml_meta_root(nodes: &mut XgRoot, span: Range<usize>) -> Option<TermExpression> {
    if nodes.len() != 1 {
        return None;
    }
    let XgNode::Meta { nodes: tg_nodes, span: meta_span } = nodes.remove(0)
    else {
        return None;
    };
    if tg_nodes.len() != 1 {
        nodes.push(XgNode::Meta { nodes: tg_nodes, span: meta_span });
        return None;
    }
    let tg_node = tg_nodes.into_iter().next().unwrap();
    let TermExpression::If(if_stmt) = expand_tg_node_to_control_flow(&tg_node, span)
    else {
        nodes.push(XgNode::Meta { nodes: vec![tg_node], span: meta_span });
        return None;
    };
    Some(TermExpression::If(if_stmt))
}

fn expand_tg_root(nodes: &TgRoot, span: Range<usize>) -> TermExpression {
    if nodes.is_empty() {
        return empty_string_literal(span);
    }
    if nodes.len() == 1 {
        return expand_tg_node(nodes.first().unwrap(), span);
    }
    let mut parts = Vec::new();
    for node in nodes {
        parts.push(expand_tg_node(node, span.clone()));
    }
    fold_string_concat(parts, span)
}

fn expand_tg_node(node: &TgNode, span: Range<usize>) -> TermExpression {
    match node {
        TgNode::Text { parts, span } => text_parts_to_expression(parts, span.clone()),
        TgNode::Comment { .. } => empty_string_literal(span.clone()),
        TgNode::Stmt { body, span } => stmt_to_block_expression(body, span.clone()),
        TgNode::If(if_node) => TermExpression::If(expand_tg_if(if_node)),
        TgNode::Loop(loop_node) => loop_header_to_expression(&loop_node.header, &loop_node.body, loop_node.span.clone()),
        TgNode::Match(match_node) => parse_tail_expression(&format!(
            "match {} {{ {} }}",
            match_node.scrutinee,
            match_node
                .arms
                .iter()
                .map(|arm| {
                    if let Some(pattern) = &arm.pattern {
                        format!("case {pattern} {{ __TG_BODY__ }}")
                    }
                    else {
                        "else { __TG_BODY__ }".to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        ))
        .unwrap_or_else(|_| empty_string_literal(span)),
    }
}

fn expand_tg_node_to_control_flow(node: &TgNode, fallback_span: Range<usize>) -> TermExpression {
    match node {
        TgNode::If(if_node) => TermExpression::If(expand_tg_if_with_xml(if_node, fallback_span)),
        other => expand_tg_node(other, fallback_span),
    }
}

fn expand_tg_if_with_xml(if_node: &TgIf, fallback_span: Range<usize>) -> Box<IfStatement> {
    let arms = &if_node.arms;
    let (chain, else_arm) = if arms.last().is_some_and(|arm| arm.condition.is_none()) && arms.len() > 1 {
        (&arms[..arms.len() - 1], Some(&arms[arms.len() - 1]))
    }
    else {
        (arms.as_slice(), None)
    };
    Box::new(build_if_chain(chain, else_arm, fallback_span))
}

fn build_if_chain(
    arms: &[std_data::text::valkyrie::tgrammar::TgIfArm],
    else_arm: Option<&std_data::text::valkyrie::tgrammar::TgIfArm>,
    fallback_span: Range<usize>,
) -> IfStatement {
    if let Some(arm) = arms.first() {
        let else_body = if arms.len() > 1 {
            Some(DeclarationBody {
                statements: Vec::new(),
                tail_expression: Some(TermExpression::If(Box::new(build_if_chain(&arms[1..], else_arm, fallback_span.clone())))),
                span: fallback_span.clone(),
            })
        }
        else if let Some(else_arm) = else_arm {
            Some(tg_root_to_declaration_body(&else_arm.body, else_arm.span.clone()))
        }
        else {
            None
        };
        return IfStatement {
            condition: arm
                .condition
                .as_ref()
                .map(|text| parse_tail_expression(text).unwrap_or_else(|_| bool_literal(true, arm.span.clone())))
                .unwrap_or_else(|| bool_literal(true, arm.span.clone())),
            then_body: tg_root_to_declaration_body(&arm.body, arm.span.clone()),
            else_body,
            span: arm.span.clone(),
        };
    }
    if let Some(else_arm) = else_arm {
        return IfStatement {
            condition: bool_literal(true, else_arm.span.clone()),
            then_body: tg_root_to_declaration_body(&else_arm.body, else_arm.span.clone()),
            else_body: None,
            span: else_arm.span.clone(),
        };
    }
    IfStatement {
        condition: bool_literal(false, fallback_span.clone()),
        then_body: DeclarationBody { statements: Vec::new(), tail_expression: None, span: fallback_span.clone() },
        else_body: None,
        span: fallback_span,
    }
}

fn expand_tg_if(if_node: &TgIf) -> Box<IfStatement> {
    expand_tg_if_with_xml(if_node, if_node.span.clone())
}

fn tg_root_to_declaration_body(nodes: &TgRoot, span: Range<usize>) -> DeclarationBody {
    if let Some(markup) = tg_root_to_xml_markup(nodes, span.clone()) {
        return DeclarationBody { statements: Vec::new(), tail_expression: Some(markup), span };
    }
    DeclarationBody {
        statements: nodes
            .iter()
            .filter_map(|node| match node {
                TgNode::Stmt { body, span } => {
                    Some(FunctionStatement::Term { expression: stmt_to_block_expression(body, span.clone()), span: span.clone() })
                }
                _ => None,
            })
            .collect(),
        tail_expression: Some(expand_tg_root(nodes, span.clone())),
        span,
    }
}

fn tg_root_to_xml_markup(nodes: &TgRoot, span: Range<usize>) -> Option<TermExpression> {
    if nodes.len() == 1
        && let TgNode::Text { parts, .. } = &nodes[0]
    {
        let mut combined = String::new();
        for part in parts {
            match part {
                TgTextPart::Static(text) => combined.push_str(text),
                TgTextPart::Expression(expr) => {
                    combined.push('{');
                    combined.push_str(expr);
                    combined.push('}');
                }
            }
        }
        let parsed = parse_xgrammar_template(combined.trim()).ok()?;
        if parsed.is_empty() {
            return None;
        }
        return Some(TermExpression::XmlMarkup { nodes: parsed, span });
    }
    None
}

fn text_parts_to_expression(parts: &[TgTextPart], span: Range<usize>) -> TermExpression {
    let mut exprs = Vec::new();
    for part in parts {
        match part {
            TgTextPart::Static(text) if text.is_empty() => {}
            TgTextPart::Static(text) => exprs.push(string_literal(text.clone(), span.clone())),
            TgTextPart::Expression(text) => {
                exprs.push(parse_tail_expression(text).unwrap_or_else(|_| string_literal(text.clone(), span.clone())));
            }
        }
    }
    if exprs.is_empty() {
        return empty_string_literal(span);
    }
    fold_string_concat(exprs, span)
}

fn fold_string_concat(mut exprs: Vec<TermExpression>, span: Range<usize>) -> TermExpression {
    let mut iter = exprs.drain(..);
    let Some(mut acc) = iter.next()
    else {
        return empty_string_literal(span);
    };
    for rhs in iter {
        acc = TermExpression::Binary(Box::new(std_data::text::valkyrie::ast::TermBinaryExpression {
            operator: BinaryOperator::Add,
            lhs: acc,
            rhs,
            span: span.clone(),
        }));
    }
    acc
}

fn stmt_to_block_expression(stmt: &str, span: Range<usize>) -> TermExpression {
    let source = format!("micro __tgrammar_stmt() {{ {stmt}; }}");
    if let Ok(root) = AstParser::parse_root(&source)
        && let Some(RootStatement::Function(function)) = root.statements.first()
        && let Some(body) = &function.body
    {
        return TermExpression::Block { body: Box::new(body.clone()), is_unsafe: false, span };
    }
    TermExpression::Block {
        body: Box::new(DeclarationBody { statements: Vec::new(), tail_expression: None, span: span.clone() }),
        is_unsafe: false,
        span,
    }
}

fn loop_header_to_expression(header: &str, body: &TgRoot, span: Range<usize>) -> TermExpression {
    let source = format!("micro __tgrammar_loop() {{ loop {header} {{ 1 }} }}");
    if let Ok(root) = AstParser::parse_root(&source)
        && let Some(RootStatement::Function(function)) = root.statements.first()
        && let Some(DeclarationBody { tail_expression: Some(TermExpression::LoopIn(mut loop_in)), .. }) = function.body.clone()
    {
        loop_in.body = tg_root_to_declaration_body(body, span.clone());
        loop_in.span = span.clone();
        return TermExpression::LoopIn(loop_in);
    }
    TermExpression::LoopIn(Box::new(LoopInStatement {
        label: None,
        pattern: None,
        iterator: None,
        condition: None,
        body: tg_root_to_declaration_body(body, span.clone()),
        span,
    }))
}

fn parse_tail_expression(fragment: &str) -> Result<TermExpression, ParseError> {
    let source = format!("micro __tgrammar_tail() {{ {fragment} }}");
    let root = AstParser::parse_root(&source)?;
    let Some(RootStatement::Function(function)) = root.statements.first()
    else {
        return Err(ParseError::invalid("expected synthetic tail wrapper"));
    };
    let Some(body) = &function.body
    else {
        return Err(ParseError::invalid("expected synthetic tail body"));
    };
    body.tail_expression.clone().ok_or_else(|| ParseError::invalid("expected tail expression"))
}

fn string_literal(text: String, span: Range<usize>) -> TermExpression {
    TermExpression::Literal {
        literal: LiteralExpression::String(std_data::text::valkyrie::StringLiteral {
            prefix: None,
            quote_count: 1,
            segments: vec![std_data::text::valkyrie::StringSegment::Text(text)],
        }),
        span,
    }
}

fn empty_string_literal(span: Range<usize>) -> TermExpression {
    string_literal(String::new(), span)
}

fn bool_literal(value: bool, span: Range<usize>) -> TermExpression {
    TermExpression::Literal { literal: LiteralExpression::Bool(value), span }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std_data::text::valkyrie::tgrammar::parse_tgrammar_template;

    #[test]
    fn expands_template_text_interpolation() {
        let nodes = parse_tgrammar_template("Hello {name}!").expect("parse");
        let expanded = expand_tg_root(&nodes, 0..1);
        assert!(matches!(expanded, TermExpression::Binary(_)));
    }

    #[test]
    fn expands_xml_meta_if_to_if_expression() {
        let mut root = AstParser::parse_vx_root(
            r#"
widget W {
    micro view() {
        <% if show %><div>{x}</div><% end %>
    }
}
"#,
        )
        .expect("parse");
        expand_tgrammar_in_root(&mut root);
        let widget = match &root.statements[0] {
            RootStatement::Class(class) => class,
            _ => panic!("widget"),
        };
        let view = widget.body.methods.iter().find(|m| m.name.name.as_str() == "view").expect("view");
        assert!(matches!(view.body.as_ref().and_then(|b| b.tail_expression.as_ref()), Some(TermExpression::If(_))));
    }
}
