//! 在 HIR lowering 前按 `arch` 展开 Oak 结构化 TGrammar 节点。

use crate::valkyrie::frontend::{
    ParseError,
    ast::{Block, Statement, StatementNode, TemplateMatch, TemplateNode, ValkyrieRoot},
};
use oak_core::TokenType;
use oak_valkyrie::lexer::token_type::ValkyrieTokenType;

/// 默认架构键；单文件 `compile_source` 测试夹具使用。
pub(crate) const DEFAULT_COMPILE_ARCH: &str = "native";

/// 展开根节点与声明体中的 TGrammar 模板。
pub(crate) fn expand_templates_in_root(root: &mut ValkyrieRoot, source: &str, arch: &str) -> Result<(), ParseError> {
    expand_statement_nodes(&mut root.items, source, arch)?;
    expand_declaration_bodies(&mut root.items, source, arch)?;
    Ok(())
}

fn expand_declaration_bodies(items: &mut [StatementNode], source: &str, arch: &str) -> Result<(), ParseError> {
    for item in items {
        match item {
            StatementNode::Micro(micro) => expand_block(&mut micro.body, source, arch)?,
            StatementNode::Class(class) => {
                for method in &mut class.methods {
                    if let Some(body) = &mut method.body {
                        expand_block(body, source, arch)?;
                    }
                }
            }
            StatementNode::Namespace(namespace) if !namespace.items.is_empty() => {
                expand_declaration_bodies(&mut namespace.items, source, arch)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn expand_block(block: &mut Block, source: &str, arch: &str) -> Result<(), ParseError> {
    expand_statements(&mut block.statements, source, arch)
}

fn expand_statements(statements: &mut Vec<Statement>, source: &str, arch: &str) -> Result<(), ParseError> {
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

fn expand_statement_nodes(items: &mut Vec<StatementNode>, source: &str, arch: &str) -> Result<(), ParseError> {
    let mut index = 0;
    while index < items.len() {
        if matches!(items[index], StatementNode::Template(_)) {
            let template = items[index].clone();
            let StatementNode::Template(template) = template else { unreachable!() };
            let expanded = expand_template_to_statement_nodes(&template, source, arch)?;
            let expanded_len = expanded.len();
            items.remove(index);
            for (offset, item) in expanded.into_iter().enumerate() {
                items.insert(index + offset, item);
            }
            index += expanded_len;
        }
        else if let StatementNode::Namespace(namespace) = &mut items[index] {
            if !namespace.items.is_empty() {
                expand_statement_nodes(&mut namespace.items, source, arch)?;
            }
            index += 1;
        }
        else {
            index += 1;
        }
    }
    Ok(())
}

fn expand_template_to_statement_nodes(template: &TemplateNode, source: &str, arch: &str) -> Result<Vec<StatementNode>, ParseError> {
    match template {
        TemplateNode::Match(match_node) => {
            let body = select_arch_match_body(match_node, source, arch)?;
            let mut items = body;
            expand_statement_nodes(&mut items, source, arch)?;
            Ok(items)
        }
        TemplateNode::If(_) | TemplateNode::Loop(_) => Err(ParseError::invalid("TGrammar `if`/`loop` 模板尚未接入 HIR 展开")),
        TemplateNode::Fragment { .. } => Err(ParseError::invalid("TGrammar 单行 meta 指令尚未接入 HIR 展开")),
    }
}

fn expand_template_to_statements(template: &TemplateNode, source: &str, arch: &str) -> Result<Vec<Statement>, ParseError> {
    expand_template_to_statement_nodes(template, source, arch)?
        .into_iter()
        .map(statement_node_to_statement)
        .collect()
}

fn statement_node_to_statement(item: StatementNode) -> Result<Statement, ParseError> {
    match item {
        StatementNode::Let(binding) => Ok(Statement::Let(*binding)),
        StatementNode::ExprStmt(expr_stmt) => Ok(Statement::Expression(expr_stmt)),
        StatementNode::Template(template) => Ok(Statement::Template(template)),
        other => Err(ParseError::invalid(format!("TGrammar 展开后不能在块内保留顶层项: {other:?}"))),
    }
}

fn select_arch_match_body(match_node: &TemplateMatch, source: &str, arch: &str) -> Result<Vec<StatementNode>, ParseError> {
    if !is_arch_scrutinee(source, &match_node.header) {
        return Err(ParseError::invalid("TGrammar `<% match %>` 当前仅支持按 `arch` 展开"));
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
        _ => tokens
            .iter()
            .filter(|token| !token.kind.is_ignored())
            .map(|token| source[token.span.clone()].trim())
            .collect::<Vec<_>>()
            .join(" "),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::valkyrie::frontend::{ast::StatementNode, parse_source};

    #[test]
    fn expands_module_level_arch_match() {
        let source = r#"<% match arch %>
<% case "wasm32" %>
micro main() -> i32 { return 23 }
<% else %>
micro main() -> i32 { return 0 }
<% end %>"#;
        let mut root = parse_source(source).expect("parse");
        expand_templates_in_root(&mut root, source, "wasm32").expect("expand");
        assert_eq!(root.items.len(), 1);
        assert!(matches!(root.items[0], StatementNode::Micro(_)));
    }

    #[test]
    fn parses_template_match_inside_micro_body_before_expand() {
        let source = r#"micro main() -> i32 {
<% match arch %>
<% case "wasm32" %>
return 23
<% else %>
return 0
<% end %>
}"#;
        let root = parse_source(source).expect("parse");
        let StatementNode::Micro(micro) = &root.items[0] else { panic!("expected micro") };
        assert_eq!(micro.body.statements.len(), 1, "micro body should contain one template statement");
        let Statement::Template(template) = &micro.body.statements[0] else { panic!("expected template statement") };
        let TemplateNode::Match(match_node) = template.as_ref() else { panic!("expected match template") };
        assert_eq!(match_node.arms.len(), 2);
        assert_eq!(match_node.arms[0].body.len(), 1, "wasm32 arm body should contain parsed `return` statement");
    }

    #[test]
    fn expands_arch_match_inside_micro_body() {
        let source = r#"micro main() -> i32 {
<% match arch %>
<% case "wasm32" %>
return 23
<% else %>
return 0
<% end %>
}"#;
        let mut root = parse_source(source).expect("parse");
        expand_templates_in_root(&mut root, source, "wasm32").expect("expand");
        let StatementNode::Micro(micro) = &root.items[0] else { panic!("expected micro") };
        assert!(!micro.body.statements.is_empty());
        assert!(!matches!(micro.body.statements[0], Statement::Template(_)));
    }
}
