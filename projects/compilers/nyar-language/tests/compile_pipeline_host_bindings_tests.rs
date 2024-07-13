use nyar::ExternalImportLink;
use nyar_language::{
    CompilerHostProviderBinding, Identifier, NamePath,
    types::hir::ValkyrieType,
    valkyrie::mir::{
        AggregateLayoutPlan, MirBlock, MirBlockRef, MirExternalCallContract, MirFunction, MirInstruction, MirModule, MirOperand, MirOperation,
        MirTerminator, MirValueRef,
    },
};
use nyar_types::{ItemId, ItemInstanceId};

use nyar_language::valkyrie::compile_pipeline::apply_host_provider_bindings;
use std::collections::BTreeMap;

fn module_with_contract_call(contract_instance: ItemInstanceId, provider_instance: ItemInstanceId) -> MirModule {
    MirModule {
        name: "demo".into(),
        functions: vec![MirFunction {
            display_symbol: "demo::main".into(),
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

#[test]
fn selected_host_provider_rewrites_contract_calls_to_provider_instance() {
    let contract_instance = ItemInstanceId::from_index(1).unwrap();
    let provider_instance = ItemInstanceId::from_index(2).unwrap();
    let mut module = module_with_contract_call(contract_instance, provider_instance);
    let bindings = vec![CompilerHostProviderBinding { contract: "demo::write".into(), symbol: "std.adaptor.clr::write".into() }];

    apply_host_provider_bindings(&mut module, &[], &bindings).expect("host provider binding must close adaptor contract");

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
    let bindings = vec![CompilerHostProviderBinding { contract: "demo::write".into(), symbol: "std.adaptor.clr::write".into() }];

    apply_host_provider_bindings(&mut module, &[], &bindings).expect("未被入口闭包引用的 contract 不要求 provider 身份");

    assert!(module.external_calls.is_empty());
    assert!(module.callable_symbols.contains_key(&contract_instance));
}

#[test]
fn missing_provider_symbol_fails_closed() {
    let contract_instance = ItemInstanceId::from_index(1).unwrap();
    let provider_instance = ItemInstanceId::from_index(2).unwrap();
    let mut module = module_with_contract_call(contract_instance, provider_instance);
    module.callable_symbols.remove(&provider_instance);
    let bindings = vec![CompilerHostProviderBinding { contract: "demo::write".into(), symbol: "std.adaptor.clr::write".into() }];

    let error = apply_host_provider_bindings(&mut module, &[], &bindings).expect_err("missing provider must fail");
    assert!(error.to_string().contains("host provider"));
}

#[test]
fn ambiguous_host_contract_fails_closed() {
    let contract_instance = ItemInstanceId::from_index(1).unwrap();
    let provider_instance = ItemInstanceId::from_index(2).unwrap();
    let mut module = module_with_contract_call(contract_instance, provider_instance);
    let duplicate = module.external_calls[0].clone();
    module.external_calls.push(duplicate);
    let bindings = vec![CompilerHostProviderBinding { contract: "demo::write".into(), symbol: "std.adaptor.clr::write".into() }];

    let error = apply_host_provider_bindings(&mut module, &[], &bindings).expect_err("ambiguous contract must fail");
    assert!(error.to_string().contains("多个候选"));
}
