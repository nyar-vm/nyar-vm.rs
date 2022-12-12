use nyar_language::{
    MirOperand, MirOperation, MirTerminator, ValkyrieCompiler,
    types::{Identifier, hir::ValkyrieType},
};

#[test]
fn compile_return_zero_lowers_to_single_return_block() {
    let source = r#"namespace test;
micro main() -> i64 {
    return 0;
}"#;
    let compiler = ValkyrieCompiler::default();
    let mir = compiler.compile_source_to_mir(source).expect("parse ok");
    assert_eq!(mir.functions.len(), 1);
    let func = &mir.functions[0];
    assert_eq!(func.symbol, "test::main");
    assert_eq!(func.blocks.len(), 1);
    let block = &func.blocks[0];
    assert_eq!(block.label, "entry");
    assert!(matches!(block.terminator, MirTerminator::Return { .. }));
}

#[test]
fn array_literal_lowers_to_builtin_array_literal_without_array_call() {
    let source = r#"micro main(): i32 {
    let mut values: [i32] = [10, 20, 30]
    return values[1]
}
"#;
    let compiler = ValkyrieCompiler::default();

    let mir = compiler.compile_source_to_mir(source).expect("mir ok");
    let mir_operations = &mir.functions[0].blocks[0].instructions;
    assert!(mir_operations.iter().any(|instruction| matches!(instruction.kind, MirOperation::ArrayLiteral { .. })));
    assert!(mir_operations.iter().any(|instruction| {
        matches!(&instruction.kind, MirOperation::ArrayLiteral { element_type: ValkyrieType::Integer32 { signed: true }, .. })
    }));
    assert!(!mir_operations.iter().any(|instruction| {
        matches!(
            &instruction.kind,
            MirOperation::Call { callee: nyar_language::MirOperand::Symbol(path), .. } if path.to_string() == "array"
        )
    }));

    let build_output = compiler.compile_source_to_build_output(source).expect("build output ok");
    assert_eq!(build_output.hir_function_count(), 1);
    assert_eq!(build_output.neutral_plan().semantic_fragments.len(), 1);
}

#[test]
fn main_exit_code_shapes_compile() {
    let compiler = ValkyrieCompiler::default();

    let inferred = compiler.compile_source(
        r#"[main]
micro main() {
    return 0
}
"#,
    );
    assert!(inferred.is_err());

    let explicit = compiler.compile_source(
        r#"[main]
micro main(): ExitCode {
    return 0
}
"#,
    );
    assert!(explicit.is_err());

    let wrapped = compiler.compile_source(
        r#"[main]
micro main(): ExitCode {
    return ExitCode(0)
}
"#,
    );
    assert!(wrapped.is_ok());

    let wrapped_mir = compiler
        .compile_source_to_mir(
            r#"[main]
micro main(): ExitCode {
    return ExitCode(0)
}
"#,
        )
        .unwrap();
    assert_eq!(wrapped_mir.functions[0].return_type, ValkyrieType::Named(Identifier::new("ExitCode")));

    let wrapped_build_output = compiler
        .compile_source_to_build_output(
            r#"[main]
micro main(): ExitCode {
    return ExitCode(0)
}
"#,
        )
        .unwrap();
    assert_eq!(wrapped_build_output.hir_function_count(), 1);
}

#[test]
fn exit_code_in_expression_position_stays_value_semantics() {
    let compiler = ValkyrieCompiler::default();
    let mir = compiler
        .compile_source_to_mir(
            r#"[main]
micro main(): ExitCode {
    if ExitCode(0) == ExitCode(1) {
        return ExitCode(1)
    }
    return ExitCode(0)
}
"#,
        )
        .expect("mir lowering should accept ExitCode in expression position");

    let has_exit_code_call =
        mir.functions.iter().flat_map(|function| function.blocks.iter()).flat_map(|block| block.instructions.iter()).any(|instruction| {
            matches!(
                &instruction.kind,
                MirOperation::Call { callee: MirOperand::Symbol(path), .. }
                    if path.parts().last().is_some_and(|name| name.as_str() == "ExitCode")
            )
        });
    assert!(has_exit_code_call, "ExitCode(x) should lower as a normal constructor call, not an operator builtin hole.");
}
