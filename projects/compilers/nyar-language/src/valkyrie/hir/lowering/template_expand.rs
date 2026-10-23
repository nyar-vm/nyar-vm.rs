//! 在 HIR lowering 前按 `arch` 展开 Oak 结构化 TGrammar 节点。

use crate::valkyrie::{
    HirError,
    frontend::{
        ast::{Block, Statement, StatementNode, TemplateIf, TemplateLoop, TemplateMatch, TemplateNode, ValkyrieRoot},
        parse_source_with_language, production_language,
    },
};

use oak_core::TokenType;

use oak_valkyrie::lexer::token_type::ValkyrieTokenType;

use super::template_const::{const_value_to_source, eval_const_condition, eval_const_expr, parse_const_loop_range};

/// 默认架构键；单文件 `compile_source` 测试夹具使用。

#[doc(hidden)]
pub const DEFAULT_COMPILE_ARCH: &str = "native";

/// 展开根节点与声明体中的 TGrammar 模板。

#[doc(hidden)]
pub fn expand_templates_in_root(root: &mut ValkyrieRoot, source: &str, arch: &str) -> Result<(), HirError> {
    expand_statement_nodes(&mut root.items, source, arch)?;

    expand_declaration_bodies(&mut root.items, source, arch)?;

    Ok(())
}

fn expand_declaration_bodies(items: &mut [StatementNode], source: &str, arch: &str) -> Result<(), HirError> {
    for item in items {
        match item {
            StatementNode::Micro(micro) => expand_block(&mut micro.body, source, arch)?,

            StatementNode::Class(class) => {
                for method in &mut class.body.methods {
                    if let Some(body) = &mut method.body {
                        expand_block(body, source, arch)?;
                    }
                }
            }

            StatementNode::Imply(imply) => {
                for method in &mut imply.methods {
                    if let Some(body) = &mut method.body {
                        expand_block(body, source, arch)?;
                    }
                }
            }

            StatementNode::Namespace(namespace) => {
                if let Some(body) = &mut namespace.body {
                    expand_declaration_bodies(&mut body.items, source, arch)?;
                }
            }

            _ => {}
        }
    }

    Ok(())
}

fn expand_block(block: &mut Block, source: &str, arch: &str) -> Result<(), HirError> {
    expand_statements(&mut block.statements, source, arch)
}

fn expand_statements(statements: &mut Vec<Statement>, source: &str, arch: &str) -> Result<(), HirError> {
    let mut index = 0;

    while index < statements.len() {
        if let Statement::Template(template) = &statements[index] {
            let expanded = expand_template_to_statements(template, source, arch)?;

            let expanded_len = expanded.len();

            statements.remove(index);

            for (offset, statement) in expanded.into_iter().enumerate() {
                statements.insert(index + offset, statement);
            }

            index += expanded_len;
        }
        else {
            index += 1;
        }
    }

    Ok(())
}

fn expand_statement_nodes(items: &mut Vec<StatementNode>, source: &str, arch: &str) -> Result<(), HirError> {
    let mut index = 0;

    while index < items.len() {
        if matches!(items[index], StatementNode::Template(_)) {
            let template = items[index].clone();

            let StatementNode::Template(template) = template
            else {
                unreachable!()
            };

            let expanded = expand_template_to_statement_nodes(&template, source, arch)?;

            let expanded_len = expanded.len();

            items.remove(index);

            for (offset, item) in expanded.into_iter().enumerate() {
                items.insert(index + offset, item);
            }

            index += expanded_len;
        }
        else if let StatementNode::Namespace(namespace) = &mut items[index] {
            if let Some(body) = &mut namespace.body {
                expand_statement_nodes(&mut body.items, source, arch)?;
            }

            index += 1;
        }
        else {
            index += 1;
        }
    }

    Ok(())
}

fn expand_template_to_statement_nodes(template: &TemplateNode, source: &str, arch: &str) -> Result<Vec<StatementNode>, HirError> {
    match template {
        TemplateNode::Match(match_node) => {
            let body = select_arch_match_body(match_node, source, arch)?;

            let mut items = body;

            expand_statement_nodes(&mut items, source, arch)?;

            Ok(items)
        }

        TemplateNode::If(if_node) => {
            let body = select_if_body(if_node, source, arch)?;

            let mut items = body;

            expand_statement_nodes(&mut items, source, arch)?;

            Ok(items)
        }

        TemplateNode::Loop(loop_node) => expand_loop_body(loop_node, source, arch),

        TemplateNode::Fragment { tokens, .. } => expand_fragment(tokens, source, arch),
    }
}

fn expand_template_to_statements(template: &TemplateNode, source: &str, arch: &str) -> Result<Vec<Statement>, HirError> {
    expand_template_to_statement_nodes(template, source, arch)?.into_iter().map(statement_node_to_statement).collect()
}

fn statement_node_to_statement(item: StatementNode) -> Result<Statement, HirError> {
    match item {
        StatementNode::Let(binding) => Ok(Statement::Let(*binding)),

        StatementNode::ExprStmt(expr_stmt) => Ok(Statement::Expression(expr_stmt)),

        StatementNode::Template(template) => Ok(Statement::Template(template)),

        other => Err(HirError::lowering_failed(format!("TGrammar 展开后不能在块内保留顶层项: {other:?}"))),
    }
}

fn select_if_body(if_node: &TemplateIf, source: &str, arch: &str) -> Result<Vec<StatementNode>, HirError> {
    for arm in &if_node.arms {
        if !arm.header.is_empty() && eval_const_condition(source, &arm.header, arch)? {
            return Ok(arm.body.clone());
        }
    }

    for arm in &if_node.arms {
        if arm.header.is_empty() {
            return Ok(arm.body.clone());
        }
    }

    Ok(Vec::new())
}

fn expand_loop_body(loop_node: &TemplateLoop, source: &str, arch: &str) -> Result<Vec<StatementNode>, HirError> {
    let range = parse_const_loop_range(source, &loop_node.header, arch)?;

    let mut items = Vec::new();

    for _ in range {
        let mut body = loop_node.body.clone();

        expand_statement_nodes(&mut body, source, arch)?;

        items.extend(body);
    }

    Ok(items)
}

fn expand_fragment(tokens: &[oak_core::Token<ValkyrieTokenType>], source: &str, arch: &str) -> Result<Vec<StatementNode>, HirError> {
    let snippet = const_value_to_source(&eval_const_expr(source, tokens, arch)?);
    if snippet.trim().is_empty() {
        return Ok(Vec::new());
    }
    // 元表达式 splice 发生在语句上下文中，须经块内重解析而非模块根。
    let wrapped = format!("micro __tg_fragment() -> void {{ {snippet}; }}");
    let root = parse_source_with_language(&wrapped, &production_language())?;
    let StatementNode::Micro(micro) = root.items.first().ok_or_else(|| HirError::lowering_failed("TGrammar 元表达式展开失败"))?
    else {
        return Err(HirError::lowering_failed("TGrammar 元表达式展开失败"));
    };
    Ok(micro.body.statements.iter().cloned().map(statement_to_statement_node).collect())
}

fn statement_to_statement_node(statement: Statement) -> StatementNode {
    match statement {
        Statement::Let(binding) => StatementNode::Let(Box::new(binding)),
        Statement::Expression(expr_stmt) => StatementNode::ExprStmt(expr_stmt),
        Statement::Template(template) => StatementNode::Template(template),
    }
}

fn select_arch_match_body(match_node: &TemplateMatch, source: &str, arch: &str) -> Result<Vec<StatementNode>, HirError> {
    if !is_arch_scrutinee(source, &match_node.header) {
        return Err(HirError::lowering_failed("TGrammar `<% match %>` 当前仅支持按 `arch` 展开"));
    }

    for arm in &match_node.arms {
        if let Some(pattern) = &arm.pattern {
            if pattern_token_text(source, pattern) == arch {
                return Ok(arm.body.clone());
            }
        }
    }

    for arm in &match_node.arms {
        if arm.pattern.is_none() {
            return Ok(arm.body.clone());
        }
    }

    Ok(Vec::new())
}

fn is_arch_scrutinee(source: &str, header: &[oak_core::Token<ValkyrieTokenType>]) -> bool {
    pattern_token_text(source, header) == "arch"
}

fn pattern_token_text(source: &str, tokens: &[oak_core::Token<ValkyrieTokenType>]) -> String {
    let meaningful: Vec<_> = tokens.iter().filter(|token| !token.kind.is_ignored()).collect();

    match meaningful.as_slice() {
        [token] if token.kind == ValkyrieTokenType::StringLiteral => decode_quoted(source[token.span.clone()].trim()),

        [token] if token.kind == ValkyrieTokenType::Identifier => source[token.span.clone()].trim().to_string(),

        _ => {
            tokens.iter().filter(|token| !token.kind.is_ignored()).map(|token| source[token.span.clone()].trim()).collect::<Vec<_>>().join(" ")
        }
    }
}

fn decode_quoted(text: &str) -> String {
    let text = text.trim();

    if (text.starts_with('"') && text.ends_with('"')) || (text.starts_with('\'') && text.ends_with('\'')) {
        text[1..text.len().saturating_sub(1)].to_string()
    }
    else {
        text.to_string()
    }
}
