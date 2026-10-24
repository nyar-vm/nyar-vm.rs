use rusty_valkyrie::hir::lowering::{eval_const_condition, parse_const_loop_range, parse_const_range_text};
use oak_core::Token;
use oak_valkyrie::lexer::{ValkyrieKeywords, token_type::ValkyrieTokenType};

fn tokens_from(source: &str, kinds: &[(ValkyrieTokenType, std::ops::Range<usize>)]) -> Vec<Token<ValkyrieTokenType>> {
    kinds.iter().map(|(kind, span)| Token { kind: *kind, span: oak_core::Range { start: span.start, end: span.end } }).collect()
}

#[test]
fn range_i64_iterates_twice_for_zero_to_two() {
    let range = parse_const_range_text("0..2").expect("range");
    assert_eq!(range, 0..2);
    assert_eq!(range.count(), 2);
}

#[test]
fn parse_loop_range_from_parsed_header() {
    use rusty_valkyrie::frontend::{ast::Statement, parse_source};
    use oak_valkyrie::ast::{StatementNode, TemplateNode};
    let source = r#"micro main() -> i32 {
<% loop _ in 0..2 %>
return 1
<% end %>
}"#;
    let root = parse_source(source).expect("parse");
    let StatementNode::Micro(micro) = &root.items[0]
    else {
        panic!("expected micro")
    };
    let Statement::Template(template) = &micro.body.statements[0]
    else {
        panic!("expected template")
    };
    let TemplateNode::Loop(loop_node) = template.as_ref()
    else {
        panic!("expected loop")
    };
    assert_eq!(parse_const_loop_range(source, &loop_node.header, "native").expect("range"), 0..2);
}

#[test]
fn parse_loop_range_from_compact_literal() {
    let source = "_ in 0..2";
    let tokens = tokens_from(
        source,
        &[
            (ValkyrieTokenType::Underscore, 0..1),
            (ValkyrieTokenType::Keyword(ValkyrieKeywords::In), 2..4),
            (ValkyrieTokenType::FloatLiteral, 5..9),
        ],
    );
    assert_eq!(parse_const_loop_range(source, &tokens, "native").expect("range"), 0..2);
}

#[test]
fn eval_arch_equality_predicate() {
    let source = r#"arch == "wasm32""#;
    let tokens = tokens_from(
        source,
        &[(ValkyrieTokenType::Identifier, 0..4), (ValkyrieTokenType::EqEq, 5..7), (ValkyrieTokenType::StringLiteral, 8..16)],
    );
    assert!(eval_const_condition(source, &tokens, "wasm32").expect("eval"));
    assert!(!eval_const_condition(source, &tokens, "native").expect("eval"));
}
