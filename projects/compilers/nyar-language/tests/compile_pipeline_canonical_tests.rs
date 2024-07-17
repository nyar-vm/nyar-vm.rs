use nyar_language::{
    MirConstant, MirFunction, MirOperand, MirOperation, MirTerminator, MirValueOrigin,
    types::hir::ValkyrieType,
    valkyrie::{
        compile_pipeline::canonical_program_from_semantic_mir,
        mir::{AggregateLayoutPlan, MirBlock, MirBlockRef, MirExternalCallContract, MirInstruction, MirModule, MirValue, MirValueRef},
        semantic_closure::micro_declaration_key,
        types::{Identifier, NamePath},
    },
};
use nyar_types::{CanonicalOperation, CanonicalPrimitiveType, CanonicalTypeKind, ImportIndex, ItemId, ItemInstanceId};
use std::collections::BTreeMap;

fn demo_main_declaration_key() -> nyar_language::valkyrie::semantic_closure::DeclarationKey {
    micro_declaration_key("demo", "demo.v", &[], "main")
}

fn source_surface_module() -> MirModule {
    let hir = nyar_language::ValkyrieCompiler::default()
        .compile_source("[export(name: \"answer\")] [main] micro answer() -> i32 { return 23 }")
        .expect("源码导出及入口合同");
    nyar_language::MirLowerer::lower_module_semantic(&hir)
}

#[test]
fn source_instances_reach_canonical_without_a_name_binding_table() {
    let mut module = source_surface_module();
    let instance = module.functions[0].instance.expect("Compiler 实例 identity");
    module.callable_symbols.clear();
    let program = canonical_program_from_semantic_mir(&module).expect("Canonical 只消费节点实例，不按名称重绑");
    assert!(program.mir.functions.contains_key(&instance));
    assert!(program.linked.exports.contains_key(&instance));
    assert!(program.linked.entries.contains_key(&instance));
}

#[test]
fn source_export_and_entry_diagnostic_names_cannot_rebind_instances() {
    let mut module = source_surface_module();
    let expected = module.functions[0].instance.expect("Compiler 实例 identity");
    module.exports[0].symbol = NamePath::new(vec![Identifier::new("unrelated_export_label")]);
    module.entries[0].symbol = NamePath::new(vec![Identifier::new("unrelated_entry_label")]);
    let program = canonical_program_from_semantic_mir(&module).expect("诊断标签不参与语义绑定");
    assert!(program.linked.exports.contains_key(&expected));
    assert!(program.linked.entries.contains_key(&expected));
}

#[test]
fn source_missing_node_instance_cannot_be_recovered_from_a_name_table() {
    let mut module = source_surface_module();
    assert!(module.functions[0].instance.take().is_some());
    assert!(!module.callable_symbols.is_empty());
    let error = canonical_program_from_semantic_mir(&module).expect_err("名称表不能补造缺失节点身份");
    assert_eq!(error.records[0].code, "CAN034");
}

fn module_with(operation: MirOperation, return_value: Option<MirValueRef>, value_types: BTreeMap<MirValueRef, ValkyrieType>) -> MirModule {
    let return_type = return_value.and_then(|value| value_types.get(&value).cloned()).unwrap_or(ValkyrieType::Unit);
    let values = value_types.keys().map(|id| MirValue { id: *id, origin: MirValueOrigin::Temporary }).collect();
    let mut module = MirModule {
        name: "demo".into(),
        functions: vec![MirFunction {
            display_symbol: "demo::main".into(),
            declaration_key: Some(demo_main_declaration_key()),
            declaration: Some(ItemId::from_index(0).unwrap()),
            instance: Some(ItemInstanceId::from_index(0).unwrap()),
            return_type,
            param_types: Vec::new(),
            value_types,
            entry: MirBlockRef(0),
            values,
            blocks: vec![MirBlock {
                id: MirBlockRef(0),
                label: "entry".into(),
                parameters: Vec::new(),
                instructions: vec![MirInstruction::from_operation_with_results(operation, return_value.into_iter().collect())],
                terminator: MirTerminator::Return { value: return_value.map(MirOperand::Value) },
            }],
        }],
        structs: Vec::new(),
        imports: Vec::new(),
        external_calls: Vec::new(),
        exports: Vec::new(),
        entries: Vec::new(),
        callable_symbols: BTreeMap::from([(ItemInstanceId::from_index(0).unwrap(), "demo::main".to_owned())]),
        type_identities: BTreeMap::new(),
        aggregate_layouts: AggregateLayoutPlan::default(),
        sum_types: Vec::new(),
        flags_types: Vec::new(),
        singleton_instances: Vec::new(),
        semantic_fragments: Vec::new(),
        diagnostics: Vec::new(),
    };
    module.type_identities =
        nyar_language::valkyrie::mir::ssa::type_identity_table(&module.functions, &module.external_calls, &module.structs, &module.sum_types);
    module
}

#[test]
fn producer_preserves_exact_function_identity_and_type_shape() {
    let value = MirValueRef(0);
    let module = module_with(
        MirOperation::LoadConstant { constant: MirConstant::Bool(true), ty: Some(ValkyrieType::Boolean) },
        Some(value),
        BTreeMap::from([(value, ValkyrieType::Boolean)]),
    );
    let program = canonical_program_from_semantic_mir(&module).expect("精确单态函数应进入 canonical");
    assert_eq!(program.linked.item_instances.len(), 1);
    assert_eq!(program.mir.functions.len(), 1);
    assert_eq!(program.linked.callable_names.values().map(ToString::to_string).collect::<Vec<_>>(), vec!["main"]);
    assert!(program.linked.types.values().any(|record| matches!(record.kind, CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Bool))));
}

#[test]
fn producer_carries_export_and_entry_contracts_by_callable_identity() {
    let output = nyar_language::ValkyrieCompiler::default()
        .compile_source_to_program("[export(name: \"public_main\")] [main] micro main() -> unit { return }")
        .expect("源码必须完成前端分析");
    let semantic_mir = nyar_language::valkyrie::mir::MirLowerer::lower_module_semantic(
        &nyar_language::ValkyrieCompiler::default()
            .compile_source("[export(name: \"public_main\")] [main] micro main() -> unit { return }")
            .expect("source for canonical test"),
    );
    let program = canonical_program_from_semantic_mir(&semantic_mir).expect("公开合同必须进入 canonical");
    assert_eq!(program.linked.exports.len(), 1);
    assert_eq!(program.linked.entries.len(), 1);
    let (instance, export) = program.linked.exports.iter().next().unwrap();
    assert_eq!(program.linked.entries.get(instance), Some(&nyar_types::canonical_program::EntryRecord));
    assert_eq!(export.exported_name, "public_main");
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn source_entry_contract_rejects_missing_reordered_and_invalid_parameters() {
    let output = nyar_language::ValkyrieCompiler::default()
        .compile_source_to_program("micro select(value: bool, other: i32) -> bool { return value }")
        .expect("源码必须完成前端分析");
    let module = nyar_language::valkyrie::mir::MirLowerer::lower_module_semantic(
        &nyar_language::ValkyrieCompiler::default()
            .compile_source("micro select(value: bool, other: i32) -> bool { return value }")
            .expect("source for canonical test"),
    );
    canonical_program_from_semantic_mir(&module).expect("合法入口必须产生 CanonicalProgram");

    let mut missing = module.clone();
    missing.functions[0].blocks[0].parameters.pop();
    let mut reordered = module.clone();
    reordered.functions[0].blocks[0].parameters.swap(0, 1);
    let mut invalid_origin = module.clone();
    invalid_origin.functions[0].values[0].origin = MirValueOrigin::Parameter { index: usize::MAX, name: "value".into() };
    let mut duplicate_origin = module.clone();
    duplicate_origin.functions[0].values.push(module.functions[0].values[0].clone());
    let mut wrong_type = module.clone();
    let value = wrong_type.functions[0].blocks[0].parameters[0];
    wrong_type.functions[0].value_types.insert(value, ValkyrieType::Unit);
    for invalid in [missing, reordered, invalid_origin, duplicate_origin, wrong_type] {
        let error = canonical_program_from_semantic_mir(&invalid).expect_err("不一致入口不得通过删除参数修复");
        assert_eq!(error.records[0].code, "CAN032");
    }
}

#[test]
fn producer_rejects_unresolved_callable_without_name_fallback() {
    let module = module_with(
        MirOperation::Call { callee: MirOperand::Symbol(NamePath::new(vec![Identifier::new("new")])), arguments: Vec::new() },
        None,
        BTreeMap::new(),
    );
    let error = canonical_program_from_semantic_mir(&module).expect_err("未解析 callable 必须在 producer 失败");
    assert_eq!(error.records[0].code, "CAN007");
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn producer_rejects_missing_compiler_callable_identity() {
    let mut module =
        module_with(MirOperation::LoadConstant { constant: MirConstant::Unit, ty: Some(ValkyrieType::Unit) }, None, BTreeMap::new());
    module.callable_symbols.clear();
    let error = canonical_program_from_semantic_mir(&module).expect_err("缺失 Compiler callable identity 必须失败");
    assert_eq!(error.records[0].code, "CAN034");
}

#[test]
fn producer_rejects_missing_compiler_type_identity() {
    let mut module =
        module_with(MirOperation::LoadConstant { constant: MirConstant::Unit, ty: Some(ValkyrieType::Unit) }, None, BTreeMap::new());
    module.type_identities.clear();
    let error = canonical_program_from_semantic_mir(&module).expect_err("缺失 Compiler type identity 必须失败");
    assert_eq!(error.records[0].code, "CAN035");
}

#[test]
fn producer_rejects_semantic_mir_lowering_diagnostics() {
    let mut module =
        module_with(MirOperation::LoadConstant { constant: MirConstant::Unit, ty: Some(ValkyrieType::Unit) }, None, BTreeMap::new());
    module
        .diagnostics
        .push(nyar_language::valkyrie::mir::MirDiagnostic::UnresolvedVariantIdentity { sum_type: "Option".into(), variant: "Missing".into() });
    let error = canonical_program_from_semantic_mir(&module).expect_err("lowering diagnostic must not enter canonical success");
    assert_eq!(error.records[0].code, "CAN033");
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn producer_carries_singleton_lifecycle_contract() {
    let output = nyar_language::ValkyrieCompiler::default()
        .compile_source_to_program(
            r#"lazy singleton Counter {
    total: i64 = 0
}"#,
        )
        .expect("singleton 源码必须产生完整 Canonical 合同");
    let plans = &output.canonical().linked.singleton_instances;
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].qualified_name(), "Counter");
    assert!(plans[0].is_lazy);
    assert_eq!(plans[0].instance_field, "INSTANCE");
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn producer_binds_external_callable_to_import_index_and_signature() {
    let mut module =
        module_with(MirOperation::LoadConstant { constant: MirConstant::Unit, ty: Some(ValkyrieType::Unit) }, None, BTreeMap::new());
    module.external_calls.push(MirExternalCallContract {
        declaration_key: Some(micro_declaration_key("demo", "demo.v", &[], "write")),
        declaration: Some(ItemId::from_index(1).unwrap()),
        instance: Some(ItemInstanceId::from_index(1).unwrap()),
        symbol: NamePath::new(vec![Identifier::new("std"), Identifier::new("console"), Identifier::new("write")]),
        link: nyar_types::ExternalImportLink::host(None, vec!["std".to_owned(), "console".to_owned(), "write".to_owned()]),
        parameter_types: vec![ValkyrieType::Boolean],
        return_type: ValkyrieType::Unit,
    });

    let program = canonical_program_from_semantic_mir(&module).expect("external declaration has a complete import contract");
    let import = ImportIndex::from_index(0).unwrap();
    let record = program.linked.imports.get(&import).expect("import index");
    assert_eq!(record.callee.index(), 1);
    assert_eq!(
        record.parameter_types,
        vec![
            program
                .linked
                .types
                .iter()
                .find_map(|(id, record)| matches!(record.kind, CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Bool)).then_some(*id))
                .unwrap()
        ]
    );
    assert_eq!(
        record.return_type,
        program
            .linked
            .types
            .iter()
            .find_map(|(id, record)| matches!(record.kind, CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Unit)).then_some(*id))
            .unwrap()
    );
}

#[test]
fn array_push_operation_canonicalizes_without_symbol_call() {
    let array_ty = ValkyrieType::Array(Box::new(ValkyrieType::Utf8));
    let array = MirValueRef(0);
    let element = MirValueRef(1);
    let result = MirValueRef(2);
    let value_types = BTreeMap::from([(array, array_ty.clone()), (element, ValkyrieType::Utf8), (result, array_ty.clone())]);
    let module = MirModule {
        name: "demo".into(),
        functions: vec![MirFunction {
            display_symbol: "demo::main".into(),
            declaration_key: Some(demo_main_declaration_key()),
            declaration: Some(ItemId::from_index(0).unwrap()),
            instance: Some(ItemInstanceId::from_index(0).unwrap()),
            return_type: array_ty.clone(),
            param_types: vec![array_ty.clone(), ValkyrieType::Utf8],
            value_types,
            entry: MirBlockRef(0),
            values: vec![
                MirValue { id: array, origin: MirValueOrigin::Parameter { index: 0, name: "array".into() } },
                MirValue { id: element, origin: MirValueOrigin::Parameter { index: 1, name: "element".into() } },
                MirValue { id: result, origin: MirValueOrigin::CallResult },
            ],
            blocks: vec![MirBlock {
                id: MirBlockRef(0),
                label: "entry".into(),
                parameters: vec![array, element],
                instructions: vec![MirInstruction::from_operation_with_results(
                    MirOperation::ArrayPush { array: MirOperand::Value(array), value: MirOperand::Value(element) },
                    vec![result],
                )],
                terminator: MirTerminator::Return { value: Some(MirOperand::Value(result)) },
            }],
        }],
        structs: Vec::new(),
        imports: Vec::new(),
        external_calls: Vec::new(),
        exports: Vec::new(),
        entries: Vec::new(),
        callable_symbols: BTreeMap::from([(ItemInstanceId::from_index(0).unwrap(), "demo::main".to_owned())]),
        type_identities: BTreeMap::new(),
        aggregate_layouts: AggregateLayoutPlan::default(),
        sum_types: Vec::new(),
        flags_types: Vec::new(),
        singleton_instances: Vec::new(),
        semantic_fragments: Vec::new(),
        diagnostics: Vec::new(),
    };
    let mut module = module;
    module.type_identities =
        nyar_language::valkyrie::mir::ssa::type_identity_table(&module.functions, &module.external_calls, &module.structs, &module.sum_types);
    let program = canonical_program_from_semantic_mir(&module).expect("ArrayPush must canonicalize");
    let has_push = program
        .mir
        .functions
        .values()
        .flat_map(|function| function.blocks.values())
        .flat_map(|block| block.instructions.iter())
        .any(|instruction| matches!(instruction.operation, CanonicalOperation::ArrayPush { .. }));
    assert!(has_push, "expected CanonicalOperation::ArrayPush");
    assert!(
        !program
            .mir
            .functions
            .values()
            .flat_map(|function| function.blocks.values())
            .flat_map(|block| block.instructions.iter())
            .any(|instruction| matches!(instruction.operation, CanonicalOperation::Invoke { .. })),
        "ArrayPush must not remain as unresolved Invoke"
    );
}

#[test]
fn array_set_operation_canonicalizes_without_symbol_call() {
    let array_ty = ValkyrieType::Array(Box::new(ValkyrieType::Utf8));
    let array = MirValueRef(0);
    let index = MirValueRef(1);
    let element = MirValueRef(2);
    let index_ty = ValkyrieType::Integer32 { signed: true };
    let value_types = BTreeMap::from([(array, array_ty.clone()), (index, index_ty.clone()), (element, ValkyrieType::Utf8)]);
    let module = MirModule {
        name: "demo".into(),
        functions: vec![MirFunction {
            display_symbol: "demo::main".into(),
            declaration_key: Some(demo_main_declaration_key()),
            declaration: Some(ItemId::from_index(0).unwrap()),
            instance: Some(ItemInstanceId::from_index(0).unwrap()),
            return_type: ValkyrieType::Unit,
            param_types: vec![array_ty.clone(), index_ty.clone(), ValkyrieType::Utf8],
            value_types,
            entry: MirBlockRef(0),
            values: vec![
                MirValue { id: array, origin: MirValueOrigin::Parameter { index: 0, name: "array".into() } },
                MirValue { id: index, origin: MirValueOrigin::Parameter { index: 1, name: "index".into() } },
                MirValue { id: element, origin: MirValueOrigin::Parameter { index: 2, name: "element".into() } },
            ],
            blocks: vec![MirBlock {
                id: MirBlockRef(0),
                label: "entry".into(),
                parameters: vec![array, index, element],
                instructions: vec![MirInstruction::from_operation(MirOperation::ArraySet {
                    array: MirOperand::Value(array),
                    index: MirOperand::Value(index),
                    value: MirOperand::Value(element),
                })],
                terminator: MirTerminator::Return { value: None },
            }],
        }],
        structs: Vec::new(),
        imports: Vec::new(),
        external_calls: Vec::new(),
        exports: Vec::new(),
        entries: Vec::new(),
        callable_symbols: BTreeMap::from([(ItemInstanceId::from_index(0).unwrap(), "demo::main".to_owned())]),
        type_identities: BTreeMap::new(),
        aggregate_layouts: AggregateLayoutPlan::default(),
        sum_types: Vec::new(),
        flags_types: Vec::new(),
        singleton_instances: Vec::new(),
        semantic_fragments: Vec::new(),
        diagnostics: Vec::new(),
    };
    let mut module = module;
    module.type_identities =
        nyar_language::valkyrie::mir::ssa::type_identity_table(&module.functions, &module.external_calls, &module.structs, &module.sum_types);
    let program = canonical_program_from_semantic_mir(&module).expect("ArraySet must canonicalize");
    let has_set = program
        .mir
        .functions
        .values()
        .flat_map(|function| function.blocks.values())
        .flat_map(|block| block.instructions.iter())
        .any(|instruction| matches!(instruction.operation, CanonicalOperation::ArraySet { .. }));
    assert!(has_set, "expected CanonicalOperation::ArraySet");
    assert!(
        !program
            .mir
            .functions
            .values()
            .flat_map(|function| function.blocks.values())
            .flat_map(|block| block.instructions.iter())
            .any(|instruction| matches!(instruction.operation, CanonicalOperation::Invoke { .. })),
        "ArraySet must not remain as unresolved Invoke"
    );
}

#[test]
fn array_length_operation_canonicalizes_without_symbol_call() {
    let array_ty = ValkyrieType::Array(Box::new(ValkyrieType::Utf8));
    let array = MirValueRef(0);
    let result = MirValueRef(1);
    let usize_ty = ValkyrieType::Integer32 { signed: false };
    let value_types = BTreeMap::from([(array, array_ty.clone()), (result, usize_ty.clone())]);
    let module = MirModule {
        name: "demo".into(),
        functions: vec![MirFunction {
            display_symbol: "demo::main".into(),
            declaration_key: Some(demo_main_declaration_key()),
            declaration: Some(ItemId::from_index(0).unwrap()),
            instance: Some(ItemInstanceId::from_index(0).unwrap()),
            return_type: usize_ty.clone(),
            param_types: vec![array_ty.clone()],
            value_types,
            entry: MirBlockRef(0),
            values: vec![
                MirValue { id: array, origin: MirValueOrigin::Parameter { index: 0, name: "array".into() } },
                MirValue { id: result, origin: MirValueOrigin::CallResult },
            ],
            blocks: vec![MirBlock {
                id: MirBlockRef(0),
                label: "entry".into(),
                parameters: vec![array],
                instructions: vec![MirInstruction::from_operation_with_results(
                    MirOperation::ArrayLength { array: MirOperand::Value(array) },
                    vec![result],
                )],
                terminator: MirTerminator::Return { value: Some(MirOperand::Value(result)) },
            }],
        }],
        structs: Vec::new(),
        imports: Vec::new(),
        external_calls: Vec::new(),
        exports: Vec::new(),
        entries: Vec::new(),
        callable_symbols: BTreeMap::from([(ItemInstanceId::from_index(0).unwrap(), "demo::main".to_owned())]),
        type_identities: BTreeMap::new(),
        aggregate_layouts: AggregateLayoutPlan::default(),
        sum_types: Vec::new(),
        flags_types: Vec::new(),
        singleton_instances: Vec::new(),
        semantic_fragments: Vec::new(),
        diagnostics: Vec::new(),
    };
    let mut module = module;
    module.type_identities =
        nyar_language::valkyrie::mir::ssa::type_identity_table(&module.functions, &module.external_calls, &module.structs, &module.sum_types);
    let program = canonical_program_from_semantic_mir(&module).expect("ArrayLength must canonicalize");
    let has_len = program
        .mir
        .functions
        .values()
        .flat_map(|function| function.blocks.values())
        .flat_map(|block| block.instructions.iter())
        .any(|instruction| matches!(instruction.operation, CanonicalOperation::ArrayLength { .. }));
    assert!(has_len, "expected CanonicalOperation::ArrayLength");
    assert!(
        !program
            .mir
            .functions
            .values()
            .flat_map(|function| function.blocks.values())
            .flat_map(|block| block.instructions.iter())
            .any(|instruction| matches!(instruction.operation, CanonicalOperation::Invoke { .. })),
        "ArrayLength must not remain as unresolved Invoke"
    );
}

#[test]
fn array_get_operation_canonicalizes_without_symbol_call() {
    let array_ty = ValkyrieType::Array(Box::new(ValkyrieType::Utf8));
    let array = MirValueRef(0);
    let index = MirValueRef(1);
    let result = MirValueRef(2);
    let index_ty = ValkyrieType::Integer32 { signed: true };
    let value_types = BTreeMap::from([(array, array_ty.clone()), (index, index_ty.clone()), (result, ValkyrieType::Utf8)]);
    let module = MirModule {
        name: "demo".into(),
        functions: vec![MirFunction {
            display_symbol: "demo::main".into(),
            declaration_key: Some(demo_main_declaration_key()),
            declaration: Some(ItemId::from_index(0).unwrap()),
            instance: Some(ItemInstanceId::from_index(0).unwrap()),
            return_type: ValkyrieType::Utf8,
            param_types: vec![array_ty.clone(), index_ty.clone()],
            value_types,
            entry: MirBlockRef(0),
            values: vec![
                MirValue { id: array, origin: MirValueOrigin::Parameter { index: 0, name: "array".into() } },
                MirValue { id: index, origin: MirValueOrigin::Parameter { index: 1, name: "index".into() } },
                MirValue { id: result, origin: MirValueOrigin::CallResult },
            ],
            blocks: vec![MirBlock {
                id: MirBlockRef(0),
                label: "entry".into(),
                parameters: vec![array, index],
                instructions: vec![MirInstruction::from_operation_with_results(
                    MirOperation::ArrayGet { array: MirOperand::Value(array), index: MirOperand::Value(index) },
                    vec![result],
                )],
                terminator: MirTerminator::Return { value: Some(MirOperand::Value(result)) },
            }],
        }],
        structs: Vec::new(),
        imports: Vec::new(),
        external_calls: Vec::new(),
        exports: Vec::new(),
        entries: Vec::new(),
        callable_symbols: BTreeMap::from([(ItemInstanceId::from_index(0).unwrap(), "demo::main".to_owned())]),
        type_identities: BTreeMap::new(),
        aggregate_layouts: AggregateLayoutPlan::default(),
        sum_types: Vec::new(),
        flags_types: Vec::new(),
        singleton_instances: Vec::new(),
        semantic_fragments: Vec::new(),
        diagnostics: Vec::new(),
    };
    let mut module = module;
    module.type_identities =
        nyar_language::valkyrie::mir::ssa::type_identity_table(&module.functions, &module.external_calls, &module.structs, &module.sum_types);
    let program = canonical_program_from_semantic_mir(&module).expect("ArrayGet must canonicalize");
    let has_get = program
        .mir
        .functions
        .values()
        .flat_map(|function| function.blocks.values())
        .flat_map(|block| block.instructions.iter())
        .any(|instruction| matches!(instruction.operation, CanonicalOperation::ArrayGet { .. }));
    assert!(has_get, "expected CanonicalOperation::ArrayGet");
    assert!(
        !program
            .mir
            .functions
            .values()
            .flat_map(|function| function.blocks.values())
            .flat_map(|block| block.instructions.iter())
            .any(|instruction| matches!(instruction.operation, CanonicalOperation::Invoke { .. })),
        "ArrayGet must not remain as unresolved Invoke"
    );
}

#[test]
fn null_intrinsic_operations_canonicalize_without_symbol_call() {
    let payload_ty = ValkyrieType::Integer32 { signed: true };
    let nullable_ty = ValkyrieType::Nullable(Box::new(payload_ty.clone()));
    let value = MirValueRef(0);
    let is_null_result = MirValueRef(1);
    let unwrap_result = MirValueRef(2);
    let value_types =
        BTreeMap::from([(value, nullable_ty.clone()), (is_null_result, ValkyrieType::Boolean), (unwrap_result, payload_ty.clone())]);
    let module = MirModule {
        name: "demo".into(),
        functions: vec![MirFunction {
            display_symbol: "demo::main".into(),
            declaration_key: Some(demo_main_declaration_key()),
            declaration: Some(ItemId::from_index(0).unwrap()),
            instance: Some(ItemInstanceId::from_index(0).unwrap()),
            return_type: payload_ty.clone(),
            param_types: vec![nullable_ty.clone()],
            value_types,
            entry: MirBlockRef(0),
            values: vec![
                MirValue { id: value, origin: MirValueOrigin::Parameter { index: 0, name: "value".into() } },
                MirValue { id: is_null_result, origin: MirValueOrigin::CallResult },
                MirValue { id: unwrap_result, origin: MirValueOrigin::CallResult },
            ],
            blocks: vec![MirBlock {
                id: MirBlockRef(0),
                label: "entry".into(),
                parameters: vec![value],
                instructions: vec![
                    MirInstruction::from_operation_with_results(
                        MirOperation::NullIsNull { value: MirOperand::Value(value) },
                        vec![is_null_result],
                    ),
                    MirInstruction::from_operation_with_results(
                        MirOperation::NullUnwrap { value: MirOperand::Value(value) },
                        vec![unwrap_result],
                    ),
                ],
                terminator: MirTerminator::Return { value: Some(MirOperand::Value(unwrap_result)) },
            }],
        }],
        structs: Vec::new(),
        imports: Vec::new(),
        external_calls: Vec::new(),
        exports: Vec::new(),
        entries: Vec::new(),
        callable_symbols: BTreeMap::from([(ItemInstanceId::from_index(0).unwrap(), "demo::main".to_owned())]),
        type_identities: BTreeMap::new(),
        aggregate_layouts: AggregateLayoutPlan::default(),
        sum_types: Vec::new(),
        flags_types: Vec::new(),
        singleton_instances: Vec::new(),
        semantic_fragments: Vec::new(),
        diagnostics: Vec::new(),
    };
    let mut module = module;
    module.type_identities =
        nyar_language::valkyrie::mir::ssa::type_identity_table(&module.functions, &module.external_calls, &module.structs, &module.sum_types);
    let program = canonical_program_from_semantic_mir(&module).expect("nullable intrinsics must canonicalize");
    let ops = program
        .mir
        .functions
        .values()
        .flat_map(|function| function.blocks.values())
        .flat_map(|block| block.instructions.iter())
        .map(|instruction| instruction.operation.clone())
        .collect::<Vec<_>>();
    assert!(ops.iter().any(|op| matches!(op, CanonicalOperation::NullIsNull { .. })));
    assert!(ops.iter().any(|op| matches!(op, CanonicalOperation::NullUnwrap { .. })));
    assert!(!ops.iter().any(|op| matches!(op, CanonicalOperation::Invoke { .. })));
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn i32_pow_while_loop_canonicalizes_with_mutable_locals() {
    nyar_language::ValkyrieCompiler::default()
        .compile_source_to_program(
            r#"
micro __i32_pow(lhs: i32, rhs: i32): i32 {
    let mut base: i32 = lhs
    let mut exp: i32 = rhs
    let mut acc: i32 = 1
    while exp > 0 {
        if exp % 2 == 1 {
            acc = acc * base
        }
        base = base * base
        exp = exp / 2
    }
    return acc
}"#,
        )
        .expect("__i32_pow must reach canonical with mutable loop locals");
}
