use super::*;
use crate::{
    contracts::{
        Block, BlockRef, Constant, DispatchKind, ExecutableFunction, Instruction, InstructionKind, IntrinsicCompareOp, IntrinsicOpcode,
        Operand, ReceiverPassingKind, StorageKind, Terminator, Value, ValueOrigin, ValueRef,
    },
    executable_provider::MirFunctionMapProvider,
    testing::field_slot_index,
};
use nyar::{Identifier, NamePath, NyarType, QualifiedName};
use nyar_types::{AggregateLayout, AggregateLayoutPlan, FieldLayout, SingletonInstancePlan};
use std::sync::Arc;

#[test]
fn point_field_slots_use_jvm_width_not_field_index() {
    let mut submission = FragmentSubmission::default();
    submission.aggregate_layouts = AggregateLayoutPlan {
        layouts: vec![AggregateLayout {
            id: 1,
            name: "Point".to_string(),
            namespace: String::new(),
            storage: StorageKind::Value,
            size: 16,
            align: 8,
            fields: vec![
                FieldLayout { name: "x".to_string(), ty: nyar::NyarType::Integer64 { signed: true }, offset: 0, size: 8, align: 8 },
                FieldLayout { name: "y".to_string(), ty: nyar::NyarType::Integer64 { signed: true }, offset: 8, size: 8, align: 8 },
            ],
        }],
        value_type_names: ["Point".to_string()].into_iter().collect(),
        type_name_to_layout: [("Point".to_string(), 1)].into_iter().collect(),
    };
    assert_eq!(field_slot_index(&submission, Some(1), "Point", "x"), 0);
    assert_eq!(field_slot_index(&submission, Some(1), "Point", "y"), 2);
}

#[test]
fn jvm_wide_local_rejects_adjacent_reference_high_slot() {
    let mut kinds = std::collections::BTreeMap::new();
    kinds.insert(29, JvmLocalKind::Reference);

    assert!(jvm_local_slot_conflicts(&kinds, 28, JvmLocalKind::Long, 2), "lstore 28 must not overwrite a reference planned at slot 29");

    kinds.insert(28, JvmLocalKind::Long);
    kinds.insert(29, JvmLocalKind::Long);
    assert!(!jvm_local_slot_conflicts(&kinds, 28, JvmLocalKind::Long, 2), "a repeated lstore to its own two-slot local remains valid");
}

#[test]
fn jvm_lowering_stores_point_y_at_slot_two() {
    let mut submission = FragmentSubmission::default();
    submission.aggregate_layouts = AggregateLayoutPlan {
        layouts: vec![AggregateLayout {
            id: 1,
            name: "Point".to_string(),
            namespace: String::new(),
            storage: StorageKind::Value,
            size: 16,
            align: 8,
            fields: vec![
                FieldLayout { name: "x".to_string(), ty: nyar::NyarType::Integer64 { signed: true }, offset: 0, size: 8, align: 8 },
                FieldLayout { name: "y".to_string(), ty: nyar::NyarType::Integer64 { signed: true }, offset: 8, size: 8, align: 8 },
            ],
        }],
        value_type_names: ["Point".to_string()].into_iter().collect(),
        type_name_to_layout: [("Point".to_string(), 1)].into_iter().collect(),
    };
    let operation = QualifiedName::new(vec![Identifier::new("main")]);
    let mir_fn = ExecutableFunction {
        symbol: "main".to_string(),
        return_type: NyarType::Named(Identifier::new("Point")),
        param_types: Vec::new(),
        value_types: Default::default(),
        entry: BlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![Instruction {
                output: Some(ValueRef(0)),
                kind: InstructionKind::StructNew {
                    type_name: "Point".to_string(),
                    storage: StorageKind::Value,
                    layout_id: Some(1),
                    fields: vec![
                        ("x".to_string(), Operand::Constant(Constant::Int(1))),
                        ("y".to_string(), Operand::Constant(Constant::Int(2))),
                    ],
                },
            }],
            terminator: Terminator::Return { value: Some(Operand::Value(ValueRef(0))) },
        }],
        diagnostics: Vec::new(),
    };
    let method = lower_mir_function_to_jvm(&submission, &operation, &mir_fn);
    let code = method.code.expect("code");
    assert!(code.instructions.iter().any(|ins| matches!(ins, JvmInstruction::LStore(2))));
}

#[test]
fn jvm_call_lowering_handles_non_builtin_call() {
    let operation = QualifiedName::new(vec![Identifier::new("main")]);
    let add_op = QualifiedName::new(vec![Identifier::new("add")]);
    let add_fn = ExecutableFunction {
        symbol: "add".to_string(),
        return_type: NyarType::Float64,
        param_types: vec![NyarType::Float64, NyarType::Float64],
        value_types: Default::default(),
        entry: BlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: Terminator::Return { value: Some(Operand::Constant(Constant::Float64(ordered_float::OrderedFloat(0.0)))) },
        }],
        diagnostics: Vec::new(),
    };
    let main_fn = ExecutableFunction {
        symbol: "main".to_string(),
        return_type: NyarType::Float64,
        param_types: Vec::new(),
        value_types: Default::default(),
        entry: BlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![Instruction {
                output: Some(ValueRef(0)),
                kind: InstructionKind::Call {
                    dispatch: DispatchKind::Static,
                    callee: Operand::Symbol(NamePath::new(vec![Identifier::new("add")])),
                    arguments: vec![
                        Operand::Constant(Constant::Float64(ordered_float::OrderedFloat(1.0))),
                        Operand::Constant(Constant::Float64(ordered_float::OrderedFloat(2.0))),
                    ],
                    witness: None,
                    effect: None,
                    receiver_kind: None,
                    parameter_types: None,
                    intrinsic_opcode: None,
                },
            }],
            terminator: Terminator::Return { value: Some(Operand::Value(ValueRef(0))) },
        }],
        diagnostics: Vec::new(),
    };
    let mut submission = FragmentSubmission::default();
    submission.module_name = "demo".to_string();
    submission.fragment_id = Identifier::new("main");
    submission.executable =
        Some(Arc::new(MirFunctionMapProvider::new([(operation.clone(), main_fn.clone()), (add_op, add_fn)].into_iter().collect())));
    let method = lower_mir_function_to_jvm(&submission, &operation, &main_fn);
    let code = method.code.expect("code");
    assert!(code.instructions.iter().any(|ins| matches!(ins, JvmInstruction::InvokeStatic(_))), "non-builtin call should emit invokestatic");
}

#[test]
fn jvm_value_receiver_call_marks_by_address() {
    let mut submission = FragmentSubmission::default();
    submission.module_name = "demo".to_string();
    submission.fragment_id = Identifier::new("main");
    submission.aggregate_layouts = AggregateLayoutPlan {
        layouts: vec![AggregateLayout {
            id: 1,
            name: "Point".to_string(),
            namespace: String::new(),
            storage: StorageKind::Value,
            size: 16,
            align: 8,
            fields: vec![
                FieldLayout { name: "x".to_string(), ty: nyar::NyarType::Integer64 { signed: true }, offset: 0, size: 8, align: 8 },
                FieldLayout { name: "y".to_string(), ty: nyar::NyarType::Integer64 { signed: true }, offset: 8, size: 8, align: 8 },
            ],
        }],
        value_type_names: ["Point".to_string()].into_iter().collect(),
        type_name_to_layout: [("Point".to_string(), 1)].into_iter().collect(),
    };
    let operation = QualifiedName::new(vec![Identifier::new("main")]);
    let mir_fn = ExecutableFunction {
        symbol: "main".to_string(),
        return_type: NyarType::Unit,
        param_types: Vec::new(),
        value_types: [(ValueRef(0), NyarType::Named(Identifier::new("Point")))].into_iter().collect(),
        entry: BlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![
                Instruction {
                    output: Some(ValueRef(0)),
                    kind: InstructionKind::StructNew {
                        type_name: "Point".to_string(),
                        storage: StorageKind::Value,
                        layout_id: Some(1),
                        fields: vec![
                            ("x".to_string(), Operand::Constant(Constant::Int(1))),
                            ("y".to_string(), Operand::Constant(Constant::Int(2))),
                        ],
                    },
                },
                Instruction {
                    output: None,
                    kind: InstructionKind::Call {
                        dispatch: DispatchKind::Static,
                        callee: Operand::Symbol(nyar::NamePath::new(vec![Identifier::new("consume")])),
                        arguments: vec![Operand::Value(ValueRef(0))],
                        witness: None,
                        effect: None,
                        receiver_kind: Some(ReceiverPassingKind::ByAddress),
                        parameter_types: None,
                        intrinsic_opcode: None,
                    },
                },
            ],
            terminator: Terminator::Return { value: None },
        }],
        diagnostics: Vec::new(),
    };
    let method = lower_mir_function_to_jvm(&submission, &operation, &mir_fn);
    let code = method.code.expect("code");
    // ByAddress 璺緞锛氭帴鏀惰€呰鍘嬫爤鍚庡彂鍑?invokestatic銆?
    assert!(
        code.instructions.iter().any(|ins| matches!(ins, JvmInstruction::InvokeStatic(_))),
        "by-address receiver call should emit invokestatic"
    );
}

#[test]
fn jvm_singleton_accessor_call_emits_invokestatic() {
    let mut submission = FragmentSubmission::default();
    submission.singleton_instances = vec![SingletonInstancePlan {
        name: "Counter".to_string(),
        namespace: String::new(),
        instance_field: "INSTANCE".to_string(),
        is_lazy: false,
        constructor_symbol: None,
        finalizer_symbol: None,
    }];
    let operation = QualifiedName::new(vec![Identifier::new("main")]);
    let mir_fn = ExecutableFunction {
        symbol: "main".to_string(),
        return_type: NyarType::Named(Identifier::new("Counter")),
        param_types: Vec::new(),
        value_types: [(ValueRef(0), NyarType::Named(Identifier::new("Counter")))].into_iter().collect(),
        entry: BlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![Instruction {
                output: Some(ValueRef(0)),
                kind: InstructionKind::Call {
                    dispatch: DispatchKind::Static,
                    callee: Operand::Symbol(nyar::NamePath::new(vec![Identifier::new("Counter"), Identifier::new("instance")])),
                    arguments: Vec::new(),
                    witness: None,
                    effect: None,
                    receiver_kind: None,
                    parameter_types: None,
                    intrinsic_opcode: None,
                },
            }],
            terminator: Terminator::Return { value: Some(Operand::Value(ValueRef(0))) },
        }],
        diagnostics: Vec::new(),
    };
    let method = lower_mir_function_to_jvm(&submission, &operation, &mir_fn);
    let code = method.code.expect("code");
    let has_accessor = code
        .instructions
        .iter()
        .any(|ins| matches!(ins, JvmInstruction::InvokeStatic(method_ref) if method_ref.owner == "Counter" && method_ref.name == "instance"));
    assert!(has_accessor, "expected InvokeStatic Counter.instance, got {:?}", code.instructions);
}

#[test]
fn jvm_singleton_instance_method_call_emits_invokevirtual() {
    let mut submission = FragmentSubmission::default();
    submission.singleton_instances = vec![SingletonInstancePlan {
        name: "Counter".to_string(),
        namespace: String::new(),
        instance_field: "INSTANCE".to_string(),
        is_lazy: false,
        constructor_symbol: None,
        finalizer_symbol: None,
    }];
    let increment_key = QualifiedName::new(vec![Identifier::new("Counter.increment")]);
    let increment_fn = ExecutableFunction {
        symbol: "Counter.increment".to_string(),
        return_type: NyarType::Integer64 { signed: true },
        param_types: vec![NyarType::Named(Identifier::new("Counter"))],
        value_types: Default::default(),
        entry: BlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: Vec::new(),
        diagnostics: Vec::new(),
    };
    let mut mir_map: std::collections::BTreeMap<QualifiedName, crate::executable_provider::ExecutableFunction> = Default::default();
    mir_map.insert(increment_key, increment_fn);

    let operation = QualifiedName::new(vec![Identifier::new("main")]);
    let value_types =
        [(ValueRef(0), NyarType::Named(Identifier::new("Counter"))), (ValueRef(1), NyarType::Integer64 { signed: true })].into_iter().collect();
    let mir_fn = ExecutableFunction {
        symbol: "main".to_string(),
        return_type: NyarType::Integer64 { signed: true },
        param_types: Vec::new(),
        value_types,
        entry: BlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![
                Instruction {
                    output: Some(ValueRef(0)),
                    kind: InstructionKind::Call {
                        dispatch: DispatchKind::Static,
                        callee: Operand::Symbol(nyar::NamePath::new(vec![Identifier::new("Counter"), Identifier::new("instance")])),
                        arguments: Vec::new(),
                        witness: None,
                        effect: None,
                        receiver_kind: None,
                        parameter_types: None,
                        intrinsic_opcode: None,
                    },
                },
                Instruction {
                    output: Some(ValueRef(1)),
                    kind: InstructionKind::Call {
                        dispatch: DispatchKind::Static,
                        callee: Operand::Symbol(nyar::NamePath::new(vec![Identifier::new("Counter"), Identifier::new("increment")])),
                        arguments: vec![Operand::Value(ValueRef(0))],
                        witness: None,
                        effect: None,
                        receiver_kind: Some(ReceiverPassingKind::ByAddress),
                        parameter_types: None,
                        intrinsic_opcode: None,
                    },
                },
            ],
            terminator: Terminator::Return { value: Some(Operand::Value(ValueRef(1))) },
        }],
        diagnostics: Vec::new(),
    };
    mir_map.insert(operation.clone(), mir_fn.clone());
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(mir_map)));
    let method = lower_mir_function_to_jvm(&submission, &operation, &mir_fn);
    let code = method.code.expect("code");
    let has_accessor = code
        .instructions
        .iter()
        .any(|ins| matches!(ins, JvmInstruction::InvokeStatic(method_ref) if method_ref.owner == "Counter" && method_ref.name == "instance"));
    assert!(has_accessor, "expected InvokeStatic Counter.instance, got {:?}", code.instructions);
    let has_method = code
        .instructions
        .iter()
        .any(|ins| matches!(ins, JvmInstruction::InvokeVirtual(method_ref) if method_ref.owner == "Counter" && method_ref.name == "increment"));
    assert!(has_method, "expected InvokeVirtual Counter.increment, got {:?}", code.instructions);
}

#[test]
fn jvm_reference_struct_new_uses_object_field_descriptor_for_utf8() {
    let mut submission = FragmentSubmission::default();
    submission.aggregate_layouts = AggregateLayoutPlan {
        layouts: vec![AggregateLayout {
            id: 2,
            name: "Widget".to_string(),
            namespace: String::new(),
            storage: StorageKind::Reference,
            size: 8,
            align: 8,
            fields: vec![FieldLayout { name: "label".to_string(), ty: nyar::NyarType::Utf8, offset: 0, size: 8, align: 8 }],
        }],
        value_type_names: Default::default(),
        type_name_to_layout: [("Widget".to_string(), 2)].into_iter().collect(),
    };
    let operation = QualifiedName::new(vec![Identifier::new("main")]);
    let mir_fn = ExecutableFunction {
        symbol: "main".to_string(),
        return_type: NyarType::Named(Identifier::new("Widget")),
        param_types: Vec::new(),
        value_types: [(ValueRef(0), NyarType::Named(Identifier::new("Widget")))].into_iter().collect(),
        entry: BlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![Instruction {
                output: Some(ValueRef(0)),
                kind: InstructionKind::StructNew {
                    type_name: "Widget".to_string(),
                    storage: StorageKind::Reference,
                    layout_id: Some(2),
                    fields: vec![("label".to_string(), Operand::Constant(Constant::Utf8("suspend_ok".to_string())))],
                },
            }],
            terminator: Terminator::Return { value: Some(Operand::Value(ValueRef(0))) },
        }],
        diagnostics: Vec::new(),
    };

    let method = lower_mir_function_to_jvm(&submission, &operation, &mir_fn);
    let code = method.code.expect("code");

    assert!(
        code.instructions.iter().any(|ins| matches!(ins, JvmInstruction::LdcString(value) if value == "suspend_ok")),
        "expected utf8 payload to lower as JVM string constant"
    );
    assert!(
        code.instructions.iter().any(|ins| matches!(
            ins,
            JvmInstruction::PutField(field)
                if field.owner == "Widget"
                    && field.name == "label"
                    && field.descriptor == JvmTypeDescriptor::Object("java/lang/String".to_string())
        )),
        "reference StructNew must emit PutField with java/lang/String descriptor"
    );
    assert!(
        code.instructions.iter().any(|ins| matches!(ins, JvmInstruction::AStore(_))),
        "reference StructNew result must stay on reference locals via AStore"
    );
}

#[test]
fn jvm_parameters_keep_abi_slots_and_temporaries_start_after_them() {
    let submission = FragmentSubmission::default();
    let operation = QualifiedName::new(vec![Identifier::new("copy_text")]);
    let parameter = ValueRef(0);
    let temporary = ValueRef(1);
    let mir_fn = ExecutableFunction {
        symbol: "copy_text".to_string(),
        return_type: NyarType::Utf8,
        param_types: vec![NyarType::Utf8],
        value_types: [(parameter, NyarType::Utf8), (temporary, NyarType::Utf8)].into_iter().collect(),
        entry: BlockRef(0),
        values: vec![Value { id: parameter, origin: ValueOrigin::Parameter { index: 0, name: "value".to_string() } }],
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![Instruction { output: Some(temporary), kind: InstructionKind::Copy { source: Operand::Value(parameter) } }],
            terminator: Terminator::Return { value: Some(Operand::Value(temporary)) },
        }],
        diagnostics: Vec::new(),
    };

    let method = lower_mir_function_to_jvm(&submission, &operation, &mir_fn);
    let code = method.code.expect("code");
    assert!(code.instructions.contains(&JvmInstruction::ALoad(0)), "parameter must load from ABI local 0");
    assert!(
        code.instructions.iter().any(|instruction| matches!(instruction, JvmInstruction::AStore(local) if *local > 0)),
        "temporary must not overwrite parameter local 0: {:?}",
        code.instructions
    );
    assert!(code.max_locals >= 2);
}

#[test]
fn jvm_initializes_non_parameter_locals_with_stable_verifier_types() {
    let submission = FragmentSubmission::default();
    let operation = QualifiedName::new(vec![Identifier::new("select_text")]);
    let text = ValueRef(0);
    let mir_fn = ExecutableFunction {
        symbol: "select_text".to_string(),
        return_type: NyarType::Utf8,
        param_types: Vec::new(),
        value_types: [(text, NyarType::Utf8)].into_iter().collect(),
        entry: BlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![Instruction {
                output: Some(text),
                kind: InstructionKind::LoadConstant { constant: Constant::Utf8("ok".to_string()), ty: Some(NyarType::Utf8) },
            }],
            terminator: Terminator::Return { value: Some(Operand::Value(text)) },
        }],
        diagnostics: Vec::new(),
    };

    let method = lower_mir_function_to_jvm(&submission, &operation, &mir_fn);
    let code = method.code.expect("code");
    assert!(
        matches!(code.instructions.as_slice(), [JvmInstruction::AConstNull, JvmInstruction::AStore(0), ..]),
        "reference locals must be verifier-initialized at method entry: {:?}",
        code.instructions
    );
}

#[test]
fn i64_zero_one_constants_use_category_two_jvm_opcodes() {
    let submission = FragmentSubmission::default();
    let operation = QualifiedName::new(vec![Identifier::new("long_constants")]);
    let zero = ValueRef(0);
    let one = ValueRef(1);
    let i64_ty = NyarType::Integer64 { signed: true };
    let mir_fn = ExecutableFunction {
        symbol: "long_constants".to_string(),
        return_type: i64_ty.clone(),
        param_types: Vec::new(),
        value_types: [(zero, i64_ty.clone()), (one, i64_ty.clone())].into_iter().collect(),
        entry: BlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![
                Instruction {
                    output: Some(zero),
                    kind: InstructionKind::LoadConstant { constant: Constant::Int(0), ty: Some(i64_ty.clone()) },
                },
                Instruction { output: Some(one), kind: InstructionKind::LoadConstant { constant: Constant::Int(1), ty: Some(i64_ty.clone()) } },
            ],
            terminator: Terminator::Return { value: Some(Operand::Value(one)) },
        }],
        diagnostics: Vec::new(),
    };

    let method = lower_mir_function_to_jvm(&submission, &operation, &mir_fn);
    let code = method.code.expect("code");
    assert!(code.instructions.iter().any(|ins| matches!(ins, JvmInstruction::LConst0)));
    assert!(code.instructions.iter().any(|ins| matches!(ins, JvmInstruction::LConst1)), "i64 one must use lconst_1: {:?}", code.instructions);
    assert!(code.instructions.iter().any(|ins| matches!(ins, JvmInstruction::LReturn)));
}

/// Same named StoreVar home must not see both `astore` and `istore` (VerifyError
/// "Register N contains wrong type"). Later Int assign reallocates away from Utf8.
#[test]
fn store_var_int_after_string_reallocates_local() {
    let submission = FragmentSubmission::default();
    let operation = QualifiedName::new(vec![Identifier::new("reuse_slot")]);
    let mir_fn = ExecutableFunction {
        symbol: "reuse_slot".to_string(),
        return_type: NyarType::Integer32 { signed: true },
        param_types: Vec::new(),
        value_types: Default::default(),
        entry: BlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![
                Instruction {
                    output: None,
                    kind: InstructionKind::StoreVar {
                        name: "x".to_string(),
                        value: Operand::Constant(Constant::Utf8("s".to_string())),
                        ty: Some(NyarType::Utf8),
                    },
                },
                Instruction {
                    output: None,
                    kind: InstructionKind::StoreVar {
                        name: "x".to_string(),
                        value: Operand::Constant(Constant::Int(1)),
                        // Polluted annotation: still Utf8 — value inference must win.
                        ty: Some(NyarType::Utf8),
                    },
                },
            ],
            terminator: Terminator::Return { value: Some(Operand::Constant(Constant::Int(0))) },
        }],
        diagnostics: Vec::new(),
    };

    let method = lower_mir_function_to_jvm(&submission, &operation, &mir_fn);
    let code = method.code.expect("code");
    let astore_slots: Vec<u16> = code
        .instructions
        .iter()
        .filter_map(|ins| match ins {
            JvmInstruction::AStore(local) => Some(*local),
            _ => None,
        })
        .collect();
    let istore_slots: Vec<u16> = code
        .instructions
        .iter()
        .filter_map(|ins| match ins {
            JvmInstruction::IStore(local) => Some(*local),
            _ => None,
        })
        .collect();
    assert!(!astore_slots.is_empty(), "expected astore for string assign: {:?}", code.instructions);
    assert!(
        istore_slots.iter().any(|slot| !astore_slots.contains(slot)),
        "int assign must realloc away from string local; astore={astore_slots:?} istore={istore_slots:?} {:?}",
        code.instructions
    );
    for slot in &astore_slots {
        assert!(!istore_slots.contains(slot), "local {slot} must not mix astore/istore: {:?}", code.instructions);
    }
}

/// `enums` / `unite` 在 JVM ABI 上是 int 句柄：返回描述符必须是 `I`，终止符用
/// `ireturn`，禁止 `iconst` + `checkcast`/`areturn`（会触发
/// `VerifyError: Expecting to find object/array on stack`）。
#[test]
fn enum_sum_type_returns_int_handle_not_areturn() {
    use nyar_types::{SumTypeLayout, SumVariantLayout};

    let mut submission = FragmentSubmission::default();
    submission.sum_types = vec![SumTypeLayout {
        name: "WitWasiCoreResultKind".to_string(),
        is_unite: false,
        tag_width: 4,
        variants: vec![
            SumVariantLayout { name: "ResultVoid".to_string(), tag: 0, payload_type: None },
            SumVariantLayout { name: "ResultI64".to_string(), tag: 2, payload_type: None },
        ],
    }];

    let operation = QualifiedName::new(vec![Identifier::new("wit_wasi_core_result_i64")]);
    let mir_fn = ExecutableFunction {
        symbol: "wit_wasi_core_result_i64".to_string(),
        return_type: NyarType::Named(Identifier::new("WitWasiCoreResultKind")),
        param_types: Vec::new(),
        value_types: Default::default(),
        entry: BlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: Terminator::Return { value: Some(Operand::Constant(Constant::Int(2))) },
        }],
        diagnostics: Vec::new(),
    };

    let method = lower_mir_function_to_jvm(&submission, &operation, &mir_fn);
    assert_eq!(
        method.descriptor.return_type,
        JvmTypeDescriptor::Int,
        "enums return must be I, not LWitWasiCoreResultKind; {:?}",
        method.descriptor
    );
    let code = method.code.expect("code");
    assert!(
        code.instructions.iter().any(|ins| matches!(ins, JvmInstruction::IReturn)),
        "expected ireturn for enum int-handle: {:?}",
        code.instructions
    );
    assert!(
        !code.instructions.iter().any(|ins| matches!(ins, JvmInstruction::AReturn | JvmInstruction::CheckCast(_))),
        "must not checkcast/areturn enum int-handle: {:?}",
        code.instructions
    );
}

#[test]
fn enum_sum_type_param_is_int_not_object() {
    use nyar_types::{SumTypeLayout, SumVariantLayout};

    let mut submission = FragmentSubmission::default();
    submission.sum_types = vec![SumTypeLayout {
        name: "WasmOpcode".to_string(),
        is_unite: false,
        tag_width: 4,
        variants: vec![SumVariantLayout { name: "I32Xor".to_string(), tag: 0, payload_type: None }],
    }];

    let operation = QualifiedName::new(vec![Identifier::new("wasm_encode_opcode")]);
    let parameter = ValueRef(0);
    let mir_fn = ExecutableFunction {
        symbol: "wasm_encode_opcode".to_string(),
        return_type: NyarType::Array(Box::new(NyarType::Integer32 { signed: true })),
        param_types: vec![NyarType::Named(Identifier::new("WasmOpcode"))],
        value_types: [(parameter, NyarType::Named(Identifier::new("WasmOpcode")))].into_iter().collect(),
        entry: BlockRef(0),
        values: vec![Value { id: parameter, origin: ValueOrigin::Parameter { index: 0, name: "op".to_string() } }],
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: Terminator::Return { value: None },
        }],
        diagnostics: Vec::new(),
    };

    let method = lower_mir_function_to_jvm(&submission, &operation, &mir_fn);
    assert_eq!(
        method.descriptor.parameter_types,
        vec![JvmTypeDescriptor::Int],
        "enums param must be I, not LWasmOpcode; {:?}",
        method.descriptor
    );
}

/// Boxed 多字段值类型作实参时必须按字段 getfield 展开，与 callee flatten ABI 一致。
/// 禁止整对象 `aload` + `checkcast LSig;`（会触发 object/array / Register wrong type）。
#[test]
fn boxed_value_type_call_arg_expands_fields_not_areference() {
    let mut submission = FragmentSubmission::default();
    submission.module_name = "demo".to_string();
    submission.fragment_id = Identifier::new("caller");
    submission.aggregate_layouts = AggregateLayoutPlan {
        layouts: vec![AggregateLayout {
            id: 1,
            name: "MsilMethodSignature".to_string(),
            namespace: String::new(),
            storage: StorageKind::Value,
            size: 16,
            align: 8,
            fields: vec![
                FieldLayout { name: "return_type".to_string(), ty: NyarType::Integer32 { signed: true }, offset: 0, size: 4, align: 4 },
                FieldLayout {
                    name: "parameter_types".to_string(),
                    ty: NyarType::Array(Box::new(NyarType::Integer32 { signed: true })),
                    offset: 4,
                    size: 8,
                    align: 8,
                },
                FieldLayout { name: "has_this".to_string(), ty: NyarType::Boolean, offset: 12, size: 1, align: 1 },
                FieldLayout {
                    name: "parameter_names".to_string(),
                    ty: NyarType::Array(Box::new(NyarType::Utf8)),
                    offset: 13,
                    size: 8,
                    align: 8,
                },
            ],
        }],
        value_type_names: ["MsilMethodSignature".to_string()].into_iter().collect(),
        type_name_to_layout: [("MsilMethodSignature".to_string(), 1)].into_iter().collect(),
    };

    let empty_op = QualifiedName::new(vec![Identifier::new("empty_msil_signature")]);
    let empty_fn = ExecutableFunction {
        symbol: "empty_msil_signature".to_string(),
        return_type: NyarType::Named(Identifier::new("MsilMethodSignature")),
        param_types: Vec::new(),
        value_types: Default::default(),
        entry: BlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: Terminator::Return { value: None },
        }],
        diagnostics: Vec::new(),
    };

    let sig = ValueRef(0);
    let out = ValueRef(1);
    let operation = QualifiedName::new(vec![Identifier::new("caller")]);
    let mir_fn = ExecutableFunction {
        symbol: "caller".to_string(),
        return_type: NyarType::Integer32 { signed: true },
        param_types: Vec::new(),
        value_types: [(sig, NyarType::Named(Identifier::new("MsilMethodSignature"))), (out, NyarType::Integer32 { signed: true })]
            .into_iter()
            .collect(),
        entry: BlockRef(0),
        values: vec![Value { id: sig, origin: ValueOrigin::CallResult }],
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![
                Instruction {
                    output: Some(sig),
                    kind: InstructionKind::Call {
                        dispatch: DispatchKind::Static,
                        callee: Operand::Symbol(NamePath::new(vec![Identifier::new("empty_msil_signature")])),
                        arguments: Vec::new(),
                        witness: None,
                        effect: None,
                        receiver_kind: None,
                        parameter_types: None,
                        intrinsic_opcode: None,
                    },
                },
                Instruction {
                    output: Some(out),
                    kind: InstructionKind::Call {
                        dispatch: DispatchKind::Static,
                        callee: Operand::Symbol(NamePath::new(vec![Identifier::new("Method")])),
                        arguments: vec![
                            Operand::Constant(Constant::Utf8("Owner".to_string())),
                            Operand::Constant(Constant::Utf8("Name".to_string())),
                            Operand::Value(sig),
                        ],
                        witness: None,
                        effect: None,
                        receiver_kind: None,
                        parameter_types: None,
                        intrinsic_opcode: None,
                    },
                },
            ],
            terminator: Terminator::Return { value: Some(Operand::Value(out)) },
        }],
        diagnostics: Vec::new(),
    };

    submission.executable =
        Some(Arc::new(MirFunctionMapProvider::new([(empty_op, empty_fn), (operation.clone(), mir_fn.clone())].into_iter().collect())));

    let method = lower_mir_function_to_jvm(&submission, &operation, &mir_fn);
    let code = method.code.expect("code");
    let getfield_count =
        code.instructions.iter().filter(|ins| matches!(ins, JvmInstruction::GetField(f) if f.owner.contains("MsilMethodSignature"))).count();
    assert!(
        !code.instructions.iter().any(|ins| { matches!(ins, JvmInstruction::CheckCast(c) if c.contains("MsilMethodSignature")) }),
        "must not checkcast whole MsilMethodSignature as call arg: {:?}",
        code.instructions
    );
    assert!(getfield_count >= 4, "boxed value-type arg must expand via getfield (≥4), got {getfield_count}: {:?}", code.instructions);
}

#[test]
fn aggregate_copy_preserves_nested_value_type_array_leaves() {
    let mut submission = FragmentSubmission::default();
    submission.module_name = "demo".to_string();
    submission.fragment_id = Identifier::new("copy");
    let int = NyarType::Integer32 { signed: true };
    let short = NyarType::Integer16 { signed: false };
    let names = NyarType::Array(Box::new(NyarType::Utf8));
    let ids = NyarType::Array(Box::new(short));
    submission.aggregate_layouts = AggregateLayoutPlan {
        layouts: vec![
            AggregateLayout {
                id: 1,
                name: "NestedMap".to_string(),
                namespace: String::new(),
                storage: StorageKind::Value,
                size: 16,
                align: 8,
                fields: vec![
                    FieldLayout { name: "names".to_string(), ty: names, offset: 0, size: 8, align: 8 },
                    FieldLayout { name: "ids".to_string(), ty: ids, offset: 8, size: 8, align: 8 },
                ],
            },
            AggregateLayout {
                id: 2,
                name: "OuterTable".to_string(),
                namespace: String::new(),
                storage: StorageKind::Value,
                size: 24,
                align: 8,
                fields: vec![
                    FieldLayout { name: "head".to_string(), ty: NyarType::Array(Box::new(int)), offset: 0, size: 8, align: 8 },
                    FieldLayout { name: "map".to_string(), ty: NyarType::Named(Identifier::new("NestedMap")), offset: 8, size: 16, align: 8 },
                ],
            },
        ],
        value_type_names: ["NestedMap".to_string(), "OuterTable".to_string()].into_iter().collect(),
        type_name_to_layout: [("NestedMap".to_string(), 1), ("OuterTable".to_string(), 2)].into_iter().collect(),
    };

    let source = ValueRef(0);
    let dest = ValueRef(1);
    let operation = QualifiedName::new(vec![Identifier::new("copy_outer_table")]);
    let outer = NyarType::Named(Identifier::new("OuterTable"));
    let mir_fn = ExecutableFunction {
        symbol: "copy_outer_table".to_string(),
        return_type: NyarType::Unit,
        param_types: vec![outer.clone(), outer],
        value_types: [(source, NyarType::Named(Identifier::new("OuterTable"))), (dest, NyarType::Named(Identifier::new("OuterTable")))]
            .into_iter()
            .collect(),
        entry: BlockRef(0),
        values: vec![
            Value { id: source, origin: ValueOrigin::Parameter { index: 0, name: "source".to_string() } },
            Value { id: dest, origin: ValueOrigin::Parameter { index: 1, name: "dest".to_string() } },
        ],
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![Instruction {
                output: None,
                kind: InstructionKind::AggregateCopy { source: Operand::Value(source), dest: Operand::Value(dest), layout_id: 2 },
            }],
            terminator: Terminator::Return { value: None },
        }],
        diagnostics: Vec::new(),
    };

    let method = lower_mir_function_to_jvm(&submission, &operation, &mir_fn);
    let code = method.code.expect("code");
    let field_writes: Vec<_> = code
        .instructions
        .iter()
        .filter_map(|instruction| match instruction {
            JvmInstruction::PutField(field) => Some(field.name.as_str()),
            _ => None,
        })
        .collect();
    assert!(field_writes.contains(&"head"), "AggregateCopy must preserve OuterTable.head: {:?}", code.instructions);
    assert!(field_writes.contains(&"map"), "AggregateCopy must preserve OuterTable.map: {:?}", code.instructions);
    assert!(field_writes.contains(&"names"), "AggregateCopy must preserve NestedMap.names leaf: {:?}", code.instructions);
    assert!(field_writes.contains(&"ids"), "AggregateCopy must preserve NestedMap.ids leaf: {:?}", code.instructions);
}

/// enums/unite 的 `FieldGet tag` 在 JVM int-handle ABI 上必须是恒等（iload/istore），
/// 禁止 `getfield tag`（会 VerifyError: Expecting object/array on stack）。
#[test]
fn sum_type_tag_field_get_is_int_identity_not_getfield() {
    use nyar_types::{SumTypeLayout, SumVariantLayout};

    let mut submission = FragmentSubmission::default();
    submission.sum_types = vec![SumTypeLayout {
        name: "WitWasiPreview".to_string(),
        is_unite: false,
        tag_width: 4,
        variants: vec![
            SumVariantLayout { name: "Preview1".to_string(), tag: 0, payload_type: None },
            SumVariantLayout { name: "Preview2".to_string(), tag: 1, payload_type: None },
        ],
    }];

    let preview = ValueRef(0);
    let tag = ValueRef(1);
    let operation = QualifiedName::new(vec![Identifier::new("wit_wasi_package_version")]);
    let mir_fn = ExecutableFunction {
        symbol: "wit_wasi_package_version".to_string(),
        return_type: NyarType::Integer32 { signed: true },
        param_types: vec![NyarType::Named(Identifier::new("WitWasiPreview"))],
        value_types: [(preview, NyarType::Named(Identifier::new("WitWasiPreview"))), (tag, NyarType::Integer32 { signed: true })]
            .into_iter()
            .collect(),
        entry: BlockRef(0),
        values: vec![Value { id: preview, origin: ValueOrigin::Parameter { index: 0, name: "preview".to_string() } }],
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![Instruction {
                output: Some(tag),
                kind: InstructionKind::FieldGet {
                    object: Operand::Value(preview),
                    field: "tag".to_string(),
                    storage: StorageKind::Value,
                    layout_id: None,
                },
            }],
            terminator: Terminator::Return { value: Some(Operand::Value(tag)) },
        }],
        diagnostics: Vec::new(),
    };

    let method = lower_mir_function_to_jvm(&submission, &operation, &mir_fn);
    let code = method.code.expect("code");
    assert!(
        !code.instructions.iter().any(|ins| matches!(ins, JvmInstruction::GetField(_))),
        "sum-type tag must not getfield: {:?}",
        code.instructions
    );
    assert!(
        code.instructions.iter().any(|ins| matches!(ins, JvmInstruction::ILoad(_)))
            && code.instructions.iter().any(|ins| matches!(ins, JvmInstruction::IStore(_))),
        "sum-type tag must copy int handle: {:?}",
        code.instructions
    );
}

/// 参数 sum 类型常只在 `param_types` + `seed_parameter_value_types`（overrides）中，
/// `value_types` 可能为空。此时 `FieldGet tag` 仍必须走 int 句柄恒等，
/// 禁止 `iload` + `getfield`（`emit_wasm_artifact_from_mir` / WasmHostBoundary）。
#[test]
fn sum_type_param_tag_field_get_without_value_types_is_identity() {
    use nyar_types::{SumTypeLayout, SumVariantLayout};

    let mut submission = FragmentSubmission::default();
    submission.sum_types = vec![SumTypeLayout {
        name: "WasmHostBoundary".to_string(),
        is_unite: false,
        tag_width: 4,
        variants: vec![
            SumVariantLayout { name: "WasmJsGlue".to_string(), tag: 0, payload_type: None },
            SumVariantLayout { name: "WasiCm".to_string(), tag: 1, payload_type: None },
        ],
    }];

    let boundary = ValueRef(0);
    let tag = ValueRef(1);
    let operation = QualifiedName::new(vec![Identifier::new("boundary_tag")]);
    let mir_fn = ExecutableFunction {
        symbol: "boundary_tag".to_string(),
        return_type: NyarType::Integer32 { signed: true },
        param_types: vec![NyarType::Named(Identifier::new("WasmHostBoundary"))],
        // Intentionally empty: production MIR often omits param entries here.
        value_types: Default::default(),
        entry: BlockRef(0),
        values: vec![Value { id: boundary, origin: ValueOrigin::Parameter { index: 0, name: "boundary".to_string() } }],
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![Instruction {
                output: Some(tag),
                kind: InstructionKind::FieldGet {
                    object: Operand::Value(boundary),
                    field: "tag".to_string(),
                    storage: StorageKind::Value,
                    layout_id: None,
                },
            }],
            terminator: Terminator::Return { value: Some(Operand::Value(tag)) },
        }],
        diagnostics: Vec::new(),
    };

    let method = lower_mir_function_to_jvm(&submission, &operation, &mir_fn);
    let code = method.code.expect("code");
    assert!(
        !code.instructions.iter().any(|ins| matches!(ins, JvmInstruction::GetField(_))),
        "param sum-type tag must not getfield when value_types empty: {:?}",
        code.instructions
    );
    assert!(
        code.instructions.iter().any(|ins| matches!(ins, JvmInstruction::ILoad(_))),
        "param sum-type tag must iload int handle: {:?}",
        code.instructions
    );
}

/// `i64` 参数描述符必须是单个 `J`，不能因 flatten 双槽变成 `(JJ)`。
#[test]
fn i64_param_descriptor_is_single_long_not_jj() {
    let submission = FragmentSubmission::default();
    let operation = QualifiedName::new(vec![Identifier::new("render_i64_text")]);
    let value = ValueRef(0);
    let mir_fn = ExecutableFunction {
        symbol: "render_i64_text".to_string(),
        return_type: NyarType::Utf8,
        param_types: vec![NyarType::Integer64 { signed: true }],
        value_types: [(value, NyarType::Integer64 { signed: true })].into_iter().collect(),
        entry: BlockRef(0),
        values: vec![Value { id: value, origin: ValueOrigin::Parameter { index: 0, name: "value".to_string() } }],
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: Terminator::Return { value: Some(Operand::Constant(Constant::Utf8("0".to_string()))) },
        }],
        diagnostics: Vec::new(),
    };
    let method = lower_mir_function_to_jvm(&submission, &operation, &mir_fn);
    assert_eq!(method.descriptor.parameter_types, vec![JvmTypeDescriptor::Long], "i64 param must be J, not JJ: {:?}", method.descriptor);
}

/// `i64 == 0` 必须 `lcmp`+`ifeq`，禁止 `lload`+`iconst`+`if_icmpeq`
///（VerifyError: Expecting to find integer on stack）。
#[test]
fn i64_compare_uses_lcmp_not_if_icmp() {
    let submission = FragmentSubmission::default();
    let operation = QualifiedName::new(vec![Identifier::new("cmp_i64")]);
    let value = ValueRef(0);
    let cmp = ValueRef(1);
    let mir_fn = ExecutableFunction {
        symbol: "cmp_i64".to_string(),
        return_type: NyarType::Boolean,
        param_types: vec![NyarType::Integer64 { signed: true }],
        value_types: [(value, NyarType::Integer64 { signed: true }), (cmp, NyarType::Boolean)].into_iter().collect(),
        entry: BlockRef(0),
        values: vec![
            Value { id: value, origin: ValueOrigin::Parameter { index: 0, name: "value".to_string() } },
            Value { id: cmp, origin: ValueOrigin::CallResult },
        ],
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![Instruction {
                output: Some(cmp),
                kind: InstructionKind::Call {
                    dispatch: DispatchKind::Static,
                    callee: Operand::Symbol(NamePath::new(vec![Identifier::new("==")])),
                    arguments: vec![Operand::Value(value), Operand::Constant(Constant::Int(0))],
                    witness: None,
                    effect: None,
                    receiver_kind: None,
                    parameter_types: Some(vec![NyarType::Integer64 { signed: true }, NyarType::Integer64 { signed: true }]),
                    intrinsic_opcode: Some(IntrinsicOpcode::Compare(IntrinsicCompareOp::Eq)),
                },
            }],
            terminator: Terminator::Return { value: Some(Operand::Value(cmp)) },
        }],
        diagnostics: Vec::new(),
    };
    let method = lower_mir_function_to_jvm(&submission, &operation, &mir_fn);
    let code = method.code.expect("code");
    assert!(code.instructions.iter().any(|ins| matches!(ins, JvmInstruction::LCmp)), "i64 compare must use lcmp: {:?}", code.instructions);
    assert!(
        !code.instructions.iter().any(|ins| matches!(
            ins,
            JvmInstruction::IfICmpEq(_)
                | JvmInstruction::IfICmpNe(_)
                | JvmInstruction::IfICmpLt(_)
                | JvmInstruction::IfICmpLe(_)
                | JvmInstruction::IfICmpGt(_)
                | JvmInstruction::IfICmpGe(_)
        )),
        "i64 compare must not use if_icmp*: {:?}",
        code.instructions
    );
}

/// `utf8._repr` 必须走 `String.getBytes("UTF-8")`，禁止 `getfield Utf16Text._repr`。
#[test]
fn utf8_repr_field_get_uses_string_get_bytes_not_utf16_getfield() {
    let submission = FragmentSubmission::default();
    let operation = QualifiedName::new(vec![Identifier::new("read_utf8_repr")]);
    let param = ValueRef(0);
    let repr = ValueRef(1);
    let mir_fn = ExecutableFunction {
        symbol: "read_utf8_repr".to_string(),
        return_type: NyarType::Array(Box::new(NyarType::Integer8 { signed: false })),
        param_types: vec![NyarType::Utf8],
        value_types: [(param, NyarType::Utf8), (repr, NyarType::Array(Box::new(NyarType::Integer8 { signed: false })))].into_iter().collect(),
        entry: BlockRef(0),
        values: vec![Value { id: param, origin: ValueOrigin::Parameter { index: 0, name: "ch".to_string() } }],
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![Instruction {
                output: Some(repr),
                kind: InstructionKind::FieldGet {
                    object: Operand::Value(param),
                    field: "_repr".to_string(),
                    storage: StorageKind::Reference,
                    layout_id: None,
                },
            }],
            terminator: Terminator::Return { value: Some(Operand::Value(repr)) },
        }],
        diagnostics: Vec::new(),
    };
    let method = lower_mir_function_to_jvm(&submission, &operation, &mir_fn);
    let code = method.code.expect("code");
    assert!(
        code.instructions.iter().any(|ins| matches!(
            ins,
            JvmInstruction::InvokeVirtual(m) if m.name == "getBytes" && m.owner == "java/lang/String"
        )),
        "expected String.getBytes for utf8._repr, got {:?}",
        code.instructions
    );
    assert!(
        !code.instructions.iter().any(|ins| matches!(ins, JvmInstruction::GetField(_))),
        "utf8._repr must not getfield: {:?}",
        code.instructions
    );
    assert!(
        code.instructions.iter().any(|ins| matches!(ins, JvmInstruction::LdcString(s) if s == "UTF-8")),
        "expected UTF-8 charset constant: {:?}",
        code.instructions
    );
}

/// `isize::prefix -` 等原始类型方法常残留 `self: Self` / 返回 `Self`。
/// 必须收成 `(I)I` + `iload`/`ineg`/`ireturn`，禁止 `Object` ABI 上的
/// `aload`+`ineg` 或 `iconst`+`areturn`（VerifyError integer / object/array）。
#[test]
fn primitive_owner_self_receiver_uses_int_abi_not_object() {
    use crate::contracts::IntrinsicOpcode;

    let submission = FragmentSubmission::default();
    let operation = QualifiedName::new(vec![Identifier::new("isize"), Identifier::new("prefix -")]);
    let parameter = ValueRef(0);
    let mir_fn = ExecutableFunction {
        symbol: "isize::prefix -".to_string(),
        return_type: NyarType::Named(Identifier::new("Self")),
        param_types: vec![NyarType::Named(Identifier::new("Self"))],
        value_types: [(parameter, NyarType::Named(Identifier::new("Self")))].into_iter().collect(),
        entry: BlockRef(0),
        values: vec![Value { id: parameter, origin: ValueOrigin::Parameter { index: 0, name: "self".to_string() } }],
        intrinsic: Some(IntrinsicOpcode::Neg),
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: vec![parameter],
            instructions: Vec::new(),
            terminator: Terminator::Unreachable,
        }],
        diagnostics: Vec::new(),
    };

    let method = lower_mir_function_to_jvm(&submission, &operation, &mir_fn);
    assert_eq!(
        method.descriptor.parameter_types,
        vec![JvmTypeDescriptor::Int],
        "Self on isize must be I, not Ljava/lang/Object;: {:?}",
        method.descriptor
    );
    assert_eq!(method.descriptor.return_type, JvmTypeDescriptor::Int, "Self return on isize must be I: {:?}", method.descriptor);
    let code = method.code.expect("code");
    assert!(
        code.instructions.iter().any(|ins| matches!(ins, JvmInstruction::ILoad(_))),
        "expected iload for Self→isize: {:?}",
        code.instructions
    );
    assert!(code.instructions.iter().any(|ins| matches!(ins, JvmInstruction::INeg)), "expected ineg: {:?}", code.instructions);
    assert!(code.instructions.iter().any(|ins| matches!(ins, JvmInstruction::IReturn)), "expected ireturn: {:?}", code.instructions);
    assert!(
        !code.instructions.iter().any(|ins| matches!(ins, JvmInstruction::ALoad(_) | JvmInstruction::AReturn | JvmInstruction::CheckCast(_))),
        "must not aload/areturn/checkcast Self→isize: {:?}",
        code.instructions
    );
}

/// `VonParsedValue { value: unite, next_index: usize }` 作 `Fine` 实参时必须压两个 int。
/// `usize` 是空 structure 别名且在 value_type_names 中；若 leaf 收集把它们丢掉，
/// 会 invent `Fine:(II)I` 却只压一个 int → VerifyError: Expecting integer on stack。
#[test]
fn fine_von_parsed_value_pushes_usize_leaf_not_dropped() {
    use nyar_types::{SumTypeLayout, SumVariantLayout};

    let mut submission = FragmentSubmission::default();
    submission.module_name = "demo".to_string();
    submission.fragment_id = Identifier::new("parse");
    submission.sum_types = vec![
        SumTypeLayout {
            name: "VonValue".to_string(),
            is_unite: true,
            tag_width: 4,
            variants: vec![SumVariantLayout { name: "Name".to_string(), tag: 0, payload_type: Some(NyarType::Utf8) }],
        },
        SumTypeLayout {
            name: "Result".to_string(),
            is_unite: true,
            tag_width: 4,
            variants: vec![
                SumVariantLayout { name: "Fine".to_string(), tag: 0, payload_type: None },
                SumVariantLayout { name: "Fail".to_string(), tag: 1, payload_type: None },
            ],
        },
    ];
    submission.aggregate_layouts = AggregateLayoutPlan {
        layouts: vec![
            AggregateLayout {
                id: 1,
                name: "usize".to_string(),
                namespace: String::new(),
                storage: StorageKind::Value,
                size: 0,
                align: 1,
                fields: Vec::new(),
            },
            AggregateLayout {
                id: 2,
                name: "VonParsedValue".to_string(),
                namespace: String::new(),
                storage: StorageKind::Value,
                size: 8,
                align: 4,
                fields: vec![
                    FieldLayout { name: "value".to_string(), ty: NyarType::Named(Identifier::new("VonValue")), offset: 0, size: 4, align: 4 },
                    FieldLayout { name: "next_index".to_string(), ty: NyarType::Named(Identifier::new("usize")), offset: 4, size: 4, align: 4 },
                ],
            },
        ],
        value_type_names: ["usize".to_string(), "VonParsedValue".to_string()].into_iter().collect(),
        type_name_to_layout: [("usize".to_string(), 1), ("VonParsedValue".to_string(), 2)].into_iter().collect(),
    };

    let parsed = ValueRef(0);
    let result = ValueRef(1);
    let operation = QualifiedName::new(vec![Identifier::new("call_fine")]);
    let mir_fn = ExecutableFunction {
        symbol: "call_fine".to_string(),
        return_type: NyarType::Named(Identifier::new("Result")),
        param_types: Vec::new(),
        value_types: [(parsed, NyarType::Named(Identifier::new("VonParsedValue"))), (result, NyarType::Named(Identifier::new("Result")))]
            .into_iter()
            .collect(),
        entry: BlockRef(0),
        values: vec![Value { id: parsed, origin: ValueOrigin::CallResult }, Value { id: result, origin: ValueOrigin::CallResult }],
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![
                Instruction {
                    output: Some(parsed),
                    kind: InstructionKind::StructNew {
                        type_name: "VonParsedValue".to_string(),
                        storage: StorageKind::Value,
                        layout_id: Some(2),
                        fields: vec![
                            ("value".to_string(), Operand::Constant(Constant::Int(0))),
                            ("next_index".to_string(), Operand::Constant(Constant::Int(1))),
                        ],
                    },
                },
                Instruction {
                    output: Some(result),
                    kind: InstructionKind::Call {
                        dispatch: DispatchKind::Static,
                        callee: Operand::Symbol(NamePath::new(vec![Identifier::new("Fine")])),
                        arguments: vec![Operand::Value(parsed)],
                        witness: None,
                        effect: None,
                        receiver_kind: None,
                        parameter_types: None,
                        intrinsic_opcode: None,
                    },
                },
            ],
            terminator: Terminator::Return { value: Some(Operand::Value(result)) },
        }],
        diagnostics: Vec::new(),
    };

    let method = lower_mir_function_to_jvm(&submission, &operation, &mir_fn);
    let code = method.code.expect("code");
    let fine_invoke = code.instructions.iter().find_map(|ins| match ins {
        JvmInstruction::InvokeStatic(method_ref) if method_ref.name == "Fine" => Some(method_ref),
        _ => None,
    });
    let fine = fine_invoke.expect(&format!("Fine invoke missing: {:?}", code.instructions));
    assert_eq!(
        fine.descriptor.parameter_types,
        vec![JvmTypeDescriptor::Int, JvmTypeDescriptor::Int],
        "Fine must flatten VonParsedValue to (II): {:?}",
        fine.descriptor
    );
    // Before Fine: two int pushes (iload/iconst), not a single int.
    let fine_at = code.instructions.iter().position(|ins| matches!(ins, JvmInstruction::InvokeStatic(m) if m.name == "Fine")).expect("Fine");
    let prelude = &code.instructions[..fine_at];
    let int_pushes = prelude.iter().filter(|ins| matches!(ins, JvmInstruction::ILoad(_) | JvmInstruction::IConst(_))).count();
    // StructNew writes two fields (2 pushes + 2 stores) then call expands two loads.
    assert!(int_pushes >= 4, "must push both value and usize leaves before Fine(II); prelude={prelude:?}");
}

#[test]
fn erased_unite_call_result_payload_uses_tuple_get_not_getfield() {
    use nyar_types::{SumTypeLayout, SumVariantLayout};

    let result_ty = NyarType::Apply(Box::new(NyarType::Named(Identifier::new("Result"))), vec![NyarType::Named(Identifier::new("VonValue"))]);
    let mut submission = FragmentSubmission::default();
    submission.module_name = "demo".to_string();
    submission.fragment_id = Identifier::new("parse");
    submission.sum_types = vec![
        SumTypeLayout {
            name: "Result".to_string(),
            is_unite: true,
            tag_width: 4,
            variants: vec![
                SumVariantLayout { name: "Fine".to_string(), tag: 0, payload_type: Some(NyarType::Named(Identifier::new("VonValue"))) },
                SumVariantLayout { name: "Fail".to_string(), tag: 1, payload_type: None },
            ],
        },
        SumTypeLayout {
            name: "VonValue".to_string(),
            is_unite: true,
            tag_width: 4,
            variants: vec![SumVariantLayout { name: "Name".to_string(), tag: 0, payload_type: Some(NyarType::Utf8) }],
        },
    ];

    let caller = QualifiedName::new(vec![Identifier::new("parse_von_tokens")]);
    let callee = QualifiedName::new(vec![Identifier::new("parse_von_value")]);
    let result = ValueRef(0);
    let payload = ValueRef(1);
    let caller_fn = ExecutableFunction {
        symbol: "parse_von_tokens".to_string(),
        return_type: NyarType::Named(Identifier::new("VonValue")),
        param_types: Vec::new(),
        // Deliberately omit `result`: this mirrors the erased CallResult in parse_von_tokens.
        value_types: [(payload, NyarType::Named(Identifier::new("VonValue")))].into_iter().collect(),
        entry: BlockRef(0),
        values: vec![Value { id: result, origin: ValueOrigin::CallResult }, Value { id: payload, origin: ValueOrigin::CallResult }],
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: vec![Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![
                Instruction {
                    output: Some(result),
                    kind: InstructionKind::Call {
                        dispatch: DispatchKind::Static,
                        callee: Operand::Symbol(NamePath::new(vec![Identifier::new("parse_von_value")])),
                        arguments: Vec::new(),
                        witness: None,
                        effect: None,
                        receiver_kind: None,
                        parameter_types: None,
                        intrinsic_opcode: None,
                    },
                },
                Instruction {
                    output: Some(payload),
                    kind: InstructionKind::FieldGet {
                        object: Operand::Value(result),
                        field: "payload".to_string(),
                        storage: StorageKind::Value,
                        layout_id: None,
                    },
                },
            ],
            terminator: Terminator::Return { value: Some(Operand::Value(payload)) },
        }],
        diagnostics: Vec::new(),
    };
    let callee_fn = ExecutableFunction {
        symbol: "parse_von_value".to_string(),
        return_type: result_ty,
        param_types: Vec::new(),
        value_types: Default::default(),
        entry: BlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: Vec::new(),
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks: Vec::new(),
        diagnostics: Vec::new(),
    };
    submission.executable =
        Some(Arc::new(MirFunctionMapProvider::new([(caller.clone(), caller_fn.clone()), (callee, callee_fn)].into_iter().collect())));

    let method = lower_mir_function_to_jvm(&submission, &caller, &caller_fn);
    let code = method.code.expect("code");
    assert!(
        code.instructions.iter().any(|ins| matches!(
            ins,
            JvmInstruction::InvokeStatic(method_ref)
                if method_ref.name == "tuple_get_0"
                    && method_ref.descriptor == JvmMethodDescriptor::new(
                        vec![JvmTypeDescriptor::Int],
                        JvmTypeDescriptor::Int,
                    )
        )),
        "erased unite payload must use int tuple_get_0: {:?}",
        code.instructions
    );
    assert!(
        !code.instructions.iter().any(|ins| matches!(
            ins,
            JvmInstruction::GetField(field) if field.name == "payload"
        )),
        "int unite handle must never use getfield payload: {:?}",
        code.instructions
    );
}
