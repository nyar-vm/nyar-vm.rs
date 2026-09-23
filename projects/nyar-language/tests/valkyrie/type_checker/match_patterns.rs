use nyar_language::{
    MirOperand, MirOperation, MirTerminator, ValkyrieCompiler,
    types::{
        SourceID,
        hir::{HirExprKind, HirExtractorPattern, HirLiteral, HirPattern, HirStatementKind},
    },
};

fn compile_source_to_legacy_lir(
    compiler: &ValkyrieCompiler,
    source: &str,
) -> Result<nyar_language::lir::LirModule, std_data::text::valkyrie::ParseError> {
    let hir = compiler.compile_source(source)?;
    let mir = nyar_language::MirLowerer::lower_module(&hir);
    let lir = nyar_language::lir::LirLowerer::lower_mir_module(&hir, &mir);
    nyar_language::lir::validation::validate_module(&lir)?;
    Ok(lir)
}

#[test]
fn lowers_literal_variable_and_or_match_patterns_into_hir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 2973 });
    let hir = compiler
        .compile_source(
            r#"micro main(value: i64) -> bool {
    return match value {
        case 1 | 2:
            true
        case n if n > 0:
            false
        case _:
            false
    };
}
"#,
        )
        .unwrap();

    let HirStatementKind::Expr(statement) = &hir.functions[0].body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::Match { arms, .. } = &expression.kind
    else {
        panic!("expected match expression");
    };

    assert!(matches!(
        &arms[0].pattern,
        HirPattern::Or(patterns)
            if matches!(patterns.as_slice(), [HirPattern::Literal(HirLiteral::Integer64(1)), HirPattern::Literal(HirLiteral::Integer64(2))])
    ));
    assert!(matches!(&arms[1].pattern, HirPattern::Variable(identifier) if identifier.name.as_str() == "n"));
    assert!(arms[1].guard.is_some());
    assert!(matches!(&arms[2].pattern, HirPattern::Wildcard));
}

#[test]
fn lowers_tuple_match_patterns_into_hir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 2974 });
    let hir = compiler
        .compile_source(
            r#"micro main(pair: ((i64, i64), i64)) -> bool {
    return match pair {
        case ((x, y), z) if z > 0:
            true
        case (_, 0):
            false
        else:
            false
    };
}
"#,
        )
        .unwrap();

    let HirStatementKind::Expr(statement) = &hir.functions[0].body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::Match { arms, .. } = &expression.kind
    else {
        panic!("expected match expression");
    };

    assert!(matches!(
        &arms[0].pattern,
        HirPattern::Tuple(items)
            if matches!(
                items.as_slice(),
                [
                    HirPattern::Tuple(inner),
                    HirPattern::Variable(last)
                ]
                if matches!(
                    inner.as_slice(),
                    [
                        HirPattern::Variable(first),
                        HirPattern::Variable(second)
                    ]
                    if first.name.as_str() == "x" && second.name.as_str() == "y"
                ) && last.name.as_str() == "z"
            )
    ));
    assert!(arms[0].guard.is_some());
    assert!(matches!(
        &arms[1].pattern,
        HirPattern::Tuple(items)
            if matches!(
                items.as_slice(),
                [
                    HirPattern::Wildcard,
                    HirPattern::Literal(HirLiteral::Integer64(0))
                ]
            )
    ));
}

#[test]
fn lowers_nested_constructor_and_object_match_patterns_into_hir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 2975 });
    let hir = compiler
        .compile_source(
            r#"unite Option {
    Some { value: i64 }
    None {}
}

class Pair {
    micro extractor(self) -> (i64, i64)? {
        return null;
    }
}

class Wrapper {
    inner: Option;
    fallback: Pair;
    micro extractor(self) -> (Option, Pair)? {
        return null;
    }
}

micro main(value: Wrapper) -> bool {
    return match value {
        case Wrapper(inner, Pair(left, right)):
            true
        case Wrapper { inner: Some(result), fallback }:
            false
        else:
            false
    };
}
"#,
        )
        .unwrap();

    let HirStatementKind::Expr(statement) = &hir.functions[0].body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::Match { arms, .. } = &expression.kind
    else {
        panic!("expected match expression");
    };

    assert!(matches!(
        &arms[0].pattern,
        HirPattern::Extractor(HirExtractorPattern::Constructor { name, fields, .. })
            if name.to_string() == "Wrapper"
                && matches!(
                    fields.as_slice(),
                    [
                        HirPattern::Variable(inner),
                        HirPattern::Extractor(HirExtractorPattern::Constructor { name: nested_name, fields: nested_fields, .. })
                    ]
                    if inner.name.as_str() == "inner"
                        && nested_name.to_string() == "Pair"
                        && matches!(
                            nested_fields.as_slice(),
                            [
                                HirPattern::Variable(left),
                                HirPattern::Variable(right)
                            ] if left.name.as_str() == "left" && right.name.as_str() == "right"
                        )
                )
    ));
    assert!(matches!(
        &arms[1].pattern,
        HirPattern::Object { name: Some(name), fields, rest: None }
            if name.to_string() == "Wrapper"
                && fields.len() == 2
                && matches!(
                    &fields[0],
                    (field_name, HirPattern::Extractor(HirExtractorPattern::Constructor { name, fields, .. }))
                        if field_name.as_str() == "inner"
                            && name.to_string() == "Some"
                            && matches!(fields.as_slice(), [HirPattern::Variable(result)] if result.name.as_str() == "result")
                )
                && matches!(
                    &fields[1],
                    (field_name, HirPattern::Variable(fallback))
                        if field_name.as_str() == "fallback" && fallback.name.as_str() == "fallback"
                )
    ));
}

#[test]
fn lowers_nested_constructor_and_object_match_patterns_into_mir_and_lir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 2976 });
    let source = r#"unite Option {
    Some { value: i64 }
    None {}
}

class Pair {
    micro extractor(self) -> (i64, i64)? {
        return null;
    }
}

class Wrapper {
    inner: Option;
    fallback: Pair;
    micro extractor(self) -> (Option, Pair)? {
        return null;
    }
}

micro main(value: Wrapper) -> bool {
    return match value {
        case Wrapper(inner, Pair(left, right)):
            true
        case Wrapper { inner: Some(result), fallback }:
            false
        else:
            false
    };
}
"#;

    let mir = compiler.compile_source_to_mir(source).unwrap();
    let chain = mir.functions[0].case_chains.first().expect("expected a match case chain to be lowered");
    assert!(chain.produce_value);
    assert_eq!(chain.arms.len(), 3);
    assert!(mir.functions[0].blocks.iter().any(|block| matches!(block.terminator, MirTerminator::Branch { .. })));

    let lir = compile_source_to_legacy_lir(&compiler, source).unwrap();
    assert!(!lir.functions[0].blocks.is_empty());
}

#[test]
fn lowers_range_array_rest_and_typed_bind_patterns_into_hir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 2977 });
    let hir = compiler
        .compile_source(
            r#"micro main(value: Vector) -> bool {
    return match value {
        case [head, ..tail, last]:
            true
        case numbers as Vector:
            false
        case 1..=10 if true:
            false
        else:
            false
    };
}
"#,
        )
        .unwrap();

    let HirStatementKind::Expr(statement) = &hir.functions[0].body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::Match { arms, .. } = &expression.kind
    else {
        panic!("expected match expression");
    };

    assert!(matches!(
        &arms[0].pattern,
        HirPattern::Extractor(HirExtractorPattern::Array { prefix, rest: Some(rest), suffix, .. })
            if matches!(prefix.as_slice(), [HirPattern::Variable(head)] if head.name.as_str() == "head")
                && rest.name.as_str() == "tail"
                && matches!(suffix.as_slice(), [HirPattern::Variable(last)] if last.name.as_str() == "last")
    ));
    assert!(matches!(
        &arms[1].pattern,
        HirPattern::TypedBind { identifier, ty }
            if identifier.name.as_str() == "numbers" && ty.to_string() == "Vector"
    ));
    assert!(matches!(
        &arms[2].pattern,
        HirPattern::Range { start: Some(HirLiteral::Integer64(1)), end: Some(HirLiteral::Integer64(10)), inclusive_end: true }
    ));
    assert!(arms[2].guard.is_some());
}

#[test]
fn lowers_qualified_and_bare_name_patterns_without_confusing_variable_binding() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 2978 });
    let hir = compiler
        .compile_source(
            r#"micro main(value: Payload) -> bool {
    return match value {
        case package::module::Unite::Variant:
            true
        case Variant:
            false
        case var:
            false
    };
}
"#,
        )
        .unwrap();

    let HirStatementKind::Expr(statement) = &hir.functions[0].body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::Match { arms, .. } = &expression.kind
    else {
        panic!("expected match expression");
    };

    assert!(matches!(
        &arms[0].pattern,
        HirPattern::Name(path)
            if path.parts().iter().map(|part| part.as_str()).eq(["package", "module", "Unite", "Variant"].into_iter())
    ));
    assert!(matches!(
        &arms[1].pattern,
        HirPattern::Name(path) if path.parts().iter().map(|part| part.as_str()).eq(["Variant"].into_iter())
    ));
    assert!(matches!(
        &arms[2].pattern,
        HirPattern::Variable(identifier) if identifier.name.as_str() == "var"
    ));
}

#[test]
fn resolves_single_segment_name_pattern_into_type_when_scrutinee_type_matches() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 2979 });
    let hir = compiler
        .compile_source(
            r#"micro main(value: Payload) -> bool {
    return match value {
        case Payload:
            true
        case Variant:
            false
        else:
            false
    };
}
"#,
        )
        .unwrap();

    let HirStatementKind::Expr(statement) = &hir.functions[0].body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::Match { arms, .. } = &expression.kind
    else {
        panic!("expected match expression");
    };

    assert!(matches!(
        &arms[0].pattern,
        HirPattern::Type(path) if path.parts().iter().map(|part| part.as_str()).eq(["Payload"].into_iter())
    ));
    assert!(matches!(
        &arms[1].pattern,
        HirPattern::Name(path) if path.parts().iter().map(|part| part.as_str()).eq(["Variant"].into_iter())
    ));
}

#[test]
fn accepts_qualified_lowercase_name_patterns_at_hir_validation() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 2980 });
    let hir = compiler
        .compile_source(
            r#"micro main(value: Payload) -> bool {
    return match value {
        case package::module::value:
            true
        else:
            false
    };
}
"#,
        )
        .unwrap();

    let HirStatementKind::Expr(statement) = &hir.functions[0].body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::Match { arms, .. } = &expression.kind
    else {
        panic!("expected match expression");
    };

    assert!(matches!(
        &arms[0].pattern,
        HirPattern::Name(path) if path.parts().iter().map(|part| part.as_str()).eq(["package", "module", "value"].into_iter())
    ));
}

#[test]
fn lowers_while_let_into_hir_loop_with_pattern_probe() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 2981 });
    let hir = compiler
        .compile_source(
            r#"unite Option {
    Some { value: i64 }
    None {}
}

micro main(opt: Option) -> i64 {
    let mut sum = 0
    while let Some(x) = opt {
        sum = sum + x
    }
    return sum
}
"#,
        )
        .unwrap();

    let loop_expr = hir.functions[0]
        .body
        .statements
        .iter()
        .find_map(|statement| match &statement.kind {
            HirStatementKind::Expr(expr) if matches!(expr.kind, HirExprKind::Loop { .. }) => Some(expr),
            _ => None,
        })
        .expect("expected while let loop statement");

    assert!(matches!(&loop_expr.kind, HirExprKind::Loop { pattern: Some(_), iterator: Some(_), .. }));
}

#[test]
fn lowers_while_let_mir_emits_header_reprobe_blocks() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 2982 });
    let source = r#"unite Option {
    Some { value: i64 }
    None {}
}

micro main(opt: Option) -> i64 {
    let mut sum = 0
    while let Some(x) = opt {
        sum = sum + x
    }
    return sum
}
"#;

    let mir = compiler.compile_source_to_mir(source).unwrap();
    let header_block = mir.functions[0].blocks.iter().find(|block| block.label == "loop_header").expect("expected loop_header block");

    assert!(matches!(header_block.terminator, MirTerminator::Branch { .. }));
}

#[test]
fn lowers_until_not_into_hir_loop_with_pattern() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 2983 });
    let hir = compiler
        .compile_source(
            r#"unite Option {
    Some { value: i64 }
    None {}
}

micro main(opt: Option) -> i64 {
    let mut sum = 0
    until not Some(x) = opt {
        sum = sum + x
    }
    return sum
}
"#,
        )
        .unwrap();

    let loop_expr = hir.functions[0]
        .body
        .statements
        .iter()
        .find_map(|statement| match &statement.kind {
            HirStatementKind::Expr(expr) if matches!(expr.kind, HirExprKind::Loop { .. }) => Some(expr),
            _ => None,
        })
        .expect("expected until not loop statement");

    assert!(matches!(&loop_expr.kind, HirExprKind::Loop { pattern: Some(_), iterator: Some(_), .. }));
}

#[test]
fn lowers_case_if_guard_into_mir_three_block_chain() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 2984 });
    let source = r#"micro main(value: i64) -> i64 {
    case value {
        case 1 if value > 0:
            return value
        else:
            return 0
    }
    return 0
}
"#;

    let mir = compiler.compile_source_to_mir(source).unwrap();
    let check_block = mir.functions[0].blocks.iter().find(|block| block.label == "case_arm_0_check").expect("expected case_arm_0_check block");
    assert!(matches!(check_block.terminator, MirTerminator::Branch { .. }));

    assert!(mir.functions[0].blocks.iter().any(|block| block.label == "case_arm_0_body"), "expected case_arm_0_body block");
}

#[test]
fn while_let_mir_header_reprobe_calls_extractor_once() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 2985 });
    let source = r#"unite Option {
    Some { value: i64 }
    None {}
}

micro main(opt: Option) -> i64 {
    let mut sum = 0
    while let Some(x) = opt {
        sum = sum + x
    }
    return sum
}
"#;

    let mir = compiler.compile_source_to_mir(source).unwrap();
    let extractor_call_count = mir.functions[0]
        .blocks
        .iter()
        .flat_map(|block| block.instructions.iter())
        .filter(|instruction| {
            matches!(
                            &instruction.kind,
                            MirInstructionKind::Call {                    callee: MirOperand::Symbol(path),
                                ..,
            } if path.parts().last().is_some_and(|part| part.as_str() == "extractor")
                        )
        })
        .count();

    assert!(extractor_call_count <= 2, "extractor should be called at most once per loop probe, got {extractor_call_count}");
}

#[test]
fn larrow_bind_syntax_parses_into_hir_bind_pattern() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 2990 });
    let hir = compiler
        .compile_source(
            r#"unite Option {
    Some { value: i64 }
    None {}
}

micro main(opt: Option) -> i64 {
    return match opt {
        case whole <- Some(value):
            whole.value
        else:
            0
    };
}
"#,
        )
        .unwrap();

    let HirStatementKind::Expr(statement) = &hir.functions[0].body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::Match { arms, .. } = &expression.kind
    else {
        panic!("expected match expression");
    };

    assert!(matches!(
        &arms[0].pattern,
        HirPattern::Bind { identifier, pattern, .. }
            if identifier.name.as_str() == "whole"
                && matches!(pattern.as_ref(), HirPattern::Extractor(HirExtractorPattern::Constructor { name, .. }) if name.to_string() == "Some")
    ));
}

#[test]
fn extractor_field_rename_syntax_parses_into_bind() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 2991 });
    let hir = compiler
        .compile_source(
            r#"unite Option {
    Some { value: i64 }
    None {}
}

class Wrapper {
    inner: Option;
    micro extractor(self) -> (Option)? {
        return null;
    }
}

micro main(value: Wrapper) -> i64 {
    return match value {
        case Wrapper(inner: Some(result)):
            result
        else:
            0
    };
}
"#,
        )
        .unwrap();

    let HirStatementKind::Expr(statement) = &hir.functions[0].body.statements[0].kind
    else {
        panic!("expected return statement");
    };
    let HirExprKind::Return(Some(expression)) = &statement.kind
    else {
        panic!("expected return expression");
    };
    let HirExprKind::Match { arms, .. } = &expression.kind
    else {
        panic!("expected match expression");
    };

    assert!(matches!(
        &arms[0].pattern,
        HirPattern::Extractor(HirExtractorPattern::Constructor { name, fields, .. })
            if name.to_string() == "Wrapper"
                && matches!(
                    fields.as_slice(),
                    [HirPattern::Bind { identifier, pattern, .. }]
                        if identifier.name.as_str() == "inner"
                            && matches!(pattern.as_ref(), HirPattern::Extractor(HirExtractorPattern::Constructor { name: nested_name, .. }) if nested_name.to_string() == "Some")
                )
    ));
}
