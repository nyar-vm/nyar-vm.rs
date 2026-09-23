use nyar::{CapabilityTag, Identifier};
use nyar_language::{
    SourceID, ValkyrieCompiler,
    valkyrie::frontend_contract::{NyarPlanningContract, hir_module_to_object_algebraic_program, hir_module_to_program_facts},
};

#[test]
fn suspend_functions_get_isolated_dimension_and_capability() {
    let compiler = ValkyrieCompiler::new(SourceID::default());
    let hir = compiler
        .compile_source(
            r#"namespace demo;

micro sync_fn() -> i32 { 1 }

micro gen() {
    yield 1
    return
}
"#,
        )
        .expect("hir");

    let facts = hir.program_facts();
    assert!(facts.capabilities.iter().any(|cap| cap.as_str() == "suspend"));
    assert!(facts.functions.iter().any(|function| function.can_suspend));

    let oa = hir_module_to_object_algebraic_program(&hir);
    let suspend = oa.dimensions.iter().find(|dimension| dimension.name == Identifier::new("suspend")).expect("suspend dimension");
    assert!(suspend.required_capabilities.contains(&CapabilityTag::new("suspend")));
    assert_eq!(suspend.exported_operations.len(), 1);
    assert!(oa.dimensions.iter().any(|dimension| dimension.name == Identifier::new("functions")));
}
