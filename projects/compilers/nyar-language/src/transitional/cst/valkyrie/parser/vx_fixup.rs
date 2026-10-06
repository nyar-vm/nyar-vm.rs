//! `.vx` widget `view` / `render` 方法体 markup 重解析（恢复 lexer 跳过的 `<% %>`）。

use crate::transitional::cst::valkyrie::{
    ast::{ClassDeclaration, ClassLikeKind, DeclarationBody, RootStatement, TermExpression, ValkyrieRoot},
    xml::parse_xgrammar_template,
};

/// 对 widget 的 `view` / `render` 方法体做源码级 X+T 重解析。
pub fn fixup_vx_widget_view_markup(root: &mut ValkyrieRoot, source: &str) {
    for statement in &mut root.statements {
        let RootStatement::Class(class) = statement
        else {
            continue;
        };
        if class.kind != ClassLikeKind::Widget {
            continue;
        }
        fixup_widget_methods(class, source);
    }
}

fn fixup_widget_methods(class: &mut ClassDeclaration, source: &str) {
    for method in &mut class.body.methods {
        let name = method.name.name.as_str();
        if name != "view" && name != "render" {
            continue;
        }
        let Some(body) = method.body.as_mut()
        else {
            continue;
        };
        fixup_view_body_markup(body, source);
    }
}

fn fixup_view_body_markup(body: &mut DeclarationBody, source: &str) {
    let slice = source.get(body.span.clone()).unwrap_or("");
    if !slice.contains("<%") {
        return;
    }
    let trimmed = slice.trim();
    if trimmed.is_empty() {
        return;
    }
    let Ok(nodes) = parse_xgrammar_template(trimmed)
    else {
        return;
    };
    if nodes.is_empty() {
        return;
    }
    body.statements.clear();
    body.tail_expression = Some(TermExpression::XmlMarkup { nodes, span: body.span.clone() });
}
