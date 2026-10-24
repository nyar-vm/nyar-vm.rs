use rusty_valkyrie::frontend::{
    ast::{Statement, StatementNode, TemplateNode},
    parse_source,
};

use rusty_valkyrie::hir::lowering::expand_templates_in_root;

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

    let StatementNode::Micro(micro) = &root.items[0]
    else {
        panic!("expected micro")
    };

    assert_eq!(micro.body.statements.len(), 1, "micro body should contain one template statement");

    let Statement::Template(template) = &micro.body.statements[0]
    else {
        panic!("expected template statement")
    };

    let TemplateNode::Match(match_node) = template.as_ref()
    else {
        panic!("expected match template")
    };

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

    let StatementNode::Micro(micro) = &root.items[0]
    else {
        panic!("expected micro")
    };

    assert!(!micro.body.statements.is_empty());

    assert!(!matches!(micro.body.statements[0], Statement::Template(_)));
}

#[test]

fn expands_arch_if_predicate() {
    let source = r#"micro main() -> i32 {

<% if arch == "wasm32" %>

return 42

<% else %>

return 0

<% end %>

}"#;

    let mut root = parse_source(source).expect("parse");

    expand_templates_in_root(&mut root, source, "wasm32").expect("expand");

    let StatementNode::Micro(micro) = &root.items[0]
    else {
        panic!("expected micro")
    };

    assert_eq!(micro.body.statements.len(), 1);

    let mut root_native = parse_source(source).expect("parse");

    expand_templates_in_root(&mut root_native, source, "native").expect("expand");

    let StatementNode::Micro(micro_native) = &root_native.items[0]
    else {
        panic!("expected micro")
    };

    assert_eq!(micro_native.body.statements.len(), 1);
}

#[test]
fn expands_const_loop_unroll() {
    let source = r#"micro main() -> i32 {
<% loop _ in 0..2 %>
return 1
<% end %>
}"#;
    let mut root = parse_source(source).expect("parse");
    expand_templates_in_root(&mut root, source, "native").expect("expand");
    let StatementNode::Micro(micro) = &root.items[0]
    else {
        panic!("expected micro")
    };
    assert_eq!(micro.body.statements.len(), 2, "loop 0..2 should unroll two `return` statements");
    assert!(micro.body.statements.iter().all(|stmt| !matches!(stmt, Statement::Template(_))));
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn expands_fragment_arch_as_string_literal() {
    let source = r#"micro main() -> i32 {
<% arch %>
}"#;
    let mut root = parse_source(source).expect("parse");
    expand_templates_in_root(&mut root, source, "wasm32").expect("expand");
    let StatementNode::Micro(micro) = &root.items[0]
    else {
        panic!("expected micro")
    };
    assert_eq!(micro.body.statements.len(), 1);
    assert!(matches!(micro.body.statements[0], Statement::Expression(_)));
}

#[test]
fn expands_nested_if_inside_const_loop() {
    let source = r#"micro main() -> i32 {
<% loop _ in 0..1 %>
<% if arch == "wasm32" %>
return 7
<% end %>
<% end %>
}"#;
    let mut root = parse_source(source).expect("parse");
    expand_templates_in_root(&mut root, source, "wasm32").expect("expand");
    let StatementNode::Micro(micro) = &root.items[0]
    else {
        panic!("expected micro")
    };
    assert_eq!(micro.body.statements.len(), 1);
    assert!(!matches!(micro.body.statements[0], Statement::Template(_)));
}

#[test]
fn expands_arch_if_else_if_predicate() {
    let source = r#"micro main() -> i32 {
<% if arch == "wasm32" %>
return 1
<% else if arch == "native" %>
return 2
<% else %>
return 3
<% end %>
}"#;
    let mut root = parse_source(source).expect("parse");
    expand_templates_in_root(&mut root, source, "native").expect("expand");
    let StatementNode::Micro(micro) = &root.items[0]
    else {
        panic!("expected micro")
    };
    assert_eq!(micro.body.statements.len(), 1);
}

#[test]
fn expands_false_if_to_empty_body() {
    let source = r#"micro main() -> i32 {
<% if arch == "wasm32" %>
return 1
<% end %>
}"#;
    let mut root = parse_source(source).expect("parse");
    expand_templates_in_root(&mut root, source, "native").expect("expand");
    let StatementNode::Micro(micro) = &root.items[0]
    else {
        panic!("expected micro")
    };
    assert!(micro.body.statements.is_empty());
}
