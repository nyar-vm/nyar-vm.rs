use rusty_valkyrie::parser::{frontend, parse};

#[test]
fn parses_minimal_micro_with_oak() {
    let output = parse("micro main() -> i32 { return 1 }");
    let root = output.result.expect("Oak 应构造 Valkyrie AST");
    assert!(!root.items.is_empty());
}

#[test]
fn preserves_call_and_numeric_expression_structure() {
    let output = parse("micro main() -> i32 { let value = add(1, 2); return value }");
    let root = output.result.expect("Oak 应构造调用 AST");
    let frontend::ast::StatementNode::Micro(micro) = &root.items[0]
    else {
        panic!("缺少 micro 声明");
    };
    let frontend::ast::Statement::Let(binding) = &micro.body.statements[0]
    else {
        panic!("缺少 let 绑定");
    };
    let Some(frontend::ast::TermExpression::ApplyCall { callee, args, .. }) = binding.expr.as_ref()
    else {
        panic!("调用没有保留为 Oak ApplyCall");
    };
    assert!(matches!(callee.as_ref(), frontend::ast::TermExpression::NamePath(_)));
    assert!(matches!(args[0], frontend::ast::TermExpression::IntegerLiteral { .. }));
    assert!(matches!(args[1], frontend::ast::TermExpression::IntegerLiteral { .. }));
}

#[test]
fn parses_tls_style_uninitialized_let_with_branch_assignments() {
    let source = r#"micro wrap_tls(config: i32) -> i32 {
    let handle: i32;
    if config > 0 {
        handle = 1
    } else {
        handle = 2
    }
    return handle
}"#;
    let output = parse(source);
    output.result.expect("tls 风格的未初始化 let + 分支赋值应可解析");
}

#[test]
fn parses_uninitialized_typed_let_binding() {
    let output = parse("micro main() -> i32 { let handle: i32; handle = 1; return handle }");
    let root = output.result.expect("Oak 应接受类型注解且无初始化表达式的 let");
    let frontend::ast::StatementNode::Micro(micro) = &root.items[0]
    else {
        panic!("缺少 micro 声明");
    };
    let frontend::ast::Statement::Let(binding) = &micro.body.statements[0]
    else {
        panic!("缺少 let 绑定");
    };
    assert!(binding.expr.is_none());
    assert!(binding.ty.is_some());
}

#[test]
fn parses_nullable_suffix_return_type() {
    let output = parse("micro parse_i32(s: utf8) -> i64? { return null }");
    output.result.expect("后缀 `?` 可空返回类型应可解析");
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn parses_generic_and_structure_declarations() {
    let output = parse("class Box[T] { value: T } micro identity[T](value: T) -> T { return value }");
    let root = output.result.expect("Oak 应构造泛型和结构 AST");
    assert!(root.items.iter().any(|item| matches!(item, frontend::ast::StatementNode::Class(_))));
    assert!(root.items.iter().any(|item| matches!(item, frontend::ast::StatementNode::Micro(_))));
}
