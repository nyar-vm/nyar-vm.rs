use nyar_language::{CompilerSourceGroup, ValkyrieCompiler};

use nyar_language::valkyrie::{compile_pipeline::CompilerHostProviderBinding, semantic_closure::micro_declaration_key};
use nyar_types::{CanonicalCallee, CanonicalOperation, ItemInstanceId};

const TEST_ARCH: &str = "native";

fn invoke_callee(canonical: &nyar_types::CanonicalProgram, instance: ItemInstanceId) -> Option<CanonicalCallee> {
    canonical.mir.functions.get(&instance)?.blocks.values().flat_map(|block| block.instructions.iter()).find_map(|instruction| {
        match &instruction.operation {
            CanonicalOperation::Invoke { callee, .. } => Some(*callee),
            _ => None,
        }
    })
}

#[test]
fn selected_host_provider_rewrites_contract_calls_in_full_source_closure() {
    let groups = [
        CompilerSourceGroup::single_source(
            "adaptor",
            "adaptor",
            "[host_provider(demo::write)] micro write(value: i32) -> i32 { return value }",
        ),
        CompilerSourceGroup::single_source(
            "app",
            "demo",
            "[host_contract] micro write(value: i32) -> i32 { } \
                [main] micro main() -> i32 { return write(1) }",
        )
        .as_entry()
        .with_direct_dependencies(vec!["adaptor".into()]),
    ];
    let bindings = vec![CompilerHostProviderBinding {
        contract: micro_declaration_key("app", "demo.v", &[], "write"),
        provider: micro_declaration_key("adaptor", "adaptor.v", &[], "write"),
    }];
    let program = ValkyrieCompiler::default()
        .compile_source_groups_to_program_with_host_bindings(&groups, &bindings, TEST_ARCH)
        .expect("host provider binding must close across dependency groups");
    let canonical = program.canonical();
    assert_eq!(canonical.linked.imports.len(), 0, "contract import must be rewritten to provider callable");
    let main_instance = *canonical.linked.entries.keys().next().expect("main entry");
    let call = invoke_callee(canonical, main_instance).expect("main must call host write");
    let provider_instance = match call {
        CanonicalCallee::Item(instance) => instance,
        other => panic!("host binding must rewrite to local provider callable, got {other:?}"),
    };
    assert_ne!(provider_instance, main_instance);
    assert!(canonical.mir.functions.contains_key(&provider_instance));
}

#[test]
fn missing_host_provider_binding_keeps_contract_import() {
    let groups = [
        CompilerSourceGroup::single_source(
            "adaptor",
            "adaptor",
            "[host_provider(demo::write)] micro write(value: i32) -> i32 { return value }",
        ),
        CompilerSourceGroup::single_source(
            "app",
            "demo",
            "[host_contract] micro write(value: i32) -> i32 { } \
                [main] micro main() -> i32 { return write(1) }",
        )
        .as_entry()
        .with_direct_dependencies(vec!["adaptor".into()]),
    ];
    let program = ValkyrieCompiler::default()
        .compile_source_groups_to_program_with_host_bindings(&groups, &[], TEST_ARCH)
        .expect("unbound contract import remains a valid external contract");
    let canonical = program.canonical();
    assert_eq!(canonical.linked.imports.len(), 1);
}
