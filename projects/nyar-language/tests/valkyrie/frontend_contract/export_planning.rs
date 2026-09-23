use nyar_language::{
    SourceID, ValkyrieCompiler,
    valkyrie::frontend_contract::{hir_module_to_object_algebraic_program, hir_module_to_program_facts},
};

fn compile_module(source: &str) -> nyar_language::types::hir::HirModule {
    let compiler = ValkyrieCompiler::new(SourceID::default());
    compiler.compile_source(source).expect("compile export test source")
}

#[test]
fn export_attribute_populates_program_facts_with_partition() {
    let module = compile_module(
        r#"
namespace demo.export;

[export(unity.runtime)]
micro run_host(): i64 {
    return 0
}
"#,
    );

    let facts = hir_module_to_program_facts(&module);
    assert_eq!(facts.exports.len(), 1);
    assert_eq!(facts.exports[0].exported_name.as_str(), "run_host");
    assert_eq!(facts.exports[0].partition.as_deref(), Some("unity.runtime"));
}

#[test]
fn bare_export_uses_default_partition() {
    let module = compile_module(
        r#"
namespace demo.export;

[export]
micro api_ping(): i64 {
    return 0
}
"#,
    );

    let facts = hir_module_to_program_facts(&module);
    assert_eq!(facts.exports.len(), 1);
    assert_eq!(facts.exports[0].partition.as_deref(), Some("default"));
}

#[test]
fn non_exported_micro_is_not_in_exports() {
    let module = compile_module(
        r#"
namespace demo.export;

micro helper(): i64 {
    return 0
}
"#,
    );

    let facts = hir_module_to_program_facts(&module);
    assert!(facts.exports.is_empty());
}

#[test]
fn export_partitions_split_object_algebraic_dimensions() {
    let module = compile_module(
        r#"
namespace demo.export;

[export(unity.runtime)]
micro run_host(): i64 {
    return 0
}

[export(unity.editor)]
micro import_msil(): i64 {
    return 0
}
"#,
    );

    let program = hir_module_to_object_algebraic_program(&module);
    assert_eq!(program.dimensions.len(), 2);
    assert!(program.dimensions.iter().any(|dimension| dimension.name.as_str() == "export__unity_runtime"));
    assert!(program.dimensions.iter().any(|dimension| dimension.name.as_str() == "export__unity_editor"));
}
