    use super::*;
    use crate::contracts::{
        Block, BlockRef, Constant, DispatchKind, ExecutableFunction, Instruction, InstructionKind, Operand, ReceiverPassingKind,
        StorageKind, StorageKind as MirStorageKind, Terminator, ValueRef,
    };
    use crate::executable_provider::MirFunctionMapProvider;
    use nyar::{Identifier, NamePath, NyarType, QualifiedName};
    use nyar_types::{AggregateLayout, AggregateLayoutPlan, FieldLayout};
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std_data::binary::wasm::{TYPE_FORM_ARRAY, TYPE_FORM_STRUCT, VALTYPE_ANYREF, VALTYPE_I32, VALTYPE_REF, WasmGcOpcode, WasmMiscOpcode, WasmOpcode};

    #[test]
    fn mir_wasm_module_includes_memory_and_copy_for_aggregate_copy() {
        let mut submission = FragmentSubmission::default();
        submission.module_name = "demo".to_string();
        submission.aggregate_layouts = AggregateLayoutPlan {
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
            value_type_names: ["Point".to_string()].into_iter().collect(),
            type_name_to_layout: [("Point".to_string(), 1)].into_iter().collect(),
        };
        let mut mir_map = std::collections::BTreeMap::new();
        mir_map.insert(
            QualifiedName::new(vec![nyar::Identifier::new("main")]),
            ExecutableFunction {
                symbol: "main".to_string(),
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
                                name: "src".to_string(),
                                value: Operand::Constant(Constant::Int(0)),
                                ty: None,
                            },
                        },
                        Instruction {
                            output: None,
                            kind: InstructionKind::StoreVar {
                                name: "dst".to_string(),
                                value: Operand::Constant(Constant::Int(0)),
                                ty: None,
                            },
                        },
                        Instruction {
                            output: None,
                            kind: InstructionKind::AggregateCopy {
                                source: Operand::Symbol(NamePath::new(vec![nyar::Identifier::new("src")])),
                                dest: Operand::Symbol(NamePath::new(vec![nyar::Identifier::new("dst")])),
                                layout_id: 1,
                            },
                        },
                    ],
                    terminator: Terminator::Return { value: None },
                }],
                diagnostics: Vec::new(),
            },
        );
        submission.executable = Some(Arc::new(MirFunctionMapProvider::new(mir_map)));
        let module = lower_fragment_mir_to_wasm_module(&submission, "main");
        assert!(module.sections.iter().any(|section| section.id == 5), "memory section");
        let code = module.sections.iter().find(|section| section.id == 10).expect("code");
        assert!(code.bytes.windows(2).any(|window| window == [WasmOpcode::PrefixMisc.as_u8(), WasmMiscOpcode::MemoryCopy.as_u8()]), "memory.copy");
    }

    /// Builds a minimal MIR function with no parameters that returns a constant i32.
    fn make_leaf_function(symbol: &str, value: i64) -> ExecutableFunction {
        ExecutableFunction {
            symbol: symbol.to_string(),
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
                instructions: Vec::new(),
                terminator: Terminator::Return { value: Some(Operand::Constant(Constant::Int(value))) },
            }],
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn node_main_without_declared_imports_uses_stable_utf8_abi_imports() {
        let mut submission = FragmentSubmission::default();
        submission.module_name = "demo".to_string();
        submission.entry_operation = Some(QualifiedName::new(vec![nyar::Identifier::new("main")]));
        attach_functions(
            &mut submission,
            [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_leaf_function("main", 0))],
        );
        let module = lower_fragment_mir_to_wasm_module(&submission, "main");
        let import_bytes = module.sections.iter().find(|section| section.id == 2).map(|section| &section.bytes);
        assert!(import_bytes.is_some(), "Node JS-glue requires the stable utf8 host ABI");
    }

    fn attach_functions(
        submission: &mut FragmentSubmission,
        functions: impl IntoIterator<Item = (QualifiedName, ExecutableFunction)>,
    ) {
        let mir_map = functions.into_iter().collect();
        submission.executable = Some(Arc::new(MirFunctionMapProvider::new(mir_map)));
    }

    fn register_intrinsic(submission: &mut FragmentSubmission, symbol: &str, opcode: nyar_types::IntrinsicOpcode) {
        submission.intrinsics.insert(symbol.to_string(), opcode);
    }

    #[test]
    fn wasm_call_lowering_handles_non_builtin_call() {
        let mut submission = FragmentSubmission::default();
        submission.module_name = "demo".to_string();
        submission.entry_operation = Some(QualifiedName::new(vec![nyar::Identifier::new("main")]));
        let mut mir_map = std::collections::BTreeMap::new();
        mir_map.insert(
            QualifiedName::new(vec![nyar::Identifier::new("helper")]),
            make_leaf_function("helper", 42),
        );
        mir_map.insert(
            QualifiedName::new(vec![nyar::Identifier::new("main")]),
            ExecutableFunction {
                symbol: "main".to_string(),
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
                    instructions: vec![Instruction {
                        output: Some(ValueRef(0)),
                        kind: InstructionKind::Call {
                            dispatch: DispatchKind::Static,
                            callee: Operand::Symbol(NamePath::new(vec![nyar::Identifier::new("helper")])),
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
            },
        );
        submission.executable = Some(Arc::new(MirFunctionMapProvider::new(mir_map)));
        let module = lower_fragment_mir_to_wasm_module(&submission, "main");
        let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
        // WASM `call` opcode is 0x10; helper is first local function, wasm_index = import_count + 0.
        assert!(code.bytes.contains(&WasmOpcode::Call.as_u8()), "expected call (0x10) opcode in code section");
        let call_position = code.bytes.iter().position(|byte| *byte == WasmOpcode::Call.as_u8()).expect("call opcode");
        let expected_helper_index = super::import_count_for_main();
        assert_eq!(
            code.bytes[call_position + 1], expected_helper_index,
            "call targets function index {expected_helper_index} (helper)"
        );
    }

    #[test]
    fn wasm_value_receiver_passes_linear_memory_address() {
        let mut submission = FragmentSubmission::default();
        submission.module_name = "demo".to_string();
        submission.aggregate_layouts = AggregateLayoutPlan {
            layouts: vec![AggregateLayout {
                id: 1,
                name: "Point".to_string(),
                namespace: String::new(),
                storage: MirStorageKind::Value,
                size: 8,
                align: 4,
                fields: vec![
                    FieldLayout { name: "x".to_string(), ty: nyar::NyarType::Integer32 { signed: true }, offset: 0, size: 4, align: 4 },
                    FieldLayout { name: "y".to_string(), ty: nyar::NyarType::Integer32 { signed: true }, offset: 4, size: 4, align: 4 },
                ],
            }],
            value_type_names: ["Point".to_string()].into_iter().collect(),
            type_name_to_layout: [("Point".to_string(), 1)].into_iter().collect(),
        };
        submission.entry_operation = Some(QualifiedName::new(vec![nyar::Identifier::new("main")]));
        let mut mir_map = std::collections::BTreeMap::new();
        // consume accepts an i32 (Point address) and returns it.
        mir_map.insert(
            QualifiedName::new(vec![nyar::Identifier::new("consume")]),
            ExecutableFunction {
                symbol: "consume".to_string(),
                return_type: NyarType::Integer32 { signed: true },
                param_types: vec![NyarType::Integer32 { signed: true }],
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
                    parameters: vec![ValueRef(0)],
                    instructions: Vec::new(),
                    terminator: Terminator::Return { value: Some(Operand::Value(ValueRef(0))) },
                }],
                diagnostics: Vec::new(),
            },
        );
        // main constructs a Point value, then calls consume with it as a ByAddress receiver.
        mir_map.insert(
            QualifiedName::new(vec![nyar::Identifier::new("main")]),
            ExecutableFunction {
                symbol: "main".to_string(),
                return_type: NyarType::Integer32 { signed: true },
                param_types: Vec::new(),
                value_types: [(ValueRef(0), NyarType::Named(nyar::Identifier::new("Point")))]
                    .into_iter()
                    .collect(),
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
                                storage: MirStorageKind::Value,
                                layout_id: Some(1),
                                fields: vec![
                                    ("x".to_string(), Operand::Constant(Constant::Int(1))),
                                    ("y".to_string(), Operand::Constant(Constant::Int(2))),
                                ],
                            },
                        },
                        Instruction {
                            output: Some(ValueRef(1)),
                            kind: InstructionKind::Call {
                                dispatch: DispatchKind::Static,
                                callee: Operand::Symbol(NamePath::new(vec![nyar::Identifier::new("consume")])),
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
            },
        );
        submission.executable = Some(Arc::new(MirFunctionMapProvider::new(mir_map)));
        let module = lower_fragment_mir_to_wasm_module(&submission, "main");
        let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
        // WASM `call` is 0x10 + uleb index. Do not take the first 0x10 in the section ??
        // cabi/other bodies may contain 0x10 as an immediate.
        let expected_consume_index = super::import_count_for_main();
        let call_position = code
            .bytes
            .windows(2)
            .position(|window| window == [WasmOpcode::Call.as_u8(), expected_consume_index])
            .expect("expected call to consume (0x10 + import_count)");
        // A `local.get` (0x20) for the receiver must appear before the call, passing the
        // value-type's linear-memory address as the i32 parameter.
        let local_get_before_call = code.bytes[..call_position].iter().any(|byte| *byte == WasmOpcode::LocalGet.as_u8());
        assert!(local_get_before_call, "expected local.get (0x20) for receiver before call");
        // The consume function type should accept an i32 parameter (Point address).
        let type_section = module.sections.iter().find(|section| section.id == 1).expect("type section");
        assert!(type_section.bytes.contains(&VALTYPE_I32), "type section includes i32 param for consume");
    }

    /// Verifies that witness dispatch resolves the callee's type_index from the
    /// WASM type section instead of hardcoding type_index = 1.
    ///
    /// With two MIR functions sorted by BTreeMap key:
    /// - "helper" at position 0 ??type_index = 1 (entry, function_index = 0)
    /// - "other" at position 1 ??type_index = 2 (function_index = 1)
    ///
    /// The Call in "helper" targets "other" via Witness dispatch, so the
    /// emitted `call_indirect` must carry type_index = 2.
    #[test]
    fn wasm_witness_call_uses_resolved_type_index() {
        let mut submission = FragmentSubmission::default();
        submission.module_name = "demo".to_string();
        submission.entry_operation = Some(QualifiedName::new(vec![nyar::Identifier::new("helper")]));
        let mut mir_map = std::collections::BTreeMap::new();
        mir_map.insert(
            QualifiedName::new(vec![nyar::Identifier::new("other")]),
            make_leaf_function("other", 0),
        );
        mir_map.insert(
            QualifiedName::new(vec![nyar::Identifier::new("helper")]),
            ExecutableFunction {
                symbol: "helper".to_string(),
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
                    instructions: vec![Instruction {
                        output: Some(ValueRef(0)),
                        kind: InstructionKind::Call {
                            dispatch: DispatchKind::Witness,
                            callee: Operand::Symbol(NamePath::new(vec![nyar::Identifier::new("other")])),
                            arguments: Vec::new(),
                            witness: Some(Operand::Constant(Constant::Int(0))),
                            effect: None,
                            receiver_kind: None,
                            parameter_types: None,
                            intrinsic_opcode: None,
                        },
                    }],
                    terminator: Terminator::Return { value: Some(Operand::Value(ValueRef(0))) },
                }],
                diagnostics: Vec::new(),
            },
        );
        submission.executable = Some(Arc::new(MirFunctionMapProvider::new(mir_map)));
        let module = lower_fragment_mir_to_wasm_module(&submission, "main");
        let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
        let call_position = code.bytes.iter().position(|byte| *byte == WasmOpcode::CallIndirect.as_u8()).expect("expected call_indirect (WasmOpcode::CallIndirect.as_u8()) opcode");
        // The module reserves the runtime/ABI function types before local
        // function types. Resolve the second local function after that
        // reserved prefix and the Node host imports.
        let expected_type_index = 3 + super::import_count_for_main();
        assert_eq!(
            code.bytes[call_position + 1], expected_type_index,
            "call_indirect should target type_index {expected_type_index} (other)"
        );
    }

    // ?? wasm-gc ???????? ??????????????????????????????????

    /// ?????????(class) ??layout????wasm-gc ????
    ///
    /// `Foo` ??class????`value_type_names` ???storage = Reference??
    /// ?? `x: i32`, `y: i32`??
    fn make_reference_class_submission() -> FragmentSubmission {
        let mut submission = FragmentSubmission::default();
        submission.module_name = "demo".to_string();
        submission.aggregate_layouts = AggregateLayoutPlan {
            layouts: vec![AggregateLayout {
                id: 1,
                name: "Foo".to_string(),
                namespace: String::new(),
                storage: MirStorageKind::Reference,
                size: 8,
                align: 4,
                fields: vec![
                    FieldLayout { name: "x".to_string(), ty: nyar::NyarType::Integer32 { signed: true }, offset: 0, size: 4, align: 4 },
                    FieldLayout { name: "y".to_string(), ty: nyar::NyarType::Integer32 { signed: true }, offset: 4, size: 4, align: 4 },
                ],
            }],
            value_type_names: [].into_iter().collect(),
            type_name_to_layout: [("Foo".to_string(), 1)].into_iter().collect(),
        };
        submission
    }

    /// ??????? MIR ?????
    fn make_mir_function(
        symbol: &str,
        value_types: BTreeMap<ValueRef, NyarType>,
        instructions: Vec<Instruction>,
    ) -> ExecutableFunction {
        ExecutableFunction {
            symbol: symbol.to_string(),
            return_type: NyarType::Integer32 { signed: true },
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
                instructions,
                terminator: Terminator::Return { value: None },
            }],
            diagnostics: Vec::new(),
        }
    }

    /// StructNew(Reference) ????struct.new_default (0xFB 0x01) + struct.set (0xFB 0x05)??
    #[test]
    fn wasm_gc_struct_new_reference_emits_struct_new_default_and_set() {
        let mut submission = make_reference_class_submission();
        let value_types: BTreeMap<ValueRef, NyarType> =
            [(ValueRef(0), NyarType::Named(nyar::Identifier::new("Foo")))].into_iter().collect();
        attach_functions(
            &mut submission,
            [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
                "main",
                value_types,
                vec![Instruction {
                    output: Some(ValueRef(0)),
                    kind: InstructionKind::StructNew {
                        type_name: "Foo".to_string(),
                        storage: MirStorageKind::Reference,
                        layout_id: Some(1),
                        fields: vec![
                            ("x".to_string(), Operand::Constant(Constant::Int(1))),
                            ("y".to_string(), Operand::Constant(Constant::Int(2))),
                        ],
                    },
                }],
            ))],
        );
        let module = lower_fragment_mir_to_wasm_module(&submission, "main");
        let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
        assert!(
            code.bytes.windows(2).any(|window| window == [WasmOpcode::PrefixGc.as_u8(), WasmGcOpcode::StructNewDefault.as_u8()]),
            "expected struct.new_default (0xFB 0x01) for reference StructNew"
        );
        assert!(
            code.bytes.windows(2).any(|window| window == [WasmOpcode::PrefixGc.as_u8(), WasmGcOpcode::StructSet.as_u8()]),
            "expected struct.set (0xFB 0x05) for reference StructNew field assignment"
        );
    }

    /// FieldGet(Reference) ????struct.get (0xFB 0x02)??
    #[test]
    fn wasm_gc_field_get_reference_emits_struct_get() {
        let mut submission = make_reference_class_submission();
        let value_types: BTreeMap<ValueRef, NyarType> = [
            (ValueRef(0), NyarType::Named(nyar::Identifier::new("Foo"))),
            (ValueRef(1), NyarType::Integer32 { signed: true }),
        ]
        .into_iter()
        .collect();
        attach_functions(
            &mut submission,
            [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
                "main",
                value_types,
                vec![
                    Instruction {
                        output: Some(ValueRef(0)),
                        kind: InstructionKind::StructNew {
                            type_name: "Foo".to_string(),
                            storage: MirStorageKind::Reference,
                            layout_id: Some(1),
                            fields: vec![
                                ("x".to_string(), Operand::Constant(Constant::Int(1))),
                                ("y".to_string(), Operand::Constant(Constant::Int(2))),
                            ],
                        },
                    },
                    Instruction {
                        output: Some(ValueRef(1)),
                        kind: InstructionKind::FieldGet {
                            object: Operand::Value(ValueRef(0)),
                            field: "x".to_string(),
                            storage: MirStorageKind::Reference,
                            layout_id: Some(1),
                        },
                    },
                ],
            ))],
        );
        let module = lower_fragment_mir_to_wasm_module(&submission, "main");
        let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
        assert!(
            code.bytes.windows(2).any(|window| window == [WasmOpcode::PrefixGc.as_u8(), WasmGcOpcode::StructGet.as_u8()]),
            "expected struct.get (0xFB 0x02) for reference FieldGet"
        );
    }

    /// AggregateCopy(Reference) ??????:struct.new_default + struct.get + struct.set??
    #[test]
    fn wasm_gc_aggregate_copy_reference_emits_deep_copy() {
        let mut submission = make_reference_class_submission();
        let value_types: BTreeMap<ValueRef, NyarType> = [
            (ValueRef(0), NyarType::Named(nyar::Identifier::new("Foo"))),
            (ValueRef(1), NyarType::Named(nyar::Identifier::new("Foo"))),
        ]
        .into_iter()
        .collect();
        attach_functions(
            &mut submission,
            [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
                "main",
                value_types,
                vec![
                    Instruction {
                        output: Some(ValueRef(0)),
                        kind: InstructionKind::StructNew {
                            type_name: "Foo".to_string(),
                            storage: MirStorageKind::Reference,
                            layout_id: Some(1),
                            fields: vec![
                                ("x".to_string(), Operand::Constant(Constant::Int(1))),
                                ("y".to_string(), Operand::Constant(Constant::Int(2))),
                            ],
                        },
                    },
                    Instruction {
                        output: Some(ValueRef(1)),
                        kind: InstructionKind::StructNew {
                            type_name: "Foo".to_string(),
                            storage: MirStorageKind::Reference,
                            layout_id: Some(1),
                            fields: Vec::new(),
                        },
                    },
                    Instruction {
                        output: None,
                        kind: InstructionKind::AggregateCopy {
                            source: Operand::Value(ValueRef(0)),
                            dest: Operand::Value(ValueRef(1)),
                            layout_id: 1,
                        },
                    },
                ],
            ))],
        );
        let module = lower_fragment_mir_to_wasm_module(&submission, "main");
        let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
        // ??????struct.new_default (0xFB 0x01), struct.get (0xFB 0x02), struct.set (0xFB 0x05)??
        assert!(
            code.bytes.windows(2).any(|window| window == [WasmOpcode::PrefixGc.as_u8(), WasmGcOpcode::StructNewDefault.as_u8()]),
            "expected struct.new_default (0xFB 0x01) for AggregateCopy deep copy"
        );
        assert!(
            code.bytes.windows(2).any(|window| window == [WasmOpcode::PrefixGc.as_u8(), WasmGcOpcode::StructGet.as_u8()]),
            "expected struct.get (0xFB 0x02) for AggregateCopy deep copy source field read"
        );
        assert!(
            code.bytes.windows(2).any(|window| window == [WasmOpcode::PrefixGc.as_u8(), WasmGcOpcode::StructSet.as_u8()]),
            "expected struct.set (0xFB 0x05) for AggregateCopy deep copy dest field write"
        );
    }

    /// ArrayNew (heap [T]) ????array.new_default (0xFB 0x07)??
    #[test]
    fn wasm_gc_array_new_emits_array_new_default() {
        let mut submission = make_reference_class_submission();
        let value_types: BTreeMap<ValueRef, NyarType> =
            [(ValueRef(0), NyarType::Array(Box::new(NyarType::Integer32 { signed: true })))]
                .into_iter()
                .collect();
        attach_functions(
            &mut submission,
            [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
                "main",
                value_types,
                vec![Instruction {
                    output: Some(ValueRef(0)),
                    kind: InstructionKind::ArrayNew {
                        element_type: NyarType::Integer32 { signed: true },
                        length: Operand::Constant(Constant::Int(3)),
                    },
                }],
            ))],
        );
        let module = lower_fragment_mir_to_wasm_module(&submission, "_start");
        let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
        assert!(
            code.bytes.windows(2).any(|window| window == [WasmOpcode::PrefixGc.as_u8(), WasmGcOpcode::ArrayNewDefault.as_u8()]),
            "expected array.new_default (0xFB 0x07) for heap ArrayNew"
        );
    }

    /// ArrayLiteral (heap [T]) ????array.new_fixed (0xFB 0x08)??
    #[test]
    fn wasm_gc_array_literal_emits_array_new_fixed() {
        let mut submission = make_reference_class_submission();
        let value_types: BTreeMap<ValueRef, NyarType> =
            [(ValueRef(0), NyarType::Array(Box::new(NyarType::Integer32 { signed: true })))]
                .into_iter()
                .collect();
        attach_functions(
            &mut submission,
            [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
                "main",
                value_types,
                vec![Instruction {
                    output: Some(ValueRef(0)),
                    kind: InstructionKind::ArrayLiteral {
                        element_type: NyarType::Integer32 { signed: true },
                        items: vec![
                            Operand::Constant(Constant::Int(10)),
                            Operand::Constant(Constant::Int(20)),
                            Operand::Constant(Constant::Int(30)),
                        ],
                    },
                }],
            ))],
        );
        let module = lower_fragment_mir_to_wasm_module(&submission, "_start");
        let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
        assert!(
            code.bytes.windows(2).any(|window| window == [WasmOpcode::PrefixGc.as_u8(), WasmGcOpcode::ArrayNewFixed.as_u8()]),
            "expected array.new_fixed (0xFB 0x08) for heap ArrayLiteral"
        );
    }

    /// Node 轨：`[utf8]` 数组元必须是 i32 句柄，不能登记成 anyref。
    /// 否则 `const_utf8` 返回的 i32 进 `array.new_fixed [anyref]` 会在 V8 校验失败。
    #[test]
    fn wasm_gc_utf8_array_literal_registers_i32_arraytype() {
        let mut submission = make_reference_class_submission();
        let utf8_ty = NyarType::Utf8;
        let value_types: BTreeMap<ValueRef, NyarType> =
            [(ValueRef(0), NyarType::Array(Box::new(utf8_ty.clone())))].into_iter().collect();
        attach_functions(
            &mut submission,
            [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
                "main",
                value_types,
                vec![Instruction {
                    output: Some(ValueRef(0)),
                    kind: InstructionKind::ArrayLiteral {
                        element_type: utf8_ty,
                        items: vec![
                            Operand::Constant(Constant::Utf8("clr".into())),
                            Operand::Constant(Constant::Utf8("jvm".into())),
                        ],
                    },
                }],
            ))],
        );
        let module = lower_fragment_mir_to_wasm_module(&submission, "_start");
        let type_section = module.sections.iter().find(|section| section.id == 1).expect("type section");
        // arraytype form = 0x5E；随后 field mutability + valtype。
        // Node 轨期望元素为 i32 (0x7F)，禁止 anyref (0x6E)。
        let bytes = &type_section.bytes;
        let mut found_i32_array = false;
        let mut found_anyref_array = false;
        let mut i = 0;
        while i + 1 < bytes.len() {
            if bytes[i] == TYPE_FORM_ARRAY {
                // encode_arraytype_raw: [0x5E, element, mut]
                let valtype = bytes[i + 1];
                if valtype == VALTYPE_I32 {
                    found_i32_array = true;
                }
                if valtype == VALTYPE_ANYREF {
                    found_anyref_array = true;
                }
            }
            i += 1;
        }
        assert!(found_i32_array, "expected arraytype [i32] for explicit utf8 heap array under Node i32-handle ABI");
        assert!(
            !found_anyref_array,
            "explicit utf8 must not register arraytype [anyref] when js_glue_utf8_as_anyref=false"
        );
    }

    /// ??????????layout ?? structtype (0x5F)??
    #[test]
    fn wasm_gc_type_section_includes_structtype_for_reference_layout() {
        let mut submission = make_reference_class_submission();
        attach_functions(
            &mut submission,
            [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
                "main",
                [(ValueRef(0), NyarType::Named(nyar::Identifier::new("Foo")))].into_iter().collect(),
                vec![Instruction {
                    output: Some(ValueRef(0)),
                    kind: InstructionKind::StructNew {
                        type_name: "Foo".to_string(),
                        storage: MirStorageKind::Reference,
                        layout_id: Some(1),
                        fields: vec![("x".to_string(), Operand::Constant(Constant::Int(1)))],
                    },
                }],
            ))],
        );
        let module = lower_fragment_mir_to_wasm_module(&submission, "main");
        let type_section = module.sections.iter().find(|section| section.id == 1).expect("type section");
        assert!(
            type_section.bytes.contains(&TYPE_FORM_STRUCT),
            "expected structtype (0x5F) in type section for reference layout"
        );
    }

    /// 绫诲瀷娈靛簲涓?heap [T] element_type 娉ㄥ唽 arraytype (0x5E)銆?
    #[test]
    fn wasm_gc_type_section_includes_arraytype_for_heap_array() {
        let mut submission = make_reference_class_submission();
        attach_functions(
            &mut submission,
            [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
                "main",
                [(ValueRef(0), NyarType::Array(Box::new(NyarType::Integer32 { signed: true })))]
                    .into_iter()
                    .collect(),
                vec![Instruction {
                    output: Some(ValueRef(0)),
                    kind: InstructionKind::ArrayNew {
                        element_type: NyarType::Integer32 { signed: true },
                        length: Operand::Constant(Constant::Int(3)),
                    },
                }],
            ))],
        );
        let module = lower_fragment_mir_to_wasm_module(&submission, "main");
        let type_section = module.sections.iter().find(|section| section.id == 1).expect("type section");
        assert!(
            type_section.bytes.contains(&TYPE_FORM_ARRAY),
            "expected arraytype (0x5E) in type section for heap array element type on Node/main"
        );
    }

    /// ???????? locals ??????anyref (0x6E)??
    #[test]
    fn wasm_gc_locals_include_anyref_for_reference_output() {
        let mut submission = make_reference_class_submission();
        attach_functions(
            &mut submission,
            [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
                "main",
                [(ValueRef(0), NyarType::Named(nyar::Identifier::new("Foo")))].into_iter().collect(),
                vec![Instruction {
                    output: Some(ValueRef(0)),
                    kind: InstructionKind::StructNew {
                        type_name: "Foo".to_string(),
                        storage: MirStorageKind::Reference,
                        layout_id: Some(1),
                        fields: vec![("x".to_string(), Operand::Constant(Constant::Int(1)))],
                    },
                }],
            ))],
        );
        let module = lower_fragment_mir_to_wasm_module(&submission, "main");
        let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
        // locals ????code body ??? <count> <type>...
        // GC struct ???? typed ref (0x64)???? anyref (0x6E)??
        assert!(
            code.bytes.contains(&WASM_GC_ANYREF) || code.bytes.contains(&VALTYPE_REF),
            "expected anyref (0x6E) or struct ref (0x64) in locals for reference output"
        );
    }

    /// Value layout + StructNew(Reference)：须按 MIR Reference 用法注册 structtype，
    /// AggregateCopy 才能走深拷贝而非 missing gc structtype。
    #[test]
    fn wasm_gc_registers_value_layout_used_as_reference_in_mir() {
        let mut submission = FragmentSubmission::default();
        submission.module_name = "demo".to_string();
        submission.aggregate_layouts = AggregateLayoutPlan {
            layouts: vec![AggregateLayout {
                id: 1,
                name: "Diag".to_string(),
                namespace: String::new(),
                storage: MirStorageKind::Value,
                size: 8,
                align: 4,
                fields: vec![
                    FieldLayout { name: "x".to_string(), ty: nyar::NyarType::Integer32 { signed: true }, offset: 0, size: 4, align: 4 },
                    FieldLayout { name: "y".to_string(), ty: nyar::NyarType::Integer32 { signed: true }, offset: 4, size: 4, align: 4 },
                ],
            }],
            value_type_names: ["Diag".to_string()].into_iter().collect(),
            type_name_to_layout: [("Diag".to_string(), 1)].into_iter().collect(),
        };
        let value_types: BTreeMap<ValueRef, NyarType> = [
            (ValueRef(0), NyarType::Named(nyar::Identifier::new("Diag"))),
            (ValueRef(1), NyarType::Named(nyar::Identifier::new("Diag"))),
        ]
        .into_iter()
        .collect();
        attach_functions(
            &mut submission,
            [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
                "main",
                value_types,
                vec![
                    Instruction {
                        output: Some(ValueRef(0)),
                        kind: InstructionKind::StructNew {
                            type_name: "Diag".to_string(),
                            storage: MirStorageKind::Reference,
                            layout_id: Some(1),
                            fields: vec![
                                ("x".to_string(), Operand::Constant(Constant::Int(1))),
                                ("y".to_string(), Operand::Constant(Constant::Int(2))),
                            ],
                        },
                    },
                    Instruction {
                        output: Some(ValueRef(1)),
                        kind: InstructionKind::StructNew {
                            type_name: "Diag".to_string(),
                            storage: MirStorageKind::Reference,
                            layout_id: Some(1),
                            fields: Vec::new(),
                        },
                    },
                    Instruction {
                        output: None,
                        kind: InstructionKind::AggregateCopy {
                            source: Operand::Value(ValueRef(0)),
                            dest: Operand::Value(ValueRef(1)),
                            layout_id: 1,
                        },
                    },
                ],
            ))],
        );
        let module = lower_fragment_mir_to_wasm_module(&submission, "main");
        let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
        assert!(
            code.bytes.windows(2).any(|window| window == [WasmOpcode::PrefixGc.as_u8(), WasmGcOpcode::StructNewDefault.as_u8()]),
            "expected struct.new_default for Value layout used as Reference in MIR"
        );
        // The code section also contains synthesized runtime helpers, so a
        // module-wide search for `memory.copy` is not a function-local proof.
        // The lowering path is selected from the reference locals above; the
        // GC struct opcodes and metadata are the contract asserted here.
        let gc_meta = module
            .sections
            .iter()
            .find(|section| section.name.as_deref() == Some("nyar.wasm.gc_layouts"))
            .expect("gc layout metadata");
        let payload = String::from_utf8_lossy(&gc_meta.bytes);
        assert!(
            payload.contains("struct\t1\tDiag\t") && payload.contains("\tregistered\t"),
            "expected Diag layout registered despite layout.storage=Value: {payload}"
        );
    }

    /// AggregateCopy(Value) ???? memory.copy (0xFC 0x0A)????wasm-gc ????
    /// ??????????????????????
    #[test]
    fn wasm_gc_value_aggregate_copy_still_uses_memory_copy() {
        let mut submission = FragmentSubmission::default();
        submission.module_name = "demo".to_string();
        submission.aggregate_layouts = AggregateLayoutPlan {
            layouts: vec![AggregateLayout {
                id: 1,
                name: "Point".to_string(),
                namespace: String::new(),
                storage: MirStorageKind::Value,
                size: 8,
                align: 4,
                fields: vec![
                    FieldLayout { name: "x".to_string(), ty: nyar::NyarType::Integer32 { signed: true }, offset: 0, size: 4, align: 4 },
                    FieldLayout { name: "y".to_string(), ty: nyar::NyarType::Integer32 { signed: true }, offset: 4, size: 4, align: 4 },
                ],
            }],
            value_type_names: ["Point".to_string()].into_iter().collect(),
            type_name_to_layout: [("Point".to_string(), 1)].into_iter().collect(),
        };
        attach_functions(
            &mut submission,
            [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
                "main",
                BTreeMap::new(),
                vec![
                    Instruction {
                        output: None,
                        kind: InstructionKind::StoreVar {
                            name: "src".to_string(),
                            value: Operand::Constant(Constant::Int(0)),
                            ty: None,
                        },
                    },
                    Instruction {
                        output: None,
                        kind: InstructionKind::StoreVar {
                            name: "dst".to_string(),
                            value: Operand::Constant(Constant::Int(0)),
                            ty: None,
                        },
                    },
                    Instruction {
                        output: None,
                        kind: InstructionKind::AggregateCopy {
                            source: Operand::Symbol(NamePath::new(vec![nyar::Identifier::new("src")])),
                            dest: Operand::Symbol(NamePath::new(vec![nyar::Identifier::new("dst")])),
                            layout_id: 1,
                        },
                    },
                ],
            ))],
        );
        let module = lower_fragment_mir_to_wasm_module(&submission, "main");
        let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
        assert!(
            code.bytes.windows(2).any(|window| window == [WasmOpcode::PrefixMisc.as_u8(), WasmMiscOpcode::MemoryCopy.as_u8()]),
            "expected memory.copy (0xFC 0x0A) for value AggregateCopy"
        );
    }

    /// `array.get` intrinsic should emit wasm-gc array.get (0xFB 0x0B).
    #[test]
    fn wasm_gc_array_get_emits_array_get() {
        let mut submission = make_reference_class_submission();
        let value_types: BTreeMap<ValueRef, NyarType> = [
            (ValueRef(0), NyarType::Array(Box::new(NyarType::Integer32 { signed: true }))),
            (ValueRef(1), NyarType::Integer32 { signed: true }),
        ]
        .into_iter()
        .collect();
        attach_functions(
            &mut submission,
            [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
                "main",
                value_types,
                vec![
                    Instruction {
                        output: Some(ValueRef(0)),
                        kind: InstructionKind::ArrayNew {
                            element_type: NyarType::Integer32 { signed: true },
                            length: Operand::Constant(Constant::Int(3)),
                        },
                    },
                    Instruction {
                        output: Some(ValueRef(1)),
                        kind: InstructionKind::Call {
                            dispatch: DispatchKind::Static,
                            callee: Operand::Symbol(NamePath::new(vec![nyar::Identifier::new("array.get")])),
                            arguments: vec![Operand::Value(ValueRef(0)), Operand::Constant(Constant::Int(1))],
                            witness: None,
                            effect: None,
                            receiver_kind: None,
                            parameter_types: None,
                            intrinsic_opcode: None,
                        },
                    },
                ],
            ))],
        );
                register_intrinsic(&mut submission, "array.get", nyar_types::IntrinsicOpcode::ArrayGet);
let module = lower_fragment_mir_to_wasm_module(&submission, "_start");
        let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
        assert!(
            code.bytes.windows(2).any(|window| window == [WasmOpcode::PrefixGc.as_u8(), WasmGcOpcode::ArrayGet.as_u8()]),
            "expected array.get (0xFB 0x0B) for array.get intrinsic"
        );
    }

    /// `array.set` intrinsic should emit wasm-gc array.set (0xFB 0x0E).
    #[test]
    fn wasm_gc_array_set_emits_array_set() {
        let mut submission = make_reference_class_submission();
        let value_types: BTreeMap<ValueRef, NyarType> =
            [(ValueRef(0), NyarType::Array(Box::new(NyarType::Integer32 { signed: true })))]
                .into_iter()
                .collect();
        attach_functions(
            &mut submission,
            [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
                "main",
                value_types,
                vec![
                    Instruction {
                        output: Some(ValueRef(0)),
                        kind: InstructionKind::ArrayNew {
                            element_type: NyarType::Integer32 { signed: true },
                            length: Operand::Constant(Constant::Int(3)),
                        },
                    },
                    Instruction {
                        output: None,
                        kind: InstructionKind::Call {
                            dispatch: DispatchKind::Static,
                            callee: Operand::Symbol(NamePath::new(vec![nyar::Identifier::new("array.set")])),
                            arguments: vec![
                                Operand::Value(ValueRef(0)),
                                Operand::Constant(Constant::Int(1)),
                                Operand::Constant(Constant::Int(42)),
                            ],
                            witness: None,
                            effect: None,
                            receiver_kind: None,
                            parameter_types: None,
                            intrinsic_opcode: None,
                        },
                    },
                ],
            ))],
        );
                register_intrinsic(&mut submission, "array.set", nyar_types::IntrinsicOpcode::ArraySet);
let module = lower_fragment_mir_to_wasm_module(&submission, "_start");
        let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
        assert!(
            code.bytes.windows(2).any(|window| window == [WasmOpcode::PrefixGc.as_u8(), WasmGcOpcode::ArraySet.as_u8()]),
            "expected array.set (0xFB 0x0E) for array.set intrinsic"
        );
    }

    /// `array.len` intrinsic should emit wasm-gc array.len (0xFB 0x0F).
    #[test]
    fn wasm_gc_array_length_emits_array_len() {
        let mut submission = make_reference_class_submission();
        let value_types: BTreeMap<ValueRef, NyarType> = [
            (ValueRef(0), NyarType::Array(Box::new(NyarType::Integer32 { signed: true }))),
            (ValueRef(1), NyarType::Integer32 { signed: true }),
        ]
        .into_iter()
        .collect();
        attach_functions(
            &mut submission,
            [(QualifiedName::new(vec![nyar::Identifier::new("main")]), make_mir_function(
                "main",
                value_types,
                vec![
                    Instruction {
                        output: Some(ValueRef(0)),
                        kind: InstructionKind::ArrayNew {
                            element_type: NyarType::Integer32 { signed: true },
                            length: Operand::Constant(Constant::Int(3)),
                        },
                    },
                    Instruction {
                        output: Some(ValueRef(1)),
                        kind: InstructionKind::Call {
                            dispatch: DispatchKind::Static,
                            callee: Operand::Symbol(NamePath::new(vec![nyar::Identifier::new("array.len")])),
                            arguments: vec![Operand::Value(ValueRef(0))],
                            witness: None,
                            effect: None,
                            receiver_kind: None,
                            parameter_types: None,
                            intrinsic_opcode: None,
                        },
                    },
                ],
            ))],
        );
                register_intrinsic(&mut submission, "array.len", nyar_types::IntrinsicOpcode::ArrayLen);
let module = lower_fragment_mir_to_wasm_module(&submission, "_start");
        let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
        assert!(
            code.bytes.windows(2).any(|window| window == [WasmOpcode::PrefixGc.as_u8(), WasmGcOpcode::ArrayLen.as_u8()]),
            "expected array.len (0xFB 0x0F) for array.len intrinsic"
        );
    }
