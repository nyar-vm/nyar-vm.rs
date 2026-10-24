use nyar_language::{CompilerSourceGroup, ValkyrieCompiler};

use nyar_language::valkyrie::{compile_pipeline::CompilerHostProviderBinding, semantic_closure::micro_declaration_key};
use nyar_types::{CanonicalCallee, CanonicalOperation, ItemInstanceId};

const TEST_ARCH: &str = "native";

/// 组模式下 `i32` 必须由依赖组声明为 primitive 结构；否则残留 `Named("i32")`
/// 会以 CAN003 拒绝进入 canonical 类型表。
const CORE_PRIMITIVE_SOURCE: &str = "namespace core::primitive; [primitive(\"core::primitive::i32\")] structure i32 { }";

fn core_group() -> CompilerSourceGroup {
    CompilerSourceGroup::single_source("core", "core", CORE_PRIMITIVE_SOURCE)
}

fn user_linked_import_count(canonical: &nyar_types::CanonicalProgram) -> usize {
    canonical
        .linked
        .imports
        .values()
        .filter(|import| {
            let label = import.capability.to_string();
            !label.ends_with("::begin_phase") && !label.ends_with("::end_phase")
        })
        .count()
}

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
        core_group(),
        CompilerSourceGroup::single_source(
            "adaptor",
            "adaptor",
            "[host_provider(demo::write)] micro write(value: i32) -> i32 { return value }",
        )
        .with_direct_dependencies(vec!["core".into()]),
        CompilerSourceGroup::single_source(
            "app",
            "demo",
            "[host_contract] micro write(value: i32) -> i32 { } \
                [main] micro main() -> i32 { return write(1) }",
        )
        .as_entry()
        .with_direct_dependencies(vec!["core".into(), "adaptor".into()]),
    ];
    let bindings = vec![CompilerHostProviderBinding {
        contract: micro_declaration_key("app", "demo.v", &[], "write"),
        provider: micro_declaration_key("adaptor", "adaptor.v", &[], "write"),
    }];
    let program = ValkyrieCompiler::default()
        .compile_source_groups_to_program_with_host_bindings(&groups, &bindings, TEST_ARCH)
        .expect("host provider binding must close across dependency groups");
    let canonical = program.canonical();
    assert_eq!(user_linked_import_count(canonical), 0, "contract import must be rewritten to provider callable");
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
fn bodyless_target_bound_provider_carries_its_own_import_contract() {
    let groups = [
        core_group(),
        CompilerSourceGroup::single_source(
            "adaptor",
            "adaptor",
            "[host_provider(demo::add)] [wasm_operator(\"i32.add\")] micro i32_add(a: i32, b: i32): i32",
        )
        .with_direct_dependencies(vec!["core".into()]),
        CompilerSourceGroup::single_source(
            "app",
            "demo",
            "[host_contract] micro add(a: i32, b: i32): i32 [main] micro main() -> i32 { return add(1, 2) }",
        )
        .as_entry()
        .with_direct_dependencies(vec!["core".into(), "adaptor".into()]),
    ];
    let bindings = vec![CompilerHostProviderBinding {
        contract: micro_declaration_key("app", "demo.v", &[], "add"),
        provider: micro_declaration_key("adaptor", "adaptor.v", &[], "i32_add"),
    }];
    let program = ValkyrieCompiler::default()
        .compile_source_groups_to_program_with_host_bindings(&groups, &bindings, TEST_ARCH)
        .expect("无体目标绑定 provider 必须闭合绑定链");
    let canonical = program.canonical();
    let imports = canonical
        .linked
        .imports
        .values()
        .filter(|import| {
            let label = import.capability.to_string();
            !label.ends_with("::begin_phase") && !label.ends_with("::end_phase")
        })
        .collect::<Vec<_>>();
    assert_eq!(imports.len(), 1, "provider 的目标绑定必须取代原合同导入");
    assert!(imports[0].link.matches_platform_tag("wasm-operator"), "link={:?}", imports[0].link);
    assert_eq!(imports[0].link.locator_segments(), &["i32.add"]);
    let main_instance = *canonical.linked.entries.keys().next().expect("main entry");
    let call = invoke_callee(canonical, main_instance).expect("main must call add");
    assert_eq!(call, CanonicalCallee::Item(imports[0].callee));
}

#[test]
fn missing_host_provider_binding_keeps_contract_import() {
    let groups = [
        core_group(),
        CompilerSourceGroup::single_source(
            "adaptor",
            "adaptor",
            "[host_provider(demo::write)] micro write(value: i32) -> i32 { return value }",
        )
        .with_direct_dependencies(vec!["core".into()]),
        CompilerSourceGroup::single_source(
            "app",
            "demo",
            "[host_contract] micro write(value: i32) -> i32 { } \
                [main] micro main() -> i32 { return write(1) }",
        )
        .as_entry()
        .with_direct_dependencies(vec!["core".into(), "adaptor".into()]),
    ];
    let program = ValkyrieCompiler::default()
        .compile_source_groups_to_program_with_host_bindings(&groups, &[], TEST_ARCH)
        .expect("unbound contract import remains a valid external contract");
    let canonical = program.canonical();
    assert_eq!(user_linked_import_count(canonical), 1);
}
