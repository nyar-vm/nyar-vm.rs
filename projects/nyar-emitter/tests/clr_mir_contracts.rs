use std::collections::{BTreeMap, BTreeSet};

use nyar::{ExternalImportLink, Identifier, QualifiedName};
use nyar_emitter::{
    FragmentSubmission,
    executable_provider::{ExecutableFunction, MirFunctionMapProvider},
    nyar_backend_clr::{MsilInstructionOperand, MsilOpcode, MsilType},
    testing::{lower_fragment_to_clr_msil, lower_mir_to_clr_method},
};
use nyar_language::{
    AggregateLayout, AggregateLayoutPlan, FieldLayout, MirBlock, MirBlockRef, MirDispatchKind, MirFunction, MirInstruction, MirInstructionKind,
    MirLowerer, MirOperand, MirStorageKind, MirTerminator, MirValue, MirValueOrigin, MirValueRef, SingletonInstancePlan, ValkyrieCompiler,
    types::{
        SourceID,
        hir::{FunctionType, ValkyrieType},
    },
};
use std::sync::Arc;

#[test]
fn rejects_unknown_call_signature_instead_of_guessing_object_object() {
    let operation = QualifiedName::new(vec![Identifier::new("main")]);
    let mir_fn = MirFunction {
        symbol: "main".to_string(),
        return_type: ValkyrieType::Unit,
        param_types: Vec::new(),
        value_types: Default::default(),
        entry: MirBlockRef(0),
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
        blocks: vec![MirBlock {
            id: MirBlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![MirInstruction {
                output: None,
                kind: MirInstructionKind::Call {
                    dispatch: MirDispatchKind::Static,
                    callee: MirOperand::Symbol(nyar_language::types::NamePath::new(vec![Identifier::new("missing")])),
                    arguments: Vec::new(),
                    witness: None,
                    effect: None,
                    receiver_kind: None,
                    parameter_types: None,
                    intrinsic_opcode: None,
                },
            }],
            terminator: MirTerminator::Return { value: None },
        }],
        diagnostics: Vec::new(),
    };
    let contract: ExecutableFunction = mir_fn.into();
    let error =
        lower_mir_to_clr_method(&FragmentSubmission::default(), &operation, &contract).expect_err("unknown call signatures must fail closed");

    let message = error.to_string();
    assert!(
        message.contains("unknown_call_signature")
            || message.contains("拒绝")
            || message.contains("Object(Object)")
            || format!("{error:?}").contains("unknown_call_signature"),
        "{error:?}"
    );
}

#[test]
fn lowers_vm_i64_to_i32_as_conv_i4_not_a_local_call() {
    let operation = QualifiedName::new(vec![Identifier::new("main")]);
    let mir_fn = MirFunction {
        symbol: "main".to_string(),
        return_type: ValkyrieType::Integer32 { signed: true },
        param_types: vec![ValkyrieType::Integer64 { signed: true }],
        value_types: [(MirValueRef(0), ValkyrieType::Integer64 { signed: true }), (MirValueRef(1), ValkyrieType::Integer32 { signed: true })]
            .into_iter()
            .collect(),
        entry: MirBlockRef(0),
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
        blocks: vec![MirBlock {
            id: MirBlockRef(0),
            label: "entry".to_string(),
            parameters: vec![MirValueRef(0)],
            instructions: vec![MirInstruction {
                output: Some(MirValueRef(1)),
                kind: MirInstructionKind::Call {
                    dispatch: MirDispatchKind::Static,
                    callee: MirOperand::Symbol(nyar_language::types::NamePath::new(vec![Identifier::new("i64_to_i32")])),
                    arguments: vec![MirOperand::Value(MirValueRef(0))],
                    witness: None,
                    effect: None,
                    receiver_kind: None,
                    parameter_types: None,
                    intrinsic_opcode: None,
                },
            }],
            terminator: MirTerminator::Return { value: Some(MirOperand::Value(MirValueRef(1))) },
        }],
        diagnostics: Vec::new(),
    };
    let contract: ExecutableFunction = mir_fn.into();
    let body = lower_mir_to_clr_method(&FragmentSubmission::default(), &operation, &contract).expect("CLR conversion lowering");

    assert!(body.instructions.iter().any(|instruction| instruction.opcode == MsilOpcode::ConvI4), "expected conv.i4, got {body:?}");
    assert!(
        !body.instructions.iter().any(|instruction| matches!(
            &instruction.operand,
            Some(MsilInstructionOperand::Method(method_ref)) if method_ref.owner.is_none() && method_ref.name == "i64_to_i32"
        )),
        "vm conversion must not leave an unresolved local call: {body:?}"
    );
}

#[test]
fn lowers_struct_new_and_aggregate_copy_to_msil() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9600 })
        .compile_source(
            r#"
structure Point {
    x: f64,
    y: f64,
}

micro main() {
    let p1 = Point { x: 1.0, y: 2.0 };
    let p2 = p1;
    p2
}
"#,
        )
        .expect("compile");
    let mir = MirLowerer::lower_module_semantic(&hir);
    let plan = mir.aggregate_layouts.clone();
    let main_symbol = mir.functions.iter().find(|function| function.symbol.ends_with("main")).expect("main mir");
    let operation = QualifiedName::new(vec![Identifier::new("main")]);
    let mut submission = FragmentSubmission::default();
    submission.aggregate_layouts = plan;
    submission.executable =
        Some(Arc::new(MirFunctionMapProvider::new([(operation.clone(), main_symbol.clone().into())].into_iter().collect())));
    let main_contract: ExecutableFunction = main_symbol.clone().into();
    let body = lower_mir_to_clr_method(&submission, &operation, &main_contract).expect("CLR MIR lowering");
    assert!(body.instructions.iter().any(|ins| ins.opcode == MsilOpcode::Initobj));
    // Value-type AggregateCopy lowers to ldloc/stloc (not cpblk): ECMA-335 forbids cpblk
    // when the valuetype may contain GC refs.
    assert!(
        body.instructions.iter().any(|ins| matches!(
            ins.opcode,
            MsilOpcode::Ldloc | MsilOpcode::Ldloc0 | MsilOpcode::Ldloc1 | MsilOpcode::Ldloc2 | MsilOpcode::Ldloc3
        )) && body.instructions.iter().any(|ins| matches!(
            ins.opcode,
            MsilOpcode::Stloc | MsilOpcode::Stloc0 | MsilOpcode::Stloc1 | MsilOpcode::Stloc2 | MsilOpcode::Stloc3
        )),
        "expected valuetype AggregateCopy via ldloc/stloc, got {body:?}"
    );
}

#[test]
fn emits_msil_type_defs_for_value_layout_plan() {
    let plan = AggregateLayoutPlan {
        layouts: vec![AggregateLayout {
            id: 1,
            name: "Point".to_string(),
            namespace: String::new(),
            storage: MirStorageKind::Value,
            size: 16,
            align: 8,
            fields: vec![
                FieldLayout { name: "x".to_string(), ty: nyar::NyarType::Float64, offset: 0, size: 8, align: 8 },
                FieldLayout { name: "y".to_string(), ty: nyar::NyarType::Float64, offset: 8, size: 8, align: 8 },
            ],
        }],
        value_type_names: BTreeSet::from(["Point".to_string()]),
        type_name_to_layout: BTreeMap::from([("Point".to_string(), 1)]),
    };
    let mir_fn = MirFunction {
        symbol: "main".to_string(),
        return_type: ValkyrieType::Unit,
        param_types: Vec::new(),
        value_types: Default::default(),
        entry: MirBlockRef(0),
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
        blocks: vec![MirBlock {
            id: MirBlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![MirInstruction {
                output: None,
                kind: MirInstructionKind::LoadConstant { constant: nyar_language::MirConstant::Unit, ty: None },
            }],
            terminator: MirTerminator::Return { value: None },
        }],
        diagnostics: Vec::new(),
    };
    let operation = QualifiedName::new(vec![Identifier::new("main")]);
    let mut submission = FragmentSubmission::default();
    submission.aggregate_layouts = plan;
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new([(operation.clone(), mir_fn.clone().into())].into_iter().collect())));
    let module = lower_fragment_to_clr_msil(&submission).expect("CLR lowering");
    assert_eq!(module.types.len(), 1);
    assert!(module.types[0].is_value_type);
}

/// Verifies that a singleton accessor call (`Counter.instance`) lowers to a static
/// `call` instruction whose method ref owner is `Counter` and name is `instance`.
#[test]
fn clr_singleton_accessor_call_emits_call() {
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
    let mir_fn = MirFunction {
        symbol: "main".to_string(),
        return_type: ValkyrieType::Named(Identifier::new("Counter")),
        param_types: Vec::new(),
        value_types: [(MirValueRef(0), ValkyrieType::Named(Identifier::new("Counter")))].into_iter().collect(),
        entry: MirBlockRef(0),
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
        blocks: vec![MirBlock {
            id: MirBlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![MirInstruction {
                output: Some(MirValueRef(0)),
                kind: MirInstructionKind::Call {
                    dispatch: MirDispatchKind::Static,
                    callee: MirOperand::Symbol(nyar_language::types::NamePath::new(vec![
                        Identifier::new("Counter"),
                        Identifier::new("instance"),
                    ])),
                    arguments: Vec::new(),
                    witness: None,
                    effect: None,
                    receiver_kind: None,
                    parameter_types: None,
                    intrinsic_opcode: None,
                },
            }],
            terminator: MirTerminator::Return { value: Some(MirOperand::Value(MirValueRef(0))) },
        }],
        diagnostics: Vec::new(),
    };
    let contract: ExecutableFunction = mir_fn.into();
    let body = lower_mir_to_clr_method(&submission, &operation, &contract).expect("CLR MIR lowering");
    let has_accessor = body.instructions.iter().any(|ins| {
        ins.opcode == MsilOpcode::Call
            && matches!(
                &ins.operand,
                Some(MsilInstructionOperand::Method(method_ref))
                    if method_ref.owner.as_deref() == Some("Counter") && method_ref.name == "instance"
            )
    });
    assert!(has_accessor, "expected Call Counter.instance, got {:?}", body.instructions);
}

/// Verifies that a singleton instance method call (`Counter.increment`) lowers to a
/// `callvirt` instruction whose method ref owner is `Counter` and name is `increment`.
#[test]
fn clr_singleton_instance_method_call_emits_callvirt() {
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
    let increment_fn = MirFunction {
        symbol: "Counter.increment".to_string(),
        return_type: ValkyrieType::Integer64 { signed: true },
        param_types: vec![ValkyrieType::Named(Identifier::new("Counter"))],
        value_types: Default::default(),
        entry: MirBlockRef(0),
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
    let mut mir_map: std::collections::BTreeMap<QualifiedName, ExecutableFunction> = Default::default();
    mir_map.insert(increment_key, increment_fn.into());

    let operation = QualifiedName::new(vec![Identifier::new("main")]);
    let value_types =
        [(MirValueRef(0), ValkyrieType::Named(Identifier::new("Counter"))), (MirValueRef(1), ValkyrieType::Integer64 { signed: true })]
            .into_iter()
            .collect();
    let mir_fn = MirFunction {
        symbol: "main".to_string(),
        return_type: ValkyrieType::Integer64 { signed: true },
        param_types: Vec::new(),
        value_types,
        entry: MirBlockRef(0),
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
        blocks: vec![MirBlock {
            id: MirBlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![
                MirInstruction {
                    output: Some(MirValueRef(0)),
                    kind: MirInstructionKind::Call {
                        dispatch: MirDispatchKind::Static,
                        callee: MirOperand::Symbol(nyar_language::types::NamePath::new(vec![
                            Identifier::new("Counter"),
                            Identifier::new("instance"),
                        ])),
                        arguments: Vec::new(),
                        witness: None,
                        effect: None,
                        receiver_kind: None,
                        parameter_types: None,
                        intrinsic_opcode: None,
                    },
                },
                MirInstruction {
                    output: Some(MirValueRef(1)),
                    kind: MirInstructionKind::Call {
                        dispatch: MirDispatchKind::Static,
                        callee: MirOperand::Symbol(nyar_language::types::NamePath::new(vec![
                            Identifier::new("Counter"),
                            Identifier::new("increment"),
                        ])),
                        arguments: vec![MirOperand::Value(MirValueRef(0))],
                        witness: None,
                        effect: None,
                        receiver_kind: None,
                        parameter_types: None,
                        intrinsic_opcode: None,
                    },
                },
            ],
            terminator: MirTerminator::Return { value: Some(MirOperand::Value(MirValueRef(1))) },
        }],
        diagnostics: Vec::new(),
    };
    mir_map.insert(operation.clone(), mir_fn.clone().into());
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(mir_map)));
    let contract: ExecutableFunction = mir_fn.into();
    let body = lower_mir_to_clr_method(&submission, &operation, &contract).expect("CLR MIR lowering");
    let has_method = body.instructions.iter().any(|ins| {
        ins.opcode == MsilOpcode::Callvirt
            && matches!(
                &ins.operand,
                Some(MsilInstructionOperand::Method(method_ref))
                    if method_ref.owner.as_deref() == Some("Counter") && method_ref.name == "increment"
            )
    });
    assert!(has_method, "expected Callvirt Counter.increment, got {:?}", body.instructions);
}

#[test]
fn clr_basic_ffi_preserves_byte_array_parameters_and_returns() {
    let main = QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("main")]);
    let read = QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("read_all_bytes")]);
    let write = QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("write_all_bytes")]);
    let byte_array = ValkyrieType::Array(Box::new(ValkyrieType::Named(Identifier::new("u8"))));
    let mir_fn = MirFunction {
        symbol: main.to_string(),
        return_type: byte_array.clone(),
        param_types: vec![ValkyrieType::Utf16],
        value_types: [(MirValueRef(0), ValkyrieType::Utf16), (MirValueRef(1), byte_array.clone())].into_iter().collect(),
        entry: MirBlockRef(0),
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
        blocks: vec![MirBlock {
            id: MirBlockRef(0),
            label: "entry".to_string(),
            parameters: vec![MirValueRef(0)],
            instructions: vec![
                MirInstruction {
                    output: Some(MirValueRef(1)),
                    kind: MirInstructionKind::Call {
                        dispatch: MirDispatchKind::Static,
                        callee: MirOperand::Symbol(nyar_language::types::NamePath::new(read.parts().to_vec())),
                        arguments: vec![MirOperand::Value(MirValueRef(0))],
                        witness: None,
                        effect: None,
                        receiver_kind: None,
                        parameter_types: Some(vec![ValkyrieType::Utf16]),
                        intrinsic_opcode: None,
                    },
                },
                MirInstruction {
                    output: None,
                    kind: MirInstructionKind::Call {
                        dispatch: MirDispatchKind::Static,
                        callee: MirOperand::Symbol(nyar_language::types::NamePath::new(write.parts().to_vec())),
                        arguments: vec![MirOperand::Value(MirValueRef(0)), MirOperand::Value(MirValueRef(1))],
                        witness: None,
                        effect: None,
                        receiver_kind: None,
                        parameter_types: Some(vec![ValkyrieType::Utf16, byte_array.clone()]),
                        intrinsic_opcode: None,
                    },
                },
            ],
            terminator: MirTerminator::Return { value: Some(MirOperand::Value(MirValueRef(1))) },
        }],
        diagnostics: Vec::new(),
    };
    let mut submission = FragmentSubmission::default();
    submission.external_import_links = BTreeMap::from([
        (
            read,
            ExternalImportLink::host(
                Some(Identifier::new("clr")),
                vec!["System.IO.FileSystem".to_string(), "System.IO.File".to_string(), "ReadAllBytes".to_string()],
            ),
        ),
        (
            write,
            ExternalImportLink::host(
                Some(Identifier::new("clr")),
                vec!["System.IO.FileSystem".to_string(), "System.IO.File".to_string(), "WriteAllBytes".to_string()],
            ),
        ),
    ]);
    let contract: ExecutableFunction = mir_fn.into();
    let body = lower_mir_to_clr_method(&submission, &main, &contract).expect("CLR MIR lowering");
    let byte_array_msil = MsilType::sz_array(MsilType::Int8 { signed: false });

    assert!(body.instructions.iter().any(|instruction| matches!(
        &instruction.operand,
        Some(MsilInstructionOperand::Method(target))
            if target.name == "ReadAllBytes"
                && target.signature.parameter_types == vec![MsilType::String]
                && target.signature.return_type == byte_array_msil
    )));
    assert!(body.instructions.iter().any(|instruction| matches!(
        &instruction.operand,
        Some(MsilInstructionOperand::Method(target))
            if target.name == "WriteAllBytes"
                && target.signature.parameter_types == vec![MsilType::String, byte_array_msil.clone()]
                && target.signature.return_type == MsilType::Void
    )));
}

#[test]
fn array_len_intrinsic_binding_emits_ldlen() {
    // Closed loop: Call to a symbol registered from `[intrinsic("array.len")]` → ArrayLen → ldlen.
    let operation = QualifiedName::new(vec![Identifier::new("build_backend_execution_request")]);
    let array_ty = ValkyrieType::Array(Box::new(ValkyrieType::Utf8));
    let len_symbol = nyar_language::types::NamePath::new(vec![Identifier::new("__array_len")]);
    let mir_fn = MirFunction {
        symbol: "build_backend_execution_request".to_string(),
        return_type: ValkyrieType::Integer64 { signed: true },
        param_types: vec![array_ty.clone()],
        value_types: [(MirValueRef(0), array_ty), (MirValueRef(1), ValkyrieType::Integer64 { signed: true })].into_iter().collect(),
        entry: MirBlockRef(0),
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
        blocks: vec![MirBlock {
            id: MirBlockRef(0),
            label: "entry".to_string(),
            parameters: vec![MirValueRef(0)],
            instructions: vec![MirInstruction {
                output: Some(MirValueRef(1)),
                kind: MirInstructionKind::Call {
                    dispatch: MirDispatchKind::Static,
                    callee: MirOperand::Symbol(len_symbol.clone()),
                    arguments: vec![MirOperand::Value(MirValueRef(0))],
                    witness: None,
                    effect: None,
                    receiver_kind: None,
                    parameter_types: None,
                    intrinsic_opcode: None,
                },
            }],
            terminator: MirTerminator::Return { value: Some(MirOperand::Value(MirValueRef(1))) },
        }],
        diagnostics: Vec::new(),
    };
    let mut submission = FragmentSubmission::default();
    submission.intrinsics.insert("__array_len".to_string(), nyar_emitter::contracts::IntrinsicOpcode::ArrayLen);
    submission.intrinsics.insert(len_symbol.to_string(), nyar_emitter::contracts::IntrinsicOpcode::ArrayLen);
    let contract: ExecutableFunction = mir_fn.into();
    let body = lower_mir_to_clr_method(&submission, &operation, &contract).expect("intrinsic ArrayLen→ldlen");
    assert!(body.instructions.iter().any(|ins| ins.opcode == MsilOpcode::Ldlen), "expected ldlen in {body:?}");
}

#[test]
fn unbound_length_call_fails_closed_without_guessing() {
    let operation = QualifiedName::new(vec![Identifier::new("build_backend_execution_request")]);
    let mir_fn = MirFunction {
        symbol: "build_backend_execution_request".to_string(),
        return_type: ValkyrieType::Unit,
        param_types: Vec::new(),
        value_types: Default::default(),
        entry: MirBlockRef(0),
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
        blocks: vec![MirBlock {
            id: MirBlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: vec![MirInstruction {
                output: None,
                kind: MirInstructionKind::Call {
                    dispatch: MirDispatchKind::Static,
                    callee: MirOperand::Symbol(nyar_language::types::NamePath::new(vec![
                        Identifier::new("plan"),
                        Identifier::new("source_closure"),
                        Identifier::new("package_names"),
                        Identifier::new("length"),
                    ])),
                    arguments: Vec::new(),
                    witness: None,
                    effect: None,
                    receiver_kind: None,
                    parameter_types: None,
                    intrinsic_opcode: None,
                },
            }],
            terminator: MirTerminator::Return { value: None },
        }],
        diagnostics: Vec::new(),
    };
    let contract: ExecutableFunction = mir_fn.into();
    let error = lower_mir_to_clr_method(&FragmentSubmission::default(), &operation, &contract)
        .expect_err("unbound length must fail closed (no name guess → ldlen)");
    let message = error.to_string();
    assert!(message.contains("拒绝猜测") || message.contains("unknown_call_signature") || message.contains("Object(Object)"), "{message}");
}

#[test]
fn ref_deref_intrinsic_binding_emits_identity() {
    // Closed loop: Call to `__ref_deref` registered as IntrinsicOpcode::Deref → ldloc identity (no MethodDef).
    let operation = QualifiedName::new(vec![Identifier::new("directed_graph_add_edge")]);
    let graph_ty = ValkyrieType::Named(Identifier::new("DirectedGraph"));
    let deref_symbol = nyar_language::types::NamePath::new(vec![Identifier::new("__ref_deref")]);
    let mir_fn = MirFunction {
        symbol: "directed_graph_add_edge".to_string(),
        return_type: graph_ty.clone(),
        param_types: vec![graph_ty.clone()],
        value_types: [(MirValueRef(0), graph_ty.clone()), (MirValueRef(1), graph_ty)].into_iter().collect(),
        entry: MirBlockRef(0),
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
        blocks: vec![MirBlock {
            id: MirBlockRef(0),
            label: "entry".to_string(),
            parameters: vec![MirValueRef(0)],
            instructions: vec![MirInstruction {
                output: Some(MirValueRef(1)),
                kind: MirInstructionKind::Call {
                    dispatch: MirDispatchKind::Static,
                    callee: MirOperand::Symbol(deref_symbol.clone()),
                    arguments: vec![MirOperand::Value(MirValueRef(0))],
                    witness: None,
                    effect: None,
                    receiver_kind: None,
                    parameter_types: None,
                    intrinsic_opcode: None,
                },
            }],
            terminator: MirTerminator::Return { value: Some(MirOperand::Value(MirValueRef(1))) },
        }],
        diagnostics: Vec::new(),
    };
    let mut submission = FragmentSubmission::default();
    submission.intrinsics.insert("__ref_deref".to_string(), nyar_emitter::contracts::IntrinsicOpcode::Deref);
    submission.intrinsics.insert(deref_symbol.to_string(), nyar_emitter::contracts::IntrinsicOpcode::Deref);
    let contract: ExecutableFunction = mir_fn.into();
    let body = lower_mir_to_clr_method(&submission, &operation, &contract).expect("intrinsic Deref→identity");
    assert!(
        !body.instructions.iter().any(|ins| {
            matches!(
                &ins.operand,
                Some(MsilInstructionOperand::Method(target)) if target.name == "deref" || target.name == "__ref_deref"
            )
        }),
        "must expand via opcode, not MethodDef: {body:?}"
    );
    assert!(
        body.instructions.iter().any(|ins| matches!(
            ins.opcode,
            MsilOpcode::Ldloc | MsilOpcode::Ldloc0 | MsilOpcode::Ldloc1 | MsilOpcode::Ldloc2 | MsilOpcode::Ldloc3
        )),
        "expected identity load in {body:?}"
    );
}

#[test]
fn function_typed_parameter_call_emits_dynamic_invoke() {
    // HOF: Call whose callee is a Function-typed SSA value (parameter `f`), not a MethodDef.
    let operation = QualifiedName::new(vec![Identifier::new("for_each")]);
    let fn_ty = ValkyrieType::Function(Box::new(FunctionType {
        params: vec![ValkyrieType::Integer64 { signed: true }],
        return_type: ValkyrieType::Unit,
    }));
    let item_ty = ValkyrieType::Integer64 { signed: true };
    let mir_fn = MirFunction {
        symbol: "for_each".to_string(),
        return_type: ValkyrieType::Unit,
        param_types: vec![fn_ty.clone(), item_ty.clone()],
        value_types: [(MirValueRef(0), fn_ty), (MirValueRef(1), item_ty)].into_iter().collect(),
        entry: MirBlockRef(0),
        values: vec![
            MirValue { id: MirValueRef(0), origin: MirValueOrigin::Parameter { index: 0, name: "f".to_string() } },
            MirValue { id: MirValueRef(1), origin: MirValueOrigin::Parameter { index: 1, name: "item".to_string() } },
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
        blocks: vec![MirBlock {
            id: MirBlockRef(0),
            label: "entry".to_string(),
            parameters: vec![MirValueRef(0), MirValueRef(1)],
            instructions: vec![MirInstruction {
                output: None,
                kind: MirInstructionKind::Call {
                    dispatch: MirDispatchKind::Indirect,
                    callee: MirOperand::Value(MirValueRef(0)),
                    arguments: vec![MirOperand::Value(MirValueRef(1))],
                    witness: None,
                    effect: None,
                    receiver_kind: None,
                    parameter_types: None,
                    intrinsic_opcode: None,
                },
            }],
            terminator: MirTerminator::Return { value: None },
        }],
        diagnostics: Vec::new(),
    };
    let contract: ExecutableFunction = mir_fn.into();
    let body = lower_mir_to_clr_method(&FragmentSubmission::default(), &operation, &contract).expect("function-value DynamicInvoke");
    assert!(
        body.instructions.iter().any(|ins| match (&ins.opcode, &ins.operand) {
            (MsilOpcode::Callvirt, Some(MsilInstructionOperand::Method(target))) => {
                target.name == "DynamicInvoke" && target.owner.as_deref() == Some("[mscorlib]System.Delegate")
            }
            _ => false,
        }),
        "expected Delegate::DynamicInvoke in {body:?}"
    );
    assert!(body.instructions.iter().any(|ins| ins.opcode == MsilOpcode::Castclass), "expected castclass System.Delegate in {body:?}");
    assert!(
        !body.instructions.iter().any(|ins| {
            matches!(
                &ins.operand,
                Some(MsilInstructionOperand::Method(target)) if target.name == "f" || target.owner.is_none() && target.name.contains("Object")
            )
        }),
        "must not invent a static MethodDef for function value `f`: {body:?}"
    );
}

#[test]
fn prefers_attached_call_parameter_types_over_name_lookup() {
    let operation = QualifiedName::new(vec![Identifier::new("main")]);
    let arg = MirValueRef(0);
    let out = MirValueRef(1);
    let mir_fn = MirFunction {
        symbol: "main".to_string(),
        return_type: ValkyrieType::Integer32 { signed: true },
        param_types: vec![ValkyrieType::Integer32 { signed: true }],
        value_types: BTreeMap::from([(arg, ValkyrieType::Integer32 { signed: true }), (out, ValkyrieType::Integer32 { signed: true })]),
        entry: MirBlockRef(0),
        values: vec![
            MirValue { id: arg, origin: MirValueOrigin::Parameter { index: 0, name: "x".into() } },
            MirValue { id: out, origin: MirValueOrigin::CallResult },
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
        blocks: vec![MirBlock {
            id: MirBlockRef(0),
            label: "entry".into(),
            parameters: vec![arg],
            instructions: vec![MirInstruction {
                output: Some(out),
                kind: MirInstructionKind::Call {
                    dispatch: MirDispatchKind::Static,
                    callee: MirOperand::Symbol(nyar_language::types::NamePath::new(vec![Identifier::new("helper")])),
                    arguments: vec![MirOperand::Value(arg)],
                    witness: None,
                    effect: None,
                    receiver_kind: None,
                    // Attached HIR signature: i32(i32). Without this, name lookup fails closed.
                    parameter_types: Some(vec![ValkyrieType::Integer32 { signed: true }]),
                    intrinsic_opcode: None,
                },
            }],
            terminator: MirTerminator::Return { value: Some(MirOperand::Value(out)) },
        }],
        diagnostics: Vec::new(),
    };
    let contract: ExecutableFunction = mir_fn.into();
    let body = lower_mir_to_clr_method(&FragmentSubmission::default(), &operation, &contract)
        .expect("attached parameter_types must unlock Call signature");
    assert!(
        body.instructions.iter().any(|ins| match (&ins.opcode, &ins.operand) {
            (MsilOpcode::Call, Some(MsilInstructionOperand::Method(target))) => {
                target.name.contains("helper")
                    && target.signature.parameter_types == vec![MsilType::Int32 { signed: true }]
                    && target.signature.return_type == MsilType::Int32 { signed: true }
            }
            _ => false,
        }),
        "expected helper(int32)->int32 from attached parameter_types: {body:?}"
    );
}

#[test]
fn lowers_nullable_none_and_some_without_methoddef() {
    let operation = QualifiedName::new(vec![Identifier::new("von_parse_take_fail")]);
    let payload = MirValueRef(0);
    let none_out = MirValueRef(1);
    let some_out = MirValueRef(2);
    let nullable =
        ValkyrieType::Union(vec![ValkyrieType::Named(Identifier::new("VonDiagnostic")), ValkyrieType::Named(Identifier::new("null"))]);
    let mir_fn = MirFunction {
        symbol: "std::data::text::von::von_parse_take_fail".to_string(),
        return_type: nullable.clone(),
        param_types: vec![ValkyrieType::Named(Identifier::new("VonParseResult"))],
        value_types: BTreeMap::from([
            (payload, ValkyrieType::Named(Identifier::new("VonDiagnostic"))),
            (none_out, nullable.clone()),
            (some_out, nullable.clone()),
        ]),
        entry: MirBlockRef(0),
        values: vec![
            MirValue { id: payload, origin: MirValueOrigin::Parameter { index: 0, name: "error".into() } },
            MirValue { id: none_out, origin: MirValueOrigin::CallResult },
            MirValue { id: some_out, origin: MirValueOrigin::CallResult },
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
        blocks: vec![MirBlock {
            id: MirBlockRef(0),
            label: "entry".into(),
            parameters: vec![payload],
            instructions: vec![
                MirInstruction {
                    output: Some(none_out),
                    kind: MirInstructionKind::Call {
                        dispatch: MirDispatchKind::Static,
                        callee: MirOperand::Symbol(nyar_language::types::NamePath::new(vec![Identifier::new("None")])),
                        arguments: Vec::new(),
                        witness: None,
                        effect: None,
                        receiver_kind: None,
                        parameter_types: None,
                        intrinsic_opcode: None,
                    },
                },
                MirInstruction {
                    output: Some(some_out),
                    kind: MirInstructionKind::Call {
                        dispatch: MirDispatchKind::Static,
                        callee: MirOperand::Symbol(nyar_language::types::NamePath::new(vec![Identifier::new("Some")])),
                        arguments: vec![MirOperand::Value(payload)],
                        witness: None,
                        effect: None,
                        receiver_kind: None,
                        parameter_types: None,
                        intrinsic_opcode: None,
                    },
                },
            ],
            terminator: MirTerminator::Return { value: Some(MirOperand::Value(some_out)) },
        }],
        diagnostics: Vec::new(),
    };
    let contract: ExecutableFunction = mir_fn.into();
    let body = lower_mir_to_clr_method(&FragmentSubmission::default(), &operation, &contract)
        .expect("nullable None()/Some(x) must lower without unknown_call_signature");
    assert!(body.instructions.iter().any(|ins| ins.opcode == MsilOpcode::Ldnull), "None() should emit ldnull: {body:?}");
    assert!(
        !body.instructions.iter().any(|ins| match &ins.operand {
            Some(MsilInstructionOperand::Method(target)) => target.name == "None" || target.name == "Some",
            _ => false,
        }),
        "must not invent None/Some MethodDefs: {body:?}"
    );
}

#[test]
fn unite_variant_struct_new_rewrites_to_sum_newobj() {
    use nyar_types::{SumTypeLayout, SumVariantLayout};

    let operation = QualifiedName::new(vec![Identifier::new("clr_local_slot_bytes")]);
    let value_arg = MirValueRef(0);
    let out = MirValueRef(1);
    let sum = SumTypeLayout {
        name: "MsilInstructionOperand".to_string(),
        is_unite: true,
        tag_width: 4,
        variants: vec![
            SumVariantLayout { name: "None".to_string(), tag: 0, payload_type: None },
            SumVariantLayout { name: "Integer".to_string(), tag: 1, payload_type: Some(nyar::NyarType::Integer64 { signed: true }) },
        ],
    };
    let mir_fn = MirFunction {
        symbol: "nyar::nyar_emitter::clr::clr_local_slot_bytes".to_string(),
        return_type: ValkyrieType::Named(Identifier::new("MsilInstructionOperand")),
        param_types: vec![ValkyrieType::Integer32 { signed: false }],
        value_types: BTreeMap::from([
            (value_arg, ValkyrieType::Integer64 { signed: true }),
            (out, ValkyrieType::Named(Identifier::new("MsilInstructionOperand"))),
        ]),
        entry: MirBlockRef(0),
        values: vec![
            MirValue { id: value_arg, origin: MirValueOrigin::Parameter { index: 0, name: "value".into() } },
            MirValue { id: out, origin: MirValueOrigin::Temporary },
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
        blocks: vec![MirBlock {
            id: MirBlockRef(0),
            label: "entry".into(),
            parameters: vec![value_arg],
            instructions: vec![MirInstruction {
                output: Some(out),
                kind: MirInstructionKind::StructNew {
                    type_name: "Integer".to_string(),
                    storage: MirStorageKind::Reference,
                    layout_id: None,
                    fields: vec![("value".to_string(), MirOperand::Value(value_arg))],
                },
            }],
            terminator: MirTerminator::Return { value: Some(MirOperand::Value(out)) },
        }],
        diagnostics: Vec::new(),
    };
    let mut submission = FragmentSubmission::default();
    submission.sum_types = vec![sum];
    let contract: ExecutableFunction = mir_fn.into();
    let body = lower_mir_to_clr_method(&submission, &operation, &contract)
        .expect("Integer { value } StructNew must rewrite to MsilInstructionOperand newobj");
    assert!(
        body.instructions.iter().any(|ins| match (&ins.opcode, &ins.operand) {
            (MsilOpcode::Newobj, Some(MsilInstructionOperand::Method(target))) => {
                target.name == ".ctor" && target.owner.as_deref() == Some("MsilInstructionOperand")
            }
            _ => false,
        }),
        "expected newobj MsilInstructionOperand::.ctor, got {body:?}"
    );
    assert!(
        !body.instructions.iter().any(|ins| match &ins.operand {
            Some(MsilInstructionOperand::Method(target)) => target.owner.as_deref() == Some("Integer"),
            _ => false,
        }),
        "must not emit newobj Integer::.ctor: {body:?}"
    );
}

#[test]
fn unite_payload_field_get_ignores_stale_algebraic_term_layout() {
    use nyar_types::{SumTypeLayout, SumVariantLayout};

    // Repro: parse_von_tokens Fine/Fail extract with MIR layout_id → AlgebraicTerm
    // must castclass/ldfld Result, never AlgebraicTerm (InvalidCastException).
    let operation = QualifiedName::new(vec![Identifier::new("parse_von_tokens")]);
    let scrutinee = MirValueRef(0);
    let payload_out = MirValueRef(1);
    let sum = SumTypeLayout {
        name: "Result".to_string(),
        is_unite: true,
        tag_width: 4,
        variants: vec![
            SumVariantLayout { name: "Fine".to_string(), tag: 0, payload_type: Some(nyar::NyarType::Named(Identifier::new("VonParsedValue"))) },
            SumVariantLayout { name: "Fail".to_string(), tag: 1, payload_type: Some(nyar::NyarType::Named(Identifier::new("VonDiagnostic"))) },
        ],
    };
    let plan = AggregateLayoutPlan {
        layouts: vec![
            AggregateLayout {
                id: 1,
                name: "AlgebraicTerm".to_string(),
                namespace: "nyar.optimizer".to_string(),
                storage: MirStorageKind::Value,
                size: 8,
                align: 8,
                fields: vec![FieldLayout { name: "payload".to_string(), ty: nyar::NyarType::Utf8, offset: 0, size: 8, align: 8 }],
            },
            AggregateLayout {
                id: 2,
                name: "Result".to_string(),
                namespace: String::new(),
                storage: MirStorageKind::Reference,
                size: 16,
                align: 8,
                fields: vec![
                    FieldLayout { name: "tag".to_string(), ty: nyar::NyarType::Integer32 { signed: true }, offset: 0, size: 4, align: 4 },
                    FieldLayout {
                        name: "payload".to_string(),
                        ty: nyar::NyarType::Named(Identifier::new("object")),
                        offset: 8,
                        size: 8,
                        align: 8,
                    },
                ],
            },
        ],
        value_type_names: BTreeSet::from(["AlgebraicTerm".to_string()]),
        type_name_to_layout: BTreeMap::from([("AlgebraicTerm".to_string(), 1), ("Result".to_string(), 2)]),
    };
    let mir_fn = MirFunction {
        symbol: "std::data::text::von::parse_von_tokens".to_string(),
        return_type: ValkyrieType::Named(Identifier::new("Result")),
        param_types: vec![ValkyrieType::Named(Identifier::new("Result"))],
        value_types: BTreeMap::from([
            (scrutinee, ValkyrieType::Named(Identifier::new("Result"))),
            (payload_out, ValkyrieType::Named(Identifier::new("VonParsedValue"))),
        ]),
        entry: MirBlockRef(0),
        values: vec![
            MirValue { id: scrutinee, origin: MirValueOrigin::Parameter { index: 0, name: "parsed".into() } },
            MirValue { id: payload_out, origin: MirValueOrigin::CallResult },
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
        blocks: vec![MirBlock {
            id: MirBlockRef(0),
            label: "entry".into(),
            parameters: vec![scrutinee],
            instructions: vec![MirInstruction {
                output: Some(payload_out),
                kind: MirInstructionKind::FieldGet {
                    object: MirOperand::Value(scrutinee),
                    field: "payload".to_string(),
                    storage: MirStorageKind::Reference,
                    // Stale: AlgebraicTerm, not Result.
                    layout_id: Some(1),
                },
            }],
            terminator: MirTerminator::Return { value: Some(MirOperand::Value(payload_out)) },
        }],
        diagnostics: Vec::new(),
    };
    let mut submission = FragmentSubmission::default();
    submission.sum_types = vec![sum];
    submission.aggregate_layouts = plan;
    let contract: ExecutableFunction = mir_fn.into();
    let body = lower_mir_to_clr_method(&submission, &operation, &contract)
        .expect("Result payload FieldGet must lower despite stale AlgebraicTerm layout_id");
    let mentions_algebraic = body.instructions.iter().any(|ins| match &ins.operand {
        Some(MsilInstructionOperand::Type(name)) | Some(MsilInstructionOperand::Field(name, _)) => name.contains("AlgebraicTerm"),
        _ => false,
    });
    assert!(!mentions_algebraic, "must not cast/ldfld AlgebraicTerm: {body:?}");
    assert!(
        body.instructions.iter().any(|ins| match (&ins.opcode, &ins.operand) {
            (MsilOpcode::Ldfld, Some(MsilInstructionOperand::Field(owner, field))) => {
                owner.contains("Result") && field == "payload"
            }
            _ => false,
        }),
        "expected ldfld Result::payload, got {body:?}"
    );
}
