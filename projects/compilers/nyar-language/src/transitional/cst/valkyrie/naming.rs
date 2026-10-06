//! Lint-level `snake_case` naming checks for Valkyrie / vx sources.
//! Does not reject parsing; surfaced as LSP / IDE warnings (`E0301`).

use std::ops::Range;

use crate::transitional::cst::{
    awsl::is_snake_case,
    valkyrie::ast::{
        ClassDeclaration, DeclarationBody, FunctionDeclaration, FunctionParameter, FunctionStatement, ImplyDeclaration, LetStatement,
        NamespaceDeclaration, ObjectBody, ObjectMethodDeclaration, PatternExpression, RootStatement, TestsDeclaration, TraitDeclaration,
        ValkyrieRoot,
    },
};

/// Diagnostic code: identifier must be `snake_case` (`E0301`).
pub const DIAG_IDENTIFIER_NOT_SNAKE_CASE: u32 = 0x0301;

/// Diagnostic code: AWSL ABI template binding must be `snake_case` (`E0302`).
pub const DIAG_ABI_BINDING_NOT_SNAKE_CASE: u32 = 0x0302;

/// One naming violation with source span of the identifier token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamingViolation {
    /// LSP / compiler diagnostic code (`E0301` or `E0302`).
    pub code: u32,
    /// Offending identifier text.
    pub name: String,
    /// Span of the identifier in the parsed source buffer.
    pub name_span: Range<usize>,
}

/// Stable diagnostic message prefix for naming violations.
pub fn naming_message(name: &str) -> String {
    format!("Name '{name}' should be snake_case")
}

/// Validate `let` bindings, `micro`/`function`/`method` declarations, and parameters.
pub fn validate_snake_case(root: &ValkyrieRoot) -> Vec<NamingViolation> {
    let mut violations = Vec::new();
    for statement in &root.statements {
        walk_root_statement(statement, &mut violations);
    }
    violations
}

fn walk_root_statement(statement: &RootStatement, violations: &mut Vec<NamingViolation>) {
    match statement {
        RootStatement::Namespace(namespace) => walk_namespace(namespace, violations),
        RootStatement::Function(function) => walk_function_declaration(function, violations),
        RootStatement::Class(class) => walk_object_body(&class.body, violations),
        RootStatement::Trait(trait_decl) => walk_trait(trait_decl, violations),
        RootStatement::Imply(imply) => walk_imply(imply, violations),
        RootStatement::Tests(tests) => walk_declaration_body(&tests.body, violations),
        _ => {}
    }
}

fn walk_namespace(namespace: &NamespaceDeclaration, violations: &mut Vec<NamingViolation>) {
    if let Some(body) = &namespace.body {
        walk_declaration_body(body, violations);
    }
}

fn walk_trait(trait_decl: &TraitDeclaration, violations: &mut Vec<NamingViolation>) {
    walk_object_body(&trait_decl.body, violations);
}

fn walk_imply(imply: &ImplyDeclaration, violations: &mut Vec<NamingViolation>) {
    for method in &imply.methods {
        walk_object_method(method, violations);
    }
}

fn walk_object_body(body: &ObjectBody, violations: &mut Vec<NamingViolation>) {
    for statement in &body.script_statements {
        walk_function_statement(statement, violations);
    }
    for method in &body.methods {
        walk_object_method(method, violations);
    }
}

fn walk_declaration_body(body: &DeclarationBody, violations: &mut Vec<NamingViolation>) {
    for statement in &body.statements {
        walk_function_statement(statement, violations);
    }
}

fn walk_function_statement(statement: &FunctionStatement, violations: &mut Vec<NamingViolation>) {
    match statement {
        FunctionStatement::Let(let_stmt) => walk_let_binding(let_stmt, violations),
        FunctionStatement::Function { function, .. } => walk_function_declaration(function, violations),
        _ => {}
    }
}

fn walk_let_binding(let_stmt: &LetStatement, violations: &mut Vec<NamingViolation>) {
    if let Some((name, span)) = pattern_binding_name(&let_stmt.pattern) {
        check_identifier(name, span, violations);
    }
}

fn walk_function_declaration(function: &FunctionDeclaration, violations: &mut Vec<NamingViolation>) {
    check_identifier(function.name.as_str(), function.name.span.clone(), violations);
    for param in &function.params {
        walk_parameter(param, violations);
    }
    if let Some(body) = &function.body {
        walk_declaration_body(body, violations);
    }
}

fn walk_object_method(method: &ObjectMethodDeclaration, violations: &mut Vec<NamingViolation>) {
    check_identifier(method.name.as_str(), method.name.span.clone(), violations);
    for param in &method.params {
        walk_parameter(param, violations);
    }
    if let Some(body) = &method.body {
        walk_declaration_body(body, violations);
    }
}

fn walk_parameter(param: &FunctionParameter, violations: &mut Vec<NamingViolation>) {
    check_identifier(param.name.as_str(), param.name.span.clone(), violations);
}

fn check_identifier(name: &str, span: Range<usize>, violations: &mut Vec<NamingViolation>) {
    if is_snake_case(name) {
        return;
    }
    violations.push(NamingViolation { code: DIAG_IDENTIFIER_NOT_SNAKE_CASE, name: name.to_string(), name_span: span });
}

fn pattern_binding_name(pattern: &PatternExpression) -> Option<(&str, Range<usize>)> {
    match pattern {
        PatternExpression::Variable { name, span } => Some((name.as_str(), span.clone())),
        PatternExpression::TypedBind { name, span, .. } => Some((name.as_str(), span.clone())),
        PatternExpression::Bind { name, span, .. } => Some((name.as_str(), span.clone())),
        PatternExpression::Name { path, span } => path.parts.last().map(|name| (name.as_str(), span.clone())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transitional::cst::valkyrie::parser::AstParser;

    #[test]
    fn rejects_camel_case_let_micro_and_param() {
        let source = r#"micro demo(onClick: i32) -> i32 {
    let fooBar = 1;
    return onClick;
}
"#;
        let root = AstParser::parse_root(source).expect("fixture should parse");
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
        let root = AstParser::parse_vx_root(source).expect("widget fixture should parse");
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
        let root = AstParser::parse_root(source).expect("valid fixture should parse");
        let violations = validate_snake_case(&root);
        assert!(violations.is_empty(), "{violations:?}");
    }
}
