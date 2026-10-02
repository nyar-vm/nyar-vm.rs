use crate::{ValkyrieCompiler, mir::MirLowerer};

#[test]
fn source_import_declaration_has_signature_but_no_fabricated_body() {
    let output = ValkyrieCompiler::default().compile_source_to_build_output(
        "[host_contract] micro foreign(value: i32) -> i32; \
         micro caller(value: i32) -> i32 { return foreign(value) }",
    ).expect("显式 import 与普通调用沿真实构建入口闭合");
    let mir = MirLowerer::lower_module_semantic(output.hir_module());
    assert_eq!(mir.functions.len(), 1);
    assert_eq!(mir.external_calls.len(), 1);
    assert!(mir.functions.iter().all(|function| function.symbol != mir.external_calls[0].symbol.to_string()));
    let canonical = output.canonical_program();
    assert_eq!(canonical.mir.functions.len(), 1);
    assert_eq!(canonical.linked.item_instances.len(), 2);
    assert_eq!(canonical.linked.imports.len(), 1);
    let import = canonical.linked.imports.values().next().unwrap();
    let signature = &canonical.linked.item_instances[&import.callee];
    assert_eq!(import.parameter_types, signature.parameter_types);
    assert_eq!(import.return_type, signature.return_type);
    assert!(!canonical.mir.functions.contains_key(&import.callee));
}

#[test]
fn source_bare_declaration_cannot_supply_an_executable_call_target() {
    let compiler = ValkyrieCompiler::default();
    let hir = compiler.compile_source("micro missing(value: i32) -> i32;")
        .expect("裸声明允许参与分析");
    let mir = MirLowerer::lower_module_semantic(&hir);
    assert!(mir.functions.is_empty());
    assert!(mir.external_calls.is_empty());
    assert!(mir.callable_identities.is_empty());
    compiler.compile_source_to_build_output(
        "micro missing(value: i32) -> i32; \
         micro caller(value: i32) -> i32 { return missing(value) }",
    ).expect_err("无定义且无显式 import 的调用不能成功");
}

#[test]
fn source_entry_declaration_cannot_become_an_empty_program_body() {
    ValkyrieCompiler::default().compile_source_to_build_output(
        "[main] micro entry() -> i32;",
    ).expect_err("入口必须有真实源码函数体，不能用声明补造");
}
