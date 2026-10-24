use nyar::ExternalImportLink;
use rusty_valkyrie::{
    Identifier, NamePath,
    types::hir::ValkyrieType,
    {
        compile_pipeline::{ResolvedHostProviderBinding, apply_host_provider_bindings},
        mir::{
            AggregateLayoutPlan, MirBlock, MirBlockRef, MirExternalCallContract, MirFunction, MirInstruction, MirModule, MirOperand,
            MirOperation, MirTerminator, MirValueRef,
        },
    },
};
use nyar_types::{ItemId, ItemInstanceId};

use std::collections::BTreeMap;

fn provider_dependency_module(provider_instance: ItemInstanceId) -> MirModule {
    MirModule {
        name: "adaptor".into(),
        functions: vec![MirFunction {
            display_symbol: "std.adaptor.clr::write".into(),
            symbol: NamePath::new(vec![Identifier::new("adaptor"), Identifier::new("write")]),
            declaration: Some(ItemId::from_index(2).unwrap()),
            instance: Some(provider_instance),
            return_type: ValkyrieType::Integer32 { signed: true },
            param_types: vec![ValkyrieType::Integer32 { signed: true }],
            value_types: BTreeMap::new(),
            entry: MirBlockRef(0),
            values: Vec::new(),
            blocks: vec![MirBlock {
                id: MirBlockRef(0),
                label: "entry".into(),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: MirTerminator::Return { value: Some(MirOperand::Value(MirValueRef(0))) },
            }],
        }],
        structs: Vec::new(),
        imports: Vec::new(),
        external_calls: Vec::new(),
        exports: Vec::new(),
        entries: Vec::new(),
        callable_symbols: BTreeMap::from([(provider_instance, "std.adaptor.clr::write".to_owned())]),
        type_identities: BTreeMap::new(),
        aggregate_layouts: AggregateLayoutPlan::default(),
        sum_types: Vec::new(),
        flags_types: Vec::new(),
        singleton_instances: Vec::new(),
        semantic_fragments: Vec::new(),
        diagnostics: Vec::new(),
    }
}

fn module_with_contract_call(contract_instance: ItemInstanceId, provider_instance: ItemInstanceId) -> MirModule {
    MirModule {
        name: "demo".into(),
        functions: vec![MirFunction {
            display_symbol: "demo::main".into(),
            symbol: NamePath::new(vec![Identifier::new("demo"), Identifier::new("main")]),
            declaration: Some(ItemId::from_index(0).unwrap()),
            instance: Some(ItemInstanceId::from_index(0).unwrap()),
            return_type: ValkyrieType::Integer32 { signed: true },
            param_types: Vec::new(),
            value_types: BTreeMap::new(),
            entry: MirBlockRef(0),
            values: Vec::new(),
            blocks: vec![MirBlock {
                id: MirBlockRef(0),
                label: "entry".into(),
                parameters: Vec::new(),
                instructions: vec![MirInstruction::from_operation_with_results(
                    MirOperation::Call { callee: MirOperand::Callable(contract_instance), arguments: vec![MirOperand::Value(MirValueRef(0))] },
                    vec![],
                )],
                terminator: MirTerminator::Return { value: Some(MirOperand::Value(MirValueRef(0))) },
            }],
        }],
        structs: Vec::new(),
        imports: Vec::new(),
        external_calls: vec![MirExternalCallContract {
            declaration: Some(ItemId::from_index(1).unwrap()),
            instance: Some(contract_instance),
            symbol: NamePath::new(vec![Identifier::new("demo"), Identifier::new("write")]),
            link: ExternalImportLink::host(None, vec!["demo".to_owned(), "write".to_owned()]),
            parameter_types: vec![ValkyrieType::Integer32 { signed: true }],
            return_type: ValkyrieType::Integer32 { signed: true },
        }],
        exports: Vec::new(),
        entries: Vec::new(),
        callable_symbols: BTreeMap::from([
            (ItemInstanceId::from_index(0).unwrap(), "demo::main".to_owned()),
            (contract_instance, "demo::write".to_owned()),
            (provider_instance, "std.adaptor.clr::write".to_owned()),
        ]),
        type_identities: BTreeMap::new(),
        aggregate_layouts: AggregateLayoutPlan::default(),
        sum_types: Vec::new(),
        flags_types: Vec::new(),
        singleton_instances: Vec::new(),
        semantic_fragments: Vec::new(),
        diagnostics: Vec::new(),
    }
}

fn binding(contract_instance: ItemInstanceId, provider_instance: ItemInstanceId) -> ResolvedHostProviderBinding {
    ResolvedHostProviderBinding { contract_instance, provider_instance }
}

#[test]
fn selected_host_provider_rewrites_contract_calls_to_provider_instance() {
    let contract_instance = ItemInstanceId::from_index(1).unwrap();
    let provider_instance = ItemInstanceId::from_index(2).unwrap();
    let mut module = module_with_contract_call(contract_instance, provider_instance);
    let dependency = provider_dependency_module(provider_instance);

    apply_host_provider_bindings(&mut module, &[dependency], &[binding(contract_instance, provider_instance)])
        .expect("host provider binding must close adaptor contract");

    let call = module.functions[0].blocks[0].instructions[0].kind.clone();
    assert_eq!(
        call,
        MirOperation::Call { callee: MirOperand::Callable(provider_instance), arguments: vec![MirOperand::Value(MirValueRef(0))] }
    );
    assert!(module.external_calls.is_empty());
    assert!(!module.callable_symbols.contains_key(&contract_instance));
    assert!(module.callable_symbols.contains_key(&provider_instance));
}

#[test]
fn unreferenced_host_contract_selection_is_ignored() {
    let contract_instance = ItemInstanceId::from_index(1).unwrap();
    let provider_instance = ItemInstanceId::from_index(2).unwrap();
    let mut module = module_with_contract_call(contract_instance, provider_instance);
    module.external_calls.clear();
    let dependency = provider_dependency_module(provider_instance);

    apply_host_provider_bindings(&mut module, &[dependency], &[binding(contract_instance, provider_instance)])
        .expect("未被入口闭包引用的 contract 不要求 provider 身份");

    assert!(module.external_calls.is_empty());
    assert!(module.callable_symbols.contains_key(&contract_instance));
}

#[test]
fn missing_provider_definition_fails_closed() {
    let contract_instance = ItemInstanceId::from_index(1).unwrap();
    let provider_instance = ItemInstanceId::from_index(2).unwrap();
    let missing_provider = ItemInstanceId::from_index(99).unwrap();
    let mut module = module_with_contract_call(contract_instance, provider_instance);

    let error = apply_host_provider_bindings(&mut module, &[], &[binding(contract_instance, missing_provider)])
        .expect_err("missing provider must fail");
    assert!(error.to_string().contains("host provider"));
}

#[test]
fn host_provider_ignores_decoy_with_same_display_symbol() {
    let contract_instance = ItemInstanceId::from_index(1).unwrap();
    let provider_instance = ItemInstanceId::from_index(2).unwrap();
    let decoy_instance = ItemInstanceId::from_index(99).unwrap();
    let mut module = module_with_contract_call(contract_instance, provider_instance);
    module.functions.push(MirFunction {
        display_symbol: "std.adaptor.clr::write".into(),
        symbol: NamePath::new(vec![Identifier::new("decoy"), Identifier::new("write")]),
        declaration: Some(ItemId::from_index(99).unwrap()),
        instance: Some(decoy_instance),
        return_type: ValkyrieType::Integer32 { signed: true },
        param_types: Vec::new(),
        value_types: BTreeMap::new(),
        entry: MirBlockRef(0),
        values: Vec::new(),
        blocks: vec![MirBlock {
            id: MirBlockRef(0),
            label: "decoy".into(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: MirTerminator::Return { value: None },
        }],
    });
    let dependency = provider_dependency_module(provider_instance);

    apply_host_provider_bindings(&mut module, &[dependency], &[binding(contract_instance, provider_instance)])
        .expect("provider must resolve through declaration identity");

    let call = module.functions[0].blocks[0].instructions[0].kind.clone();
    assert_eq!(
        call,
        MirOperation::Call { callee: MirOperand::Callable(provider_instance), arguments: vec![MirOperand::Value(MirValueRef(0))] }
    );
}

#[test]
fn conflicting_host_contract_bindings_fail_closed() {
    let contract_instance = ItemInstanceId::from_index(1).unwrap();
    let provider_instance = ItemInstanceId::from_index(2).unwrap();
    let other_provider = ItemInstanceId::from_index(3).unwrap();
    let mut module = module_with_contract_call(contract_instance, provider_instance);
    let dependencies = [provider_dependency_module(provider_instance), provider_dependency_module(other_provider)];

    let error = apply_host_provider_bindings(
        &mut module,
        &dependencies,
        &[binding(contract_instance, provider_instance), binding(contract_instance, other_provider)],
    )
    .expect_err("ambiguous contract must fail");
    assert!(error.to_string().contains("多个"));
}
