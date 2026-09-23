use nyar_language::{
    ValkyrieCompiler,
    types::{SourceID, hir::HirParameterBindingKind},
};

#[test]
fn param_binding_lowers_lt_gt_markers() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 9002 });
    let hir = compiler
        .compile_source(
            r#"
micro f(a, <, b, >, c) {
    c
}
"#,
        )
        .expect("compile");
    let params = &hir.functions[0].params;
    assert_eq!(params[0].binding_kind, HirParameterBindingKind::PositionalOnly);
    assert_eq!(params[1].binding_kind, HirParameterBindingKind::PositionalOrKeyword);
    assert_eq!(params[2].binding_kind, HirParameterBindingKind::KeywordOnly);
}
