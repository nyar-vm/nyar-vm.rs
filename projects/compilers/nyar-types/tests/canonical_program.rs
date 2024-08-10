use std::collections::BTreeMap;

use nyar_types::{
    canonical_program::{CanonicalEffectKind, EntryRecord, ExportRecord},
    layout_choice::RepresentationPlan,
    semantic_ids::ItemInstanceId,
    *,
};

#[test]
fn canonical_program_is_success_only() {
    let mut linked = LinkedSemanticProgram::default();
    linked.module_name = "demo".into();
    let item = ItemInstanceId::from_index(0).unwrap();
    let ty = TypeId::from_index(0).unwrap();
    linked.types.insert(ty, TypeRecord { declaration: ty, kind: CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Unit) });
    linked
        .item_instances
        .insert(item, ItemInstanceRecord { declaration: ItemId::from_index(0).unwrap(), parameter_types: Vec::new(), return_type: ty });
    let instance = ItemInstanceId::from_index(0).unwrap();
    let function = CanonicalFunction {
        instance,
        parameters: Vec::new(),
        return_type: ty,
        value_types: BTreeMap::new(),
        entry: CanonicalBlockId(0),
        blocks: BTreeMap::from([(
            CanonicalBlockId(0),
            CanonicalBlock {
                id: CanonicalBlockId(0),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: CanonicalTerminator::Return { value: None },
            },
        )]),
    };
    let mut functions = BTreeMap::new();
    functions.insert(instance, function);
    let program = CanonicalProgram { linked, mir: CanonicalSemanticMir { module_name: "demo".into(), functions } };
    assert_eq!(program.mir.functions.len(), 1);
    program.mir.validate(&program.linked).expect("canonical MIR contract");
    program.validate().expect("canonical program contract");
}

#[test]
fn canonical_mir_rejects_unknown_callee_before_planning() {
    let mut linked = LinkedSemanticProgram::default();
    let instance = ItemInstanceId::from_index(0).unwrap();
    let unknown = ItemInstanceId::from_index(1).unwrap();
    let ty = TypeId::from_index(0).unwrap();
    linked
        .item_instances
        .insert(instance, ItemInstanceRecord { declaration: ItemId::from_index(0).unwrap(), parameter_types: Vec::new(), return_type: ty });
    linked.types.insert(ty, TypeRecord { declaration: ty, kind: CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Unit) });
    let function = CanonicalFunction {
        instance,
        parameters: Vec::new(),
        return_type: ty,
        value_types: BTreeMap::new(),
        entry: CanonicalBlockId(0),
        blocks: BTreeMap::from([(
            CanonicalBlockId(0),
            CanonicalBlock {
                id: CanonicalBlockId(0),
                parameters: Vec::new(),
                instructions: vec![CanonicalInstruction {
                    id: InstructionId::from_index(0).unwrap(),
                    results: Vec::new(),
                    operation: CanonicalOperation::Invoke { callee: CanonicalCallee::Item(unknown), arguments: Vec::new() },
                }],
                terminator: CanonicalTerminator::Return { value: None },
            },
        )]),
    };
    let mut functions = BTreeMap::new();
    functions.insert(instance, function);
    let mir = CanonicalSemanticMir { module_name: "demo".into(), functions };
    assert!(matches!(mir.validate(&linked), Err(CanonicalMirError::UnknownCallee { .. })));
}

#[test]
fn structured_diagnostic_set_rejects_empty() {
    assert!(StructuredDiagnosticSet::from_records(Vec::new()).is_none());
}

#[test]
fn canonical_array_contract_checks_inputs_and_results_before_planning() {
    let instance = ItemInstanceId::from_index(0).unwrap();
    let integer = TypeId::from_index(0).unwrap();
    let boolean = TypeId::from_index(1).unwrap();
    let array_type = TypeId::from_index(2).unwrap();
    let array = MirValueId::from_index(0).unwrap();
    let index = MirValueId::from_index(1).unwrap();
    let result = MirValueId::from_index(2).unwrap();
    let instruction = InstructionId::from_index(0).unwrap();
    let mut linked = LinkedSemanticProgram::default();
    for (ty, kind) in [
        (integer, CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { bits: 32, signed: true })),
        (boolean, CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Bool)),
        (array_type, CanonicalTypeKind::Array { element: integer, length: None }),
    ] {
        linked.types.insert(ty, TypeRecord { declaration: ty, kind });
    }
    linked.item_instances.insert(
        instance,
        ItemInstanceRecord { declaration: ItemId::from_index(0).unwrap(), parameter_types: vec![array_type, integer], return_type: integer },
    );
    let function = CanonicalFunction {
        instance,
        parameters: vec![(array, array_type), (index, integer)],
        return_type: integer,
        value_types: BTreeMap::from([(array, array_type), (index, integer), (result, integer)]),
        entry: CanonicalBlockId(0),
        blocks: BTreeMap::from([(
            CanonicalBlockId(0),
            CanonicalBlock {
                id: CanonicalBlockId(0),
                parameters: Vec::new(),
                instructions: vec![CanonicalInstruction {
                    id: instruction,
                    results: vec![result],
                    operation: CanonicalOperation::ArrayGet { array, index },
                }],
                terminator: CanonicalTerminator::Return { value: Some(result) },
            },
        )]),
    };
    let program = CanonicalProgram {
        linked,
        mir: CanonicalSemanticMir { module_name: "array".into(), functions: BTreeMap::from([(instance, function)]) },
    };
    program.validate().expect("完整数组读取合同");
    let mut invalid = program.clone();
    invalid.mir.functions.get_mut(&instance).unwrap().value_types.insert(result, boolean);
    assert_eq!(invalid.validate(), Err(CanonicalMirError::OperationResultTypeMismatch { function: instance, instruction }));
    let mut invalid = program.clone();
    invalid.mir.functions.get_mut(&instance).unwrap().blocks.get_mut(&CanonicalBlockId(0)).unwrap().instructions[0].results.clear();
    assert_eq!(invalid.validate(), Err(CanonicalMirError::OperationResultArityMismatch { function: instance, instruction }));
    let mut invalid = program;
    invalid.mir.functions.get_mut(&instance).unwrap().blocks.get_mut(&CanonicalBlockId(0)).unwrap().instructions[0].operation =
        CanonicalOperation::ArrayGet { array: index, index };
    assert_eq!(invalid.validate(), Err(CanonicalMirError::OperationOperandTypeMismatch { function: instance, instruction, value: index }));
}

#[test]
fn canonical_effect_terminator_preserves_payload_and_resume_contract() {
    let instance = ItemInstanceId::from_index(0).unwrap();
    let ty = TypeId::from_index(0).unwrap();
    let payload = MirValueId::from_index(0).unwrap();
    let resume_value = MirValueId::from_index(1).unwrap();
    let mut linked = LinkedSemanticProgram::default();
    linked.types.insert(ty, TypeRecord { declaration: ty, kind: CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Unit) });
    linked
        .item_instances
        .insert(instance, ItemInstanceRecord { declaration: ItemId::from_index(0).unwrap(), parameter_types: vec![ty], return_type: ty });
    let function = CanonicalFunction {
        instance,
        parameters: vec![(payload, ty)],
        return_type: ty,
        value_types: BTreeMap::from([(payload, ty), (resume_value, ty)]),
        entry: CanonicalBlockId(0),
        blocks: BTreeMap::from([
            (
                CanonicalBlockId(0),
                CanonicalBlock {
                    id: CanonicalBlockId(0),
                    parameters: Vec::new(),
                    instructions: Vec::new(),
                    terminator: CanonicalTerminator::PerformEffect {
                        effect: CanonicalEffectKind::Raise,
                        payload: Some(payload),
                        resume_target: CanonicalBlockId(1),
                    },
                },
            ),
            (
                CanonicalBlockId(1),
                CanonicalBlock {
                    id: CanonicalBlockId(1),
                    parameters: vec![(resume_value, ty)],
                    instructions: Vec::new(),
                    terminator: CanonicalTerminator::Return { value: None },
                },
            ),
        ]),
    };
    let program = CanonicalSemanticMir { module_name: "effect".into(), functions: BTreeMap::from([(instance, function)]) };
    program.validate(&linked).expect("effect payload and resume block are complete");
}

#[test]
fn canonical_effect_terminator_rejects_missing_payload() {
    let instance = ItemInstanceId::from_index(0).unwrap();
    let ty = TypeId::from_index(0).unwrap();
    let mut linked = LinkedSemanticProgram::default();
    linked.types.insert(ty, TypeRecord { declaration: ty, kind: CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Unit) });
    linked
        .item_instances
        .insert(instance, ItemInstanceRecord { declaration: ItemId::from_index(0).unwrap(), parameter_types: Vec::new(), return_type: ty });
    let function = CanonicalFunction {
        instance,
        parameters: Vec::new(),
        return_type: ty,
        value_types: BTreeMap::new(),
        entry: CanonicalBlockId(0),
        blocks: BTreeMap::from([(
            CanonicalBlockId(0),
            CanonicalBlock {
                id: CanonicalBlockId(0),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: CanonicalTerminator::PerformEffect {
                    effect: CanonicalEffectKind::Raise,
                    payload: None,
                    resume_target: CanonicalBlockId(0),
                },
            },
        )]),
    };
    let program = CanonicalSemanticMir { module_name: "effect".into(), functions: BTreeMap::from([(instance, function)]) };
    assert!(matches!(program.validate(&linked), Err(CanonicalMirError::MissingEffectPayload { .. })));
}

fn typed_call_program() -> CanonicalProgram {
    let caller = ItemInstanceId::from_index(0).unwrap();
    let callee = ItemInstanceId::from_index(1).unwrap();
    let ty = TypeId::from_index(0).unwrap();
    let argument = MirValueId::from_index(0).unwrap();
    let result = MirValueId::from_index(1).unwrap();
    let mut linked = LinkedSemanticProgram::default();
    linked.types.insert(ty, TypeRecord { declaration: ty, kind: CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Unit) });
    for instance in [caller, callee] {
        linked.item_instances.insert(
            instance,
            ItemInstanceRecord { declaration: ItemId::from_index(instance.index()).unwrap(), parameter_types: vec![ty], return_type: ty },
        );
    }
    let function = CanonicalFunction {
        instance: caller,
        parameters: vec![(argument, ty)],
        return_type: ty,
        value_types: BTreeMap::from([(argument, ty), (result, ty)]),
        entry: CanonicalBlockId(0),
        blocks: BTreeMap::from([(
            CanonicalBlockId(0),
            CanonicalBlock {
                id: CanonicalBlockId(0),
                parameters: Vec::new(),
                instructions: vec![CanonicalInstruction {
                    id: InstructionId::from_index(0).unwrap(),
                    results: vec![result],
                    operation: CanonicalOperation::Invoke { callee: CanonicalCallee::Item(callee), arguments: vec![argument] },
                }],
                terminator: CanonicalTerminator::Return { value: Some(result) },
            },
        )]),
    };
    CanonicalProgram { linked, mir: CanonicalSemanticMir { module_name: "typed".into(), functions: BTreeMap::from([(caller, function)]) } }
}

fn nominal_program() -> CanonicalProgram {
    let declaration = TypeId::from_index(0).unwrap();
    let field_type = TypeId::from_index(1).unwrap();
    let nominal = NominalInstanceId::from_index(0).unwrap();
    let field = FieldId::from_index(0).unwrap();
    let mut program = CanonicalProgram { linked: LinkedSemanticProgram::default(), mir: CanonicalSemanticMir::default() };
    program
        .linked
        .types
        .insert(declaration, TypeRecord { declaration, kind: CanonicalTypeKind::Nominal { declaration, arguments: Vec::new() } });
    program
        .linked
        .types
        .insert(field_type, TypeRecord { declaration: field_type, kind: CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Bool) });
    program.linked.nominal_instances.insert(
        nominal,
        NominalInstanceRecord { declaration, ty: declaration, semantics: NominalValueSemantics::Reference, fields: vec![field] },
    );
    program.linked.fields.insert(field, FieldRecord { owner: nominal, ty: field_type });
    program
}

#[test]
fn canonical_nominal_contract_validates_the_entire_field_closure() {
    let program = nominal_program();
    program.validate().expect("没有构造指令的名义声明仍须验证");
    let nominal = NominalInstanceId::from_index(0).unwrap();
    let field = FieldId::from_index(0).unwrap();
    let mut invalid = program.clone();
    invalid.linked.nominal_instances.get_mut(&nominal).unwrap().fields.push(field);
    assert_eq!(invalid.validate(), Err(CanonicalMirError::InvalidNominalField { nominal, field }));
    let mut invalid = program.clone();
    invalid.linked.fields.get_mut(&field).unwrap().owner = NominalInstanceId::from_index(1).unwrap();
    assert!(matches!(invalid.validate(), Err(CanonicalMirError::InvalidNominalField { .. })));
    let mut invalid = program.clone();
    invalid.linked.fields.get_mut(&field).unwrap().ty = TypeId::from_index(9).unwrap();
    assert_eq!(invalid.validate(), Err(CanonicalMirError::InvalidNominalField { nominal, field }));
    let mut invalid = program.clone();
    invalid.linked.nominal_instances.get_mut(&nominal).unwrap().fields.clear();
    assert_eq!(invalid.validate(), Err(CanonicalMirError::InvalidNominalField { nominal, field }));
    let mut invalid = program.clone();
    invalid.linked.nominal_instances.clear();
    invalid.linked.fields.clear();
    assert_eq!(invalid.validate(), Err(CanonicalMirError::MissingNominalSemantics { declaration: TypeId::from_index(0).unwrap() }));
    let mut invalid = program;
    invalid.linked.nominal_instances.get_mut(&nominal).unwrap().declaration = TypeId::from_index(1).unwrap();
    assert!(matches!(invalid.validate(), Err(CanonicalMirError::InvalidNominalDeclaration { .. })));
}

#[test]
fn canonical_nominal_instances_cannot_disagree_on_declaration_semantics() {
    let mut program = nominal_program();
    let mut other = program.linked.nominal_instances.values().next().unwrap().clone();
    other.fields.clear();
    other.ty = TypeId::from_index(2).unwrap();
    program.linked.types.insert(
        other.ty,
        TypeRecord {
            declaration: other.declaration,
            kind: CanonicalTypeKind::Nominal { declaration: other.declaration, arguments: vec![TypeId::from_index(1).unwrap()] },
        },
    );
    let instance = NominalInstanceId::from_index(1).unwrap();
    program.linked.nominal_instances.insert(instance, other.clone());
    program.validate().expect("同一声明不同实例保持共同值语义");
    program.linked.nominal_instances.get_mut(&instance).unwrap().semantics = NominalValueSemantics::Value;
    assert_eq!(program.validate(), Err(CanonicalMirError::ConflictingNominalSemantics { declaration: other.declaration }));
}

#[test]
fn compiled_program_requires_exact_nominal_representation_coverage() {
    let program = nominal_program();
    let nominal = NominalInstanceId::from_index(0).unwrap();
    let mut plan = RepresentationPlan::default();
    assert_eq!(CompiledProgram::new(program.clone(), plan.clone()), Err(CompiledProgramError::MissingAdtRepresentation { nominal }));
    plan.adt_reps.insert(nominal, nyar_types::layout_choice::AdtRepresentation::TypedAggregate);
    CompiledProgram::new(program.clone(), plan.clone()).expect("名义表示完整");
    let extra = NominalInstanceId::from_index(9).unwrap();
    plan.adt_reps.insert(extra, nyar_types::layout_choice::AdtRepresentation::TypedAggregate);
    assert_eq!(CompiledProgram::new(program, plan), Err(CompiledProgramError::ExtraAdtRepresentation { nominal: extra }));
}

fn instantiated_nominal_operation_program(operation: CanonicalOperation, result_type: Option<TypeId>) -> CanonicalProgram {
    let mut program = nominal_program();
    let declaration = TypeId::from_index(0).unwrap();
    let boolean = TypeId::from_index(1).unwrap();
    let first_type = TypeId::from_index(2).unwrap();
    let second_type = TypeId::from_index(3).unwrap();
    let integer = TypeId::from_index(4).unwrap();
    program.linked.types.insert(
        integer,
        TypeRecord { declaration: integer, kind: CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { bits: 32, signed: true }) },
    );
    for (ty, argument) in [(first_type, boolean), (second_type, integer)] {
        program
            .linked
            .types
            .insert(ty, TypeRecord { declaration, kind: CanonicalTypeKind::Nominal { declaration, arguments: vec![argument] } });
    }
    let first = NominalInstanceId::from_index(0).unwrap();
    let second = NominalInstanceId::from_index(1).unwrap();
    program.linked.nominal_instances.get_mut(&first).unwrap().ty = first_type;
    let second_field = FieldId::from_index(1).unwrap();
    program.linked.nominal_instances.insert(
        second,
        NominalInstanceRecord { declaration, ty: second_type, semantics: NominalValueSemantics::Reference, fields: vec![second_field] },
    );
    program.linked.fields.insert(second_field, FieldRecord { owner: second, ty: boolean });
    for (index, owner) in [(0, first), (1, second)] {
        program.linked.variants.insert((owner, VariantId::from_index(index).unwrap()), VariantRecord { payload_type: Some(boolean) });
    }
    let instance = ItemInstanceId::from_index(0).unwrap();
    let object = MirValueId::from_index(0).unwrap();
    let payload = MirValueId::from_index(1).unwrap();
    let result = MirValueId::from_index(2).unwrap();
    let return_type = result_type.unwrap_or(boolean);
    program.linked.item_instances.insert(
        instance,
        ItemInstanceRecord { declaration: ItemId::from_index(0).unwrap(), parameter_types: vec![first_type, boolean], return_type },
    );
    let mut value_types = BTreeMap::from([(object, first_type), (payload, boolean)]);
    if let Some(ty) = result_type {
        value_types.insert(result, ty);
    }
    let entry = CanonicalBlockId(0);
    program.mir.functions.insert(
        instance,
        CanonicalFunction {
            instance,
            parameters: vec![(object, first_type), (payload, boolean)],
            return_type,
            value_types,
            entry,
            blocks: BTreeMap::from([(
                entry,
                CanonicalBlock {
                    id: entry,
                    parameters: Vec::new(),
                    instructions: vec![CanonicalInstruction {
                        id: InstructionId::from_index(0).unwrap(),
                        results: result_type.map(|_| vec![result]).unwrap_or_default(),
                        operation,
                    }],
                    terminator: CanonicalTerminator::Return { value: Some(if result_type.is_some() { result } else { payload }) },
                },
            )]),
        },
    );
    program
}

#[test]
fn canonical_structure_operations_use_exact_instantiated_owner_types() {
    let first = NominalInstanceId::from_index(0).unwrap();
    let second = NominalInstanceId::from_index(1).unwrap();
    let first_field = FieldId::from_index(0).unwrap();
    let second_field = FieldId::from_index(1).unwrap();
    let first_variant = VariantId::from_index(0).unwrap();
    let second_variant = VariantId::from_index(1).unwrap();
    let object = MirValueId::from_index(0).unwrap();
    let payload = MirValueId::from_index(1).unwrap();
    let boolean = TypeId::from_index(1).unwrap();
    let first_type = TypeId::from_index(2).unwrap();
    let cases = [
        (
            CanonicalOperation::StructNew { nominal: first, fields: vec![(first_field, payload)] },
            CanonicalOperation::StructNew { nominal: second, fields: vec![(second_field, payload)] },
            Some(first_type),
        ),
        (
            CanonicalOperation::FieldGet { object, field: first_field },
            CanonicalOperation::FieldGet { object, field: second_field },
            Some(boolean),
        ),
        (
            CanonicalOperation::FieldSet { object, field: first_field, value: payload },
            CanonicalOperation::FieldSet { object, field: second_field, value: payload },
            None,
        ),
        (
            CanonicalOperation::SumNew { nominal: first, variant: first_variant, payload: Some(payload) },
            CanonicalOperation::SumNew { nominal: second, variant: second_variant, payload: Some(payload) },
            Some(first_type),
        ),
        (
            CanonicalOperation::SumPayloadGet { nominal: first, variant: first_variant, object },
            CanonicalOperation::SumPayloadGet { nominal: second, variant: second_variant, object },
            Some(boolean),
        ),
        (
            CanonicalOperation::SumVariantIs { nominal: first, variant: first_variant, object },
            CanonicalOperation::SumVariantIs { nominal: second, variant: second_variant, object },
            Some(boolean),
        ),
    ];
    for (valid, wrong_owner, result_type) in cases {
        instantiated_nominal_operation_program(valid, result_type).validate().expect("同一声明的不同泛型实例可独立持有结构合同");
        let error = instantiated_nominal_operation_program(wrong_owner, result_type).validate().expect_err("字段形状相同不能掩盖泛型实例错位");
        assert!(
            matches!(
                error,
                CanonicalMirError::FieldOwnerMismatch { .. }
                    | CanonicalMirError::OperationResultTypeMismatch { .. }
                    | CanonicalMirError::OperationOperandTypeMismatch { .. }
            ),
            "{error:?}"
        );
    }
}

#[test]
fn canonical_sum_declaration_variant_has_independent_instance_payloads() {
    let first = NominalInstanceId::from_index(0).unwrap();
    let second = NominalInstanceId::from_index(1).unwrap();
    let variant = VariantId::from_index(0).unwrap();
    let object = MirValueId::from_index(0).unwrap();
    let boolean = TypeId::from_index(1).unwrap();
    let integer = TypeId::from_index(4).unwrap();
    let operation = CanonicalOperation::SumPayloadGet { nominal: first, variant, object };
    let mut program = instantiated_nominal_operation_program(operation, Some(boolean));
    program.linked.variants.remove(&(second, VariantId::from_index(1).unwrap()));
    program.linked.variants.insert((second, variant), VariantRecord { payload_type: Some(integer) });
    program.validate().expect("同一声明 variant 在两个实例中保留不同 payload，不覆盖第一实例");
    assert_eq!(program.linked.variants[&(first, variant)].payload_type, Some(boolean));
    assert_eq!(program.linked.variants[&(second, variant)].payload_type, Some(integer));
    let instance = ItemInstanceId::from_index(0).unwrap();
    program.mir.functions.get_mut(&instance).unwrap().blocks.get_mut(&CanonicalBlockId(0)).unwrap().instructions[0].operation =
        CanonicalOperation::SumVariantIs { nominal: second, variant, object };
    assert!(matches!(program.validate(), Err(CanonicalMirError::OperationOperandTypeMismatch { .. })));
}

#[test]
fn canonical_sum_registry_rejects_missing_owner_without_an_operation() {
    let mut program = nominal_program();
    let nominal = NominalInstanceId::from_index(9).unwrap();
    let variant = VariantId::from_index(0).unwrap();
    program.linked.variants.insert((nominal, variant), VariantRecord { payload_type: None });
    assert_eq!(program.validate(), Err(CanonicalMirError::InvalidVariantOwner { nominal, variant }));
}

#[test]
fn canonical_nominal_instance_coverage_precedes_operations() {
    let field = FieldId::from_index(0).unwrap();
    let boolean = TypeId::from_index(1).unwrap();
    let object = MirValueId::from_index(0).unwrap();
    let program = instantiated_nominal_operation_program(CanonicalOperation::FieldGet { object, field }, Some(boolean));
    program.validate().expect("两个完整实例均有独立合同");
    let mut missing = program.clone();
    missing.linked.nominal_instances.remove(&NominalInstanceId::from_index(1).unwrap());
    missing.linked.fields.remove(&FieldId::from_index(1).unwrap());
    missing.linked.variants.remove(&(NominalInstanceId::from_index(1).unwrap(), VariantId::from_index(1).unwrap()));
    assert_eq!(missing.validate(), Err(CanonicalMirError::MissingNominalInstance { ty: TypeId::from_index(3).unwrap() }));

    let declaration = TypeId::from_index(0).unwrap();
    let instance = ItemInstanceId::from_index(0).unwrap();
    let mut signature = program.clone();
    signature.linked.item_instances.get_mut(&instance).unwrap().parameter_types[0] = declaration;
    assert_eq!(signature.validate(), Err(CanonicalMirError::MissingNominalInstance { ty: declaration }));
    let mut value = program.clone();
    value.mir.functions.get_mut(&instance).unwrap().value_types.insert(object, declaration);
    assert_eq!(value.validate(), Err(CanonicalMirError::MissingNominalInstance { ty: declaration }));
    let mut field_type = program.clone();
    field_type.linked.fields.get_mut(&field).unwrap().ty = declaration;
    assert_eq!(field_type.validate(), Err(CanonicalMirError::MissingNominalInstance { ty: declaration }));
    let mut payload = program.clone();
    payload.linked.variants.get_mut(&(NominalInstanceId::from_index(0).unwrap(), VariantId::from_index(0).unwrap())).unwrap().payload_type =
        Some(declaration);
    assert_eq!(payload.validate(), Err(CanonicalMirError::MissingNominalInstance { ty: declaration }));

    let wrapper = TypeId::from_index(9).unwrap();
    let shapes = [
        CanonicalTypeKind::Array { element: declaration, length: None },
        CanonicalTypeKind::Array { element: declaration, length: Some(2) },
        CanonicalTypeKind::Tuple(vec![declaration]),
        CanonicalTypeKind::Nullable(declaration),
        CanonicalTypeKind::Union(vec![declaration, boolean]),
        CanonicalTypeKind::Intersection(vec![declaration, boolean]),
        CanonicalTypeKind::Function { parameters: vec![declaration], return_type: boolean },
        CanonicalTypeKind::Function { parameters: vec![boolean], return_type: declaration },
    ];
    for kind in shapes {
        let mut nested = program.clone();
        nested.linked.types.insert(wrapper, TypeRecord { declaration: wrapper, kind });
        assert_eq!(nested.validate(), Err(CanonicalMirError::MissingNominalInstance { ty: declaration }));
    }
}

#[test]
fn canonical_recursive_nominal_contract_does_not_expand_types() {
    let mut program = nominal_program();
    let declaration = TypeId::from_index(0).unwrap();
    program.linked.fields.get_mut(&FieldId::from_index(0).unwrap()).unwrap().ty = declaration;
    program.validate().expect("递归字段只验证现有闭包，不递归实例化或展开布局");
}

#[test]
fn canonical_nominal_instance_type_is_required_and_unique() {
    let nominal = NominalInstanceId::from_index(0).unwrap();
    let mut program = nominal_program();
    let unknown = TypeId::from_index(9).unwrap();
    program.linked.nominal_instances.get_mut(&nominal).unwrap().ty = unknown;
    assert_eq!(program.validate(), Err(CanonicalMirError::InvalidNominalInstanceType { nominal, ty: unknown }));
    let mut program = nominal_program();
    program.linked.nominal_instances.insert(NominalInstanceId::from_index(1).unwrap(), program.linked.nominal_instances[&nominal].clone());
    assert_eq!(program.validate(), Err(CanonicalMirError::DuplicateNominalInstanceType { ty: TypeId::from_index(0).unwrap() }));
}

#[test]
fn canonical_call_requires_instantiated_signature() {
    let mut program = typed_call_program();
    program.validate().expect("完整实例化调用合同");
    if let CanonicalOperation::Invoke { arguments, .. } =
        &mut program.mir.functions.values_mut().next().unwrap().blocks.get_mut(&CanonicalBlockId(0)).unwrap().instructions[0].operation
    {
        arguments.clear();
    }
    assert!(matches!(program.validate(), Err(CanonicalMirError::CallArityMismatch { .. })));
}

#[test]
fn canonical_surface_requires_a_body_for_each_export_and_entry() {
    let caller = ItemInstanceId::from_index(0).unwrap();
    let external = ItemInstanceId::from_index(1).unwrap();
    let mut program = typed_call_program();
    program.linked.exports.insert(caller, ExportRecord { exported_name: "public_answer".into() });
    program.linked.entries.insert(caller, EntryRecord);
    program.validate().expect("完整公开合同");
    program.linked.exports.insert(external, ExportRecord { exported_name: "external_answer".into() });
    assert_eq!(program.validate(), Err(CanonicalMirError::MissingSurfaceBody { function: external }));
    program.linked.exports.remove(&external);
    program.linked.entries.insert(external, EntryRecord);
    assert_eq!(program.validate(), Err(CanonicalMirError::MissingSurfaceBody { function: external }));
    program.linked.entries.remove(&external);
    let unknown = ItemInstanceId::from_index(9).unwrap();
    program.linked.entries.insert(unknown, EntryRecord);
    assert_eq!(program.validate(), Err(CanonicalMirError::MissingSurfaceBody { function: unknown }));
}

#[test]
fn canonical_surface_rejects_duplicate_public_names_but_allows_multiple_exports() {
    let caller = ItemInstanceId::from_index(0).unwrap();
    let other = ItemInstanceId::from_index(1).unwrap();
    let mut program = typed_call_program();
    let mut function = program.mir.functions[&caller].clone();
    function.instance = other;
    program.mir.functions.insert(other, function);
    program.linked.exports.insert(caller, ExportRecord { exported_name: "first_answer".into() });
    program.linked.exports.insert(other, ExportRecord { exported_name: "second_answer".into() });
    program.validate().expect("多导出完整合同");
    program.linked.exports.get_mut(&other).unwrap().exported_name = "first_answer".into();
    assert_eq!(program.validate(), Err(CanonicalMirError::DuplicateExportName { name: "first_answer".into() }));
}

#[test]
fn compiled_program_requires_complete_representation_plan() {
    let program = typed_call_program();
    let error = CompiledProgram::new(program, RepresentationPlan::default()).expect_err("缺少值表示必须在处理边界失败");
    assert!(matches!(error, CompiledProgramError::MissingValueRepresentation { .. }));
}

#[test]
fn compiled_program_keeps_canonical_and_representation_as_one_success_value() {
    let program = typed_call_program();
    let caller = ItemInstanceId::from_index(0).unwrap();
    let argument = MirValueId::from_index(0).unwrap();
    let result = MirValueId::from_index(1).unwrap();
    let mut representation = RepresentationPlan::default();
    for value in [argument, result] {
        representation
            .value_reps
            .insert(ValueIdentity::new(caller, value), nyar_types::semantic_ids::layout_choice::ValueRepresentation::Specialized);
    }
    representation
        .invoke_lowerings
        .insert(InstructionId::from_index(0).unwrap(), nyar_types::semantic_ids::layout_choice::InvokeLowering::Direct);
    let compiled = CompiledProgram::new(program.clone(), representation.clone()).expect("完整处理载荷必须成功");
    assert_eq!(compiled.canonical(), &program);
    assert_eq!(compiled.representation(), &representation);
}

#[test]
fn compiled_program_rejects_representation_for_unknown_value() {
    let program = typed_call_program();
    let caller = ItemInstanceId::from_index(0).unwrap();
    let mut representation = RepresentationPlan::default();
    representation.value_reps.insert(
        ValueIdentity::new(caller, MirValueId::from_index(0).unwrap()),
        nyar_types::semantic_ids::layout_choice::ValueRepresentation::Specialized,
    );
    representation.value_reps.insert(
        ValueIdentity::new(caller, MirValueId::from_index(1).unwrap()),
        nyar_types::semantic_ids::layout_choice::ValueRepresentation::Specialized,
    );
    representation.value_reps.insert(
        ValueIdentity::new(caller, MirValueId::from_index(2).unwrap()),
        nyar_types::semantic_ids::layout_choice::ValueRepresentation::Specialized,
    );
    representation
        .invoke_lowerings
        .insert(InstructionId::from_index(0).unwrap(), nyar_types::semantic_ids::layout_choice::InvokeLowering::Direct);
    let error = CompiledProgram::new(program, representation).expect_err("未知值不得进入后端");
    assert!(matches!(error, CompiledProgramError::ExtraValueRepresentation { .. }));
}

#[test]
fn compiled_program_rejects_representation_for_unknown_instruction() {
    let program = typed_call_program();
    let caller = ItemInstanceId::from_index(0).unwrap();
    let mut representation = RepresentationPlan::default();
    for value in [MirValueId::from_index(0).unwrap(), MirValueId::from_index(1).unwrap()] {
        representation
            .value_reps
            .insert(ValueIdentity::new(caller, value), nyar_types::semantic_ids::layout_choice::ValueRepresentation::Specialized);
    }
    representation
        .invoke_lowerings
        .insert(InstructionId::from_index(0).unwrap(), nyar_types::semantic_ids::layout_choice::InvokeLowering::Direct);
    representation
        .invoke_lowerings
        .insert(InstructionId::from_index(1).unwrap(), nyar_types::semantic_ids::layout_choice::InvokeLowering::Direct);
    let error = CompiledProgram::new(program, representation).expect_err("未知调用指令不得进入后端");
    assert!(matches!(error, CompiledProgramError::ExtraInvokeRepresentation { .. }));
}

#[test]
fn canonical_call_rejects_argument_type_mismatch() {
    let mut program = typed_call_program();
    let other = TypeId::from_index(1).unwrap();
    program.linked.types.insert(other, TypeRecord { declaration: other, kind: CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Bool) });
    program.linked.item_instances.get_mut(&ItemInstanceId::from_index(1).unwrap()).unwrap().parameter_types[0] = other;
    assert!(matches!(program.validate(), Err(CanonicalMirError::CallArgumentTypeMismatch { .. })));
}

#[test]
fn canonical_call_rejects_missing_result_type() {
    let mut program = typed_call_program();
    program.mir.functions.values_mut().next().unwrap().value_types.remove(&MirValueId::from_index(1).unwrap());
    assert!(matches!(program.validate(), Err(CanonicalMirError::CallResultTypeMismatch { .. })));
}

#[test]
fn canonical_function_requires_declared_entry_signature() {
    let mut program = typed_call_program();
    program.mir.functions.values_mut().next().unwrap().parameters.clear();
    assert!(matches!(program.validate(), Err(CanonicalMirError::FunctionSignatureMismatch { .. })));
}

#[test]
fn canonical_call_rejects_unlinked_signature_type() {
    let mut program = typed_call_program();
    program.linked.item_instances.get_mut(&ItemInstanceId::from_index(1).unwrap()).unwrap().return_type = TypeId::from_index(9).unwrap();
    assert!(matches!(program.validate(), Err(CanonicalMirError::UnknownType { .. })));
}

fn typed_function_value_program() -> CanonicalProgram {
    let caller = ItemInstanceId::from_index(0).unwrap();
    let function_type = TypeId::from_index(0).unwrap();
    let value_type = TypeId::from_index(1).unwrap();
    let function_value = MirValueId::from_index(0).unwrap();
    let argument = MirValueId::from_index(1).unwrap();
    let result = MirValueId::from_index(2).unwrap();
    let mut linked = LinkedSemanticProgram::default();
    linked.types.insert(
        function_type,
        TypeRecord { declaration: function_type, kind: CanonicalTypeKind::Function { parameters: vec![value_type], return_type: value_type } },
    );
    linked.types.insert(
        value_type,
        TypeRecord { declaration: value_type, kind: CanonicalTypeKind::Primitive(CanonicalPrimitiveType::Integer { bits: 32, signed: true }) },
    );
    linked.item_instances.insert(
        caller,
        ItemInstanceRecord {
            declaration: ItemId::from_index(0).unwrap(),
            parameter_types: vec![function_type, value_type],
            return_type: value_type,
        },
    );
    let function = CanonicalFunction {
        instance: caller,
        parameters: vec![(function_value, function_type), (argument, value_type)],
        return_type: value_type,
        value_types: BTreeMap::from([(function_value, function_type), (argument, value_type), (result, value_type)]),
        entry: CanonicalBlockId(0),
        blocks: BTreeMap::from([(
            CanonicalBlockId(0),
            CanonicalBlock {
                id: CanonicalBlockId(0),
                parameters: Vec::new(),
                instructions: vec![CanonicalInstruction {
                    id: InstructionId::from_index(0).unwrap(),
                    results: vec![result],
                    operation: CanonicalOperation::Invoke { callee: CanonicalCallee::Value(function_value), arguments: vec![argument] },
                }],
                terminator: CanonicalTerminator::Return { value: Some(result) },
            },
        )]),
    };
    CanonicalProgram {
        linked,
        mir: CanonicalSemanticMir { module_name: "function-value".into(), functions: BTreeMap::from([(caller, function)]) },
    }
}

#[test]
fn canonical_function_value_call_preserves_function_type() {
    typed_function_value_program().validate().expect("函数值调用必须通过完整语义合同");
}

#[test]
fn canonical_function_value_call_rejects_scalar_callee() {
    let mut program = typed_function_value_program();
    let function = program.mir.functions.values_mut().next().unwrap();
    let value = MirValueId::from_index(0).unwrap();
    function.value_types.insert(value, TypeId::from_index(1).unwrap());
    function.parameters[0].1 = TypeId::from_index(1).unwrap();
    program.linked.item_instances.get_mut(&ItemInstanceId::from_index(0).unwrap()).unwrap().parameter_types[0] = TypeId::from_index(1).unwrap();
    assert!(matches!(program.validate(), Err(CanonicalMirError::ValueCalleeNotFunction { value: rejected, .. }) if rejected == value));
}

#[test]
fn canonical_function_value_call_rejects_argument_contract() {
    let mut program = typed_function_value_program();
    let wrong = MirValueId::from_index(0).unwrap();
    if let CanonicalOperation::Invoke { arguments, .. } =
        &mut program.mir.functions.values_mut().next().unwrap().blocks.get_mut(&CanonicalBlockId(0)).unwrap().instructions[0].operation
    {
        *arguments = vec![wrong, wrong];
    }
    assert!(matches!(program.validate(), Err(CanonicalMirError::ValueCalleeArityMismatch { .. })));
}

#[test]
fn canonical_function_value_callee_is_an_ssa_use() {
    let mut program = typed_function_value_program();
    let function = program.mir.functions.values_mut().next().unwrap();
    function.parameters = vec![(MirValueId::from_index(1).unwrap(), TypeId::from_index(1).unwrap())];
    program.linked.item_instances.get_mut(&ItemInstanceId::from_index(0).unwrap()).unwrap().parameter_types =
        vec![TypeId::from_index(1).unwrap()];
    assert!(
        matches!(program.validate(), Err(CanonicalMirError::UseBeforeDefinition { value, .. }) if value == MirValueId::from_index(0).unwrap())
    );
}

#[test]
fn stage_order_is_one_way() {
    assert_eq!(pipeline::STAGE_ORDER[0], CompileStage::Ast);
    assert_eq!(*pipeline::STAGE_ORDER.last().unwrap(), CompileStage::Emit);
}
