use rusty_valkyrie::{frontend, *};

#[test]
fn source_mir_analysis_rejects_the_same_unresolved_call_as_hir() {
    let compiler = ValkyrieCompiler::default();
    let source = "micro caller() -> i32 { return missing() }";
    let hir_error = compiler.compile_source(source).expect_err("HIR 必须拒绝缺失的调用身份");
    let mir_error = compiler.compile_source_to_mir(source).expect_err("分析入口不得跳过 HIR 调用合同");
    assert!(hir_error.to_string().contains("SMIR003"), "{hir_error}");
    assert_eq!(hir_error.to_string(), mir_error.to_string());
}

#[test]
fn ast_mir_analysis_rejects_unresolved_calls_before_lowering() {
    let root = frontend::parse_source("micro caller() -> i32 { return missing() }").expect("Oak 应解析测试源码");
    let error = ValkyrieCompiler::default().lower_root_to_mir(&root).expect_err("AST 分析入口必须执行 HIR 合同");
    assert!(error.to_string().contains("SMIR003"), "{error}");
}

#[test]
fn source_mir_analysis_preserves_resolved_function_signatures() {
    let compiler = ValkyrieCompiler::default();
    let source = "micro identity(value: i32) -> i32 { return value } micro caller(value: i32) -> i32 { return identity(value) }";
    let hir = compiler.compile_source(source).expect("正式 HIR 调用解析");
    let mir = compiler.compile_source_to_mir(source).expect("相同源码的 MIR 分析");
    assert_eq!(mir.functions.len(), hir.functions.len());
    for (mir_function, hir_function) in mir.functions.iter().zip(&hir.functions) {
        assert_eq!(mir_function.param_types, hir_function.params.iter().map(|parameter| parameter.ty.clone()).collect::<Vec<_>>());
        assert_eq!(mir_function.return_type, hir_function.return_type);
    }
}
