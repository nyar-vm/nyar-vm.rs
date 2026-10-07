use crate::{CompilerSourceGroup, ValkyrieCompiler};

use super::CompilerHostProviderBinding;
use nyar_types::{CanonicalCallee, CanonicalOperation, ItemInstanceId};

const TEST_ARCH: &str = "native";

fn invoke_callee(
    canonical: &nyar_types::CanonicalProgram,
    instance: ItemInstanceId,
) -> Option<CanonicalCallee> {
    canonical
        .mir
        .functions
        .get(&instance)?
        .blocks
        .values()
        .flat_map(|block| block.instructions.iter())
        .find_map(|instruction| match &instruction.operation {
            CanonicalOperation::Invoke { callee, .. } => Some(*callee),
            _ => None,
        })
}

#[test]
fn selected_host_provider_rewrites_contract_calls_in_full_source_closure() {
    let groups = [
        CompilerSourceGroup {
            dependency_key: "adaptor".into(),
            name: "adaptor".into(),
            source: "[host_provider(demo.write)] micro write(value: i32) -> i32 { return value }".into(),
            direct_dependencies: Vec::new(),
        },
        CompilerSourceGroup {
            dependency_key: "app".into(),
            name: "demo".into(),
            source: "[host_contract] micro write(value: i32) -> i32 { } \
                [main] micro main() -> i32 { return write(1) }"
                .into(),
            direct_dependencies: vec!["adaptor".into()],
        },
    ];
    let bindings = vec![CompilerHostProviderBinding {
        contract: "demo.write".into(),
        symbol: "adaptor.write".into(),
    }];
    let program = ValkyrieCompiler::default()
        .compile_source_groups_to_program_with_host_bindings(&groups, &bindings, TEST_ARCH)
        .expect("host provider binding must close across dependency groups");
    let canonical = program.canonical();
    assert_eq!(canonical.linked.imports.len(), 0, "contract import must be rewritten to provider callable");
    let main_instance = *canonical.linked.entries.keys().next().expect("main entry");
    let provider_instance = canonical
        .linked
        .callable_names
        .iter()
        .find(|(_, name)| name.to_string().contains("adaptor") && name.to_string().contains("write"))
        .map(|(instance, _)| *instance)
        .expect("provider callable");
    let call = invoke_callee(canonical, main_instance).expect("main must call host write");
    assert_eq!(call, CanonicalCallee::Item(provider_instance));
}

#[test]
fn missing_host_provider_binding_keeps_contract_import() {
    let groups = [
        CompilerSourceGroup {
            dependency_key: "adaptor".into(),
            name: "adaptor".into(),
            source: "[host_provider(demo.write)] micro write(value: i32) -> i32 { return value }".into(),
            direct_dependencies: Vec::new(),
        },
        CompilerSourceGroup {
            dependency_key: "app".into(),
            name: "demo".into(),
            source: "[host_contract] micro write(value: i32) -> i32 { } \
                [main] micro main() -> i32 { return write(1) }"
                .into(),
            direct_dependencies: vec!["adaptor".into()],
        },
    ];
    let program = ValkyrieCompiler::default()
        .compile_source_groups_to_program_with_host_bindings(&groups, &[], TEST_ARCH)
        .expect("unbound contract import remains a valid external contract");
    let canonical = program.canonical();
    assert_eq!(canonical.linked.imports.len(), 1);
}
