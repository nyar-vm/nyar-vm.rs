//! Oak AST `snake_case` naming lint for LSP surfaces.

use std::ops::Range;

use oak_valkyrie::ast::{
    Block, ClassDeclaration, ImplyDeclaration, Let, MethodDeclaration, MicroDeclaration, NamespaceDeclaration, Param, Pattern,
    Property, SingletonDeclaration, Statement, StatementNode, Trait, TypeFunction, ValkyrieRoot, WidgetDeclaration,
};
use std_data::text::{
    awsl::is_snake_case,
    valkyrie::naming::{DIAG_IDENTIFIER_NOT_SNAKE_CASE, NamingViolation},
};

/// Validate `let` bindings, `micro`/`method` declarations, and parameters on Oak AST.
pub fn validate_snake_case(root: &ValkyrieRoot) -> Vec<NamingViolation> {
    let mut violations = Vec::new();
    for item in &root.items {
        walk_statement_node(item, &mut violations);
    }
    violations
}

fn walk_statement_node(node: &StatementNode, violations: &mut Vec<NamingViolation>) {
    match node {
        StatementNode::Namespace(namespace) => walk_namespace(namespace, violations),
        StatementNode::Class(class) => walk_class(class, violations),
        StatementNode::Singleton(singleton) => walk_singleton(singleton, violations),
        StatementNode::Trait(trait_decl) => walk_trait(trait_decl, violations),
        StatementNode::Imply(imply) => walk_imply(imply, violations),
        StatementNode::Widget(widget) => walk_widget(widget, violations),
        StatementNode::Micro(micro) => walk_micro(micro, violations),
        StatementNode::TypeFunction(type_function) => walk_type_function(type_function, violations),
        StatementNode::Property(property) => walk_property(property, violations),
        StatementNode::Let(let_stmt) => walk_let_binding(let_stmt, violations),
        StatementNode::Statement(inner) => walk_statement_node(inner, violations),
        _ => {}
    }
}

fn walk_namespace(namespace: &NamespaceDeclaration, violations: &mut Vec<NamingViolation>) {
    for item in &namespace.items {
        walk_statement_node(item, violations);
    }
}

fn walk_class(class: &ClassDeclaration, violations: &mut Vec<NamingViolation>) {
    for method in &class.methods {
        walk_method(method, violations);
    }
}

fn walk_singleton(singleton: &SingletonDeclaration, violations: &mut Vec<NamingViolation>) {
    for method in &singleton.methods {
        walk_method(method, violations);
    }
}

fn walk_trait(trait_decl: &Trait, violations: &mut Vec<NamingViolation>) {
    for method in &trait_decl.methods {
        walk_method(method, violations);
    }
}

fn walk_imply(imply: &ImplyDeclaration, violations: &mut Vec<NamingViolation>) {
    for method in &imply.methods {
        walk_method(method, violations);
    }
}

fn walk_widget(widget: &WidgetDeclaration, violations: &mut Vec<NamingViolation>) {
    for item in &widget.items {
        walk_statement_node(item, violations);
    }
}

fn walk_micro(micro: &MicroDeclaration, violations: &mut Vec<NamingViolation>) {
    check_identifier(&micro.name.name, std_range(&micro.name.span), violations);
    for param in &micro.params {
        walk_parameter(param, violations);
    }
    walk_block(&micro.body, violations);
}

fn walk_type_function(type_function: &TypeFunction, violations: &mut Vec<NamingViolation>) {
    check_identifier(&type_function.name.name, std_range(&type_function.name.span), violations);
    for param in &type_function.params {
        walk_parameter(param, violations);
    }
    walk_block(&type_function.body, violations);
}

fn walk_property(property: &Property, violations: &mut Vec<NamingViolation>) {
    check_identifier(&property.name.name, std_range(&property.name.span), violations);
    for param in &property.params {
        walk_parameter(param, violations);
    }
    walk_block(&property.body, violations);
}

fn walk_method(method: &MethodDeclaration, violations: &mut Vec<NamingViolation>) {
    check_identifier(&method.name.name, std_range(&method.name.span), violations);
    for param in &method.params {
        walk_parameter(param, violations);
    }
    if let Some(body) = &method.body {
        walk_block(body, violations);
    }
}

fn walk_block(block: &Block, violations: &mut Vec<NamingViolation>) {
    for statement in &block.statements {
        walk_statement(statement, violations);
    }
}

fn walk_statement(statement: &Statement, violations: &mut Vec<NamingViolation>) {
    match statement {
        Statement::Let(let_stmt) => walk_let_binding(let_stmt, violations),
        Statement::ExprStmt(_) => {}
    }
}

fn walk_let_binding(let_stmt: &Let, violations: &mut Vec<NamingViolation>) {
    if let Some((name, span)) = pattern_binding_name(&let_stmt.pattern) {
        check_identifier(name, span, violations);
    }
}

fn walk_parameter(param: &Param, violations: &mut Vec<NamingViolation>) {
    check_identifier(&param.name.name, std_range(&param.name.span), violations);
}

fn check_identifier(name: &str, span: Range<usize>, violations: &mut Vec<NamingViolation>) {
    if is_snake_case(name) {
        return;
    }
    violations.push(NamingViolation {
        code: DIAG_IDENTIFIER_NOT_SNAKE_CASE,
        name: name.to_string(),
        name_span: span,
    });
}

fn pattern_binding_name(pattern: &Pattern) -> Option<(&str, Range<usize>)> {
    match pattern {
        Pattern::Variable(variable) => Some((variable.name.name.as_str(), std_range(&variable.span))),
        Pattern::Class(class_pattern) => class_pattern.fields.first().map(|(name, _)| (name.name.as_str(), std_range(&name.span))),
        _ => None,
    }
}

fn std_range(span: &oak_core::Range<usize>) -> Range<usize> {
    span.start..span.end
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::valkyrie::frontend::parse_source;
    use std_data::text::valkyrie::naming::DIAG_IDENTIFIER_NOT_SNAKE_CASE;

    #[test]
    fn rejects_camel_case_let_micro_and_param() {
        let source = r#"micro demo(onClick: i32) -> i32 {
    let fooBar = 1;
    return onClick;
}
"#;
        let root = parse_source(source).expect("fixture should parse");
        let violations = validate_snake_case(&root);
        let names: Vec<_> = violations.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"onClick"));
        assert!(names.contains(&"fooBar"));
        assert!(violations.iter().all(|v| v.code == DIAG_IDENTIFIER_NOT_SNAKE_CASE));
    }

    #[test]
    fn rejects_camel_case_in_widget_script() {
        let source = r#"widget counter {
    let themeChange = 0;

    micro onTap() {
        themeChange = themeChange + 1;
    }

    micro defaultActive() -> bool {
        return true;
    }
}"#;
        let root = parse_source(source).expect("widget fixture should parse");
        let violations = validate_snake_case(&root);
        let names: Vec<_> = violations.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"themeChange"));
        assert!(names.contains(&"onTap"));
        assert!(names.contains(&"defaultActive"));
    }

    #[test]
    fn accepts_valid_snake_case_names() {
        let source = r#"micro on_click(item_count: i32) -> i32 {
    let theme_change = item_count;
    return theme_change;
}
"#;
        let root = parse_source(source).expect("valid fixture should parse");
        let violations = validate_snake_case(&root);
        assert!(violations.is_empty(), "{violations:?}");
    }
}
