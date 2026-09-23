    use std::collections::BTreeMap;

    use nyar::{
        BinaryTarget, CapabilityTag, ControlFlowPayload, Identifier, QualifiedName, RewriteTheory, SuspendFunctionArtifact,
        SuspendStateArtifact, SuspendWitnessBinding, TheoryBundle, WitnessMethodSlotSubmission, WitnessSubmission,
        backends::{CompilationOptions, TargetCodeGenBackend, clr::ClrImageKind},
    };
    use crate::nyar_backend_clr::{ClrBinaryBackendInput, MsilInstructionOperand, MsilOpcode, PeWriter, PeWriterOptions};
    use crate::contracts::{Block, BlockRef, Constant, ExecutableFunction, Operand, Terminator};
    use crate::executable_provider::MirFunctionMapProvider;
    use nyar::NyarType;
    use std::sync::Arc;
    use tempfile::tempdir;

    use super::*;
    use crate::{FragmentSubmission, lowering::clr};

    fn attach_minimal_mir(submission: &mut FragmentSubmission) {
        let mut functions = BTreeMap::<QualifiedName, ExecutableFunction>::new();
        for operation in &submission.exported_operations {
            let function = ExecutableFunction {
                symbol: operation.to_string(),
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
                    terminator: Terminator::Return {
                        value: Some(Operand::Constant(Constant::Int(0))),
                    },
                }],
                diagnostics: Vec::new(),
            };
            functions.insert(operation.clone(), function);
        }
        submission.executable = Some(Arc::new(MirFunctionMapProvider::new(functions)));
    }

    fn delegate_submission() -> FragmentSubmission {
        let mut submission = FragmentSubmission {
            module_name: "demo".to_string(),
            fragment_id: Identifier::new("suspend"),
            exported_operations: vec![QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("gen")])],
            required_capabilities: vec![CapabilityTag::new("suspend"), CapabilityTag::new("trait-witness")],
            theory_bundle: TheoryBundle { shared: RewriteTheory::default(), fragment: RewriteTheory::default() },
            entry_operation: Some(QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("gen")])),
            external_import_links: BTreeMap::new(),
            external_call_edges: Vec::new(),
            internal_call_edges: Vec::new(),
            operation_literal_returns: Default::default(),
            operation_void_returns: Default::default(),
            witness_tables: vec![WitnessSubmission {
                type_name: "CounterIterator".to_string(),
                trait_name: "Iterator".to_string(),
                table_label: "witness_table_CounterIterator_Iterator".to_string(),
                fat_ptr_label: "witness_fat_CounterIterator_Iterator".to_string(),
                methods: vec![WitnessMethodSlotSubmission {
                    method_name: "next".to_string(),
                    impl_symbol: "witness_CounterIterator_Iterator_next".to_string(),
                    method_index: 0,
                }],
                result_literal: String::new(),
            }],
            witness_calls: Vec::new(),
            control_flow: Some(ControlFlowPayload {
                functions: vec![SuspendFunctionArtifact {
                    symbol: QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("gen")]),
                    state_machine_type: "GenStateMachine".to_string(),
                    state_field: "__state".to_string(),
                    frame_fields: vec!["__witness_payload_0".to_string()],
                    dispatch_cases: Vec::new(),
                    states: vec![SuspendStateArtifact {
                        state_id: 0,
                        effect: "DelegateYield".to_string(),
                        resume_case_key: 1,
                        frame_carrier: "this".to_string(),
                        spill_fields: vec!["__witness_payload_0".to_string()],
                        suspend_block_label: "yield_from".to_string(),
                        resume_block_label: "resume".to_string(),
                        resume_parameter_count: 0,
                        witness_bindings: vec![SuspendWitnessBinding {
                            trait_name: "Iterator".to_string(),
                            method_name: "next".to_string(),
                            method_index: 0,
                            type_name: Some("CounterIterator".to_string()),
                            impl_symbol: Some("witness_CounterIterator_Iterator_next".to_string()),
                        }],
                        continuation_index: None,
                    }],
                    continuations: Vec::new(),
                }],
            }),
            suspend_runtime: None,
        ..Default::default()
        };
        attach_minimal_mir(&mut submission);
        submission
    }

    #[test]
    fn writes_pe_for_yield_state_machine_module() {
        let symbol = QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("main")]);
        let mut submission = FragmentSubmission {
            module_name: "demo".to_string(),
            fragment_id: Identifier::new("suspend"),
            exported_operations: vec![symbol.clone()],
            required_capabilities: vec![CapabilityTag::new("suspend")],
            theory_bundle: TheoryBundle { shared: RewriteTheory::default(), fragment: RewriteTheory::default() },
            entry_operation: Some(symbol.clone()),
            external_import_links: BTreeMap::new(),
            external_call_edges: Vec::new(),
            internal_call_edges: Vec::new(),
            operation_literal_returns: Default::default(),
            operation_void_returns: Default::default(),
            witness_tables: Vec::new(),
            witness_calls: Vec::new(),
            control_flow: Some(ControlFlowPayload {
                functions: vec![SuspendFunctionArtifact {
                    symbol: symbol.clone(),
                    state_machine_type: "MainStateMachine".to_string(),
                    state_field: "__state".to_string(),
                    frame_fields: Vec::new(),
                    dispatch_cases: Vec::new(),
                    states: vec![SuspendStateArtifact {
                        state_id: 0,
                        effect: "Yield".to_string(),
                        resume_case_key: 1,
                        frame_carrier: "this".to_string(),
                        spill_fields: Vec::new(),
                        suspend_block_label: "yield_0".to_string(),
                        resume_block_label: "resume_0".to_string(),
                        resume_parameter_count: 0,
                        witness_bindings: Vec::new(),
                        continuation_index: None,
                    }],
                    continuations: Vec::new(),
                }],
            }),
            suspend_runtime: None,
        ..Default::default()
        };
        attach_minimal_mir(&mut submission);
        let mut module = clr::lower_fragment_to_msil(&submission).expect("CLR lowering");
        augment_msil_with_suspend(&submission, &mut module);
        PeWriter::new(PeWriterOptions {
            assembly_name: module.assembly.name.clone(),
            module_name: "demo.dll".to_string(),
            image_kind: ClrImageKind::Executable,
        })
        .write_module(&module)
        .expect("PE write should succeed for suspend module");
    }

    #[test]
    fn witness_receiver_load_emits_ldfld_when_spill_present() {
        let submission = delegate_submission();
        let mut module = clr::lower_fragment_to_msil(&submission).expect("CLR lowering");
        augment_msil_with_suspend(&submission, &mut module);
        let move_next =
            module.types.iter().flat_map(|ty| ty.methods.iter()).find(|method| method.method.name == "MoveNext").expect("move next");
        assert!(move_next.instructions.iter().any(|instruction| matches!(
            &instruction.operand,
            Some(MsilInstructionOperand::Field(_, field)) if field == "__witness_payload_0"
                && instruction.opcode == MsilOpcode::Ldfld
        )));
    }

    #[test]
    fn await_future_msil_poll_branches() {
        let submission = await_submission();
        let mut module = clr::lower_fragment_to_msil(&submission).expect("CLR lowering");
        augment_msil_with_suspend(&submission, &mut module);
        let move_next =
            module.types.iter().flat_map(|ty| ty.methods.iter()).find(|method| method.method.name == "MoveNext").expect("move next");
        assert!(move_next.instructions.iter().any(|instruction| instruction.opcode == MsilOpcode::Brfalse));
    }

    /// spec Task 3.2：`await` 在 `Future::poll` 返回 ready 后必须显式调用 `Future::output`
    /// 取出恢复值 `T`，并将其存入 `__current` 字段供 resume 路径读取。
    #[test]
    fn await_msil_calls_output_after_poll_true_and_stores_resume_value() {
        let mut submission = await_submission();
        submission.witness_tables[0].methods.push(WitnessMethodSlotSubmission {
            method_name: "output".to_string(),
            impl_symbol: "witness_ReadyFuture_Future_output".to_string(),
            method_index: 1,
        });
        let states = &mut submission.control_flow.as_mut().expect("payload").functions[0].states;
        states[0].witness_bindings.push(SuspendWitnessBinding {
            trait_name: "Future".to_string(),
            method_name: "output".to_string(),
            method_index: 1,
            type_name: Some("ReadyFuture".to_string()),
            impl_symbol: Some("witness_ReadyFuture_Future_output".to_string()),
        });

        let mut module = clr::lower_fragment_to_msil(&submission).expect("CLR lowering");
        augment_msil_with_suspend(&submission, &mut module);
        let move_next =
            module.types.iter().flat_map(|ty| ty.methods.iter()).find(|method| method.method.name == "MoveNext").expect("move next");

        let poll_index = move_next
            .instructions
            .iter()
            .position(|instruction| matches!(
                &instruction.operand,
                Some(MsilInstructionOperand::Method(target)) if target.name == "witness_ReadyFuture_Future_poll"
            ))
            .expect("expected a Call to Future::poll");
        let brfalse_index = move_next
            .instructions
            .iter()
            .skip(poll_index)
            .position(|instruction| instruction.opcode == MsilOpcode::Brfalse)
            .expect("expected Brfalse after poll")
            + poll_index;
        let output_index = move_next
            .instructions
            .iter()
            .skip(brfalse_index)
            .position(|instruction| matches!(
                &instruction.operand,
                Some(MsilInstructionOperand::Method(target)) if target.name == "witness_ReadyFuture_Future_output"
            ))
            .expect("expected a Call to Future::output after Brfalse")
            + brfalse_index;
        assert!(output_index > brfalse_index, "output call must follow the poll Brfalse branch");
        let stfld_index = move_next
            .instructions
            .iter()
            .skip(output_index)
            .position(|instruction| matches!(
                &instruction.operand,
                Some(MsilInstructionOperand::Field(_, field)) if field == "__current" && instruction.opcode == MsilOpcode::Stfld
            ))
            .expect("expected Stfld __current after output call")
            + output_index;
        assert!(stfld_index > output_index, "Stfld __current must follow the output call");
    }

    #[test]
    fn await_without_witness_binding_falls_back_to_done() {
        let mut submission = await_submission();
        submission.control_flow.as_mut().unwrap().functions[0].states[0].witness_bindings.clear();
        let mut module = clr::lower_fragment_to_msil(&submission).expect("CLR lowering");
        augment_msil_with_suspend(&submission, &mut module);
        let move_next =
            module.types.iter().flat_map(|ty| ty.methods.iter()).find(|method| method.method.name == "MoveNext").expect("move next");
        assert!(move_next.instructions.iter().any(|instruction| instruction.opcode == MsilOpcode::LdcI4_0));
    }

    #[test]
    fn yield_from_iterator_msil_has_null_branch() {
        let submission = delegate_submission();
        let mut module = clr::lower_fragment_to_msil(&submission).expect("CLR lowering");
        augment_msil_with_suspend(&submission, &mut module);
        let move_next =
            module.types.iter().flat_map(|ty| ty.methods.iter()).find(|method| method.method.name == "MoveNext").expect("move next");
        assert!(move_next.instructions.iter().any(|instruction| instruction.opcode == MsilOpcode::Brfalse));
        assert!(move_next.instructions.iter().any(|instruction| matches!(
            &instruction.operand,
            Some(MsilInstructionOperand::Method(target)) if target.name == "witness_CounterIterator_Iterator_next"
        )));
    }

    #[test]
    fn witness_payload_initialized_in_ctor() {
        let submission = delegate_submission();
        let mut module = clr::lower_fragment_to_msil(&submission).expect("CLR lowering");
        augment_msil_with_suspend(&submission, &mut module);
        let ctor = module.types.iter().flat_map(|ty| ty.methods.iter()).find(|method| method.method.name == ".ctor").expect("ctor");
        assert!(ctor.instructions.iter().any(|instruction| instruction.opcode == MsilOpcode::Newarr));
    }

    #[test]
    fn delegate_yield_stores_current_before_yield() {
        let submission = delegate_submission();
        let mut module = clr::lower_fragment_to_msil(&submission).expect("CLR lowering");
        augment_msil_with_suspend(&submission, &mut module);
        let move_next =
            module.types.iter().flat_map(|ty| ty.methods.iter()).find(|method| method.method.name == "MoveNext").expect("move next");
        assert!(move_next.instructions.iter().any(|instruction| matches!(
            &instruction.operand,
            Some(MsilInstructionOperand::Field(_, field)) if field == "__current" && instruction.opcode == MsilOpcode::Stfld
        )));
    }

    #[test]
    fn dispatch_cases_nonempty_path() {
        let submission = delegate_submission();
        let artifact = &submission.control_flow.as_ref().expect("payload").functions[0];
        assert_eq!(dispatch_case_keys(artifact), vec![0, 1]);
    }

    #[test]
    fn delegate_yield_emits_witness_call_in_msil() {
        let submission = delegate_submission();
        let mut module = clr::lower_fragment_to_msil(&submission).expect("CLR lowering");
        augment_msil_with_suspend(&submission, &mut module);
        assert!(module.global_methods.iter().any(|method| method.method.name == "witness_CounterIterator_Iterator_next"));
        let move_next =
            module.types.iter().flat_map(|ty| ty.methods.iter()).find(|method| method.method.name == "MoveNext").expect("move next");
        assert!(move_next.instructions.iter().any(|instruction| matches!(
            &instruction.operand,
            Some(MsilInstructionOperand::Method(target)) if target.name == "witness_CounterIterator_Iterator_next"
        )));
        PeWriter::new(PeWriterOptions {
            assembly_name: module.assembly.name.clone(),
            module_name: "demo.dll".to_string(),
            image_kind: ClrImageKind::Executable,
        })
        .write_module(&module)
        .expect("PE write");
    }

    fn await_submission() -> FragmentSubmission {
        let mut submission = FragmentSubmission {
            module_name: "demo".to_string(),
            fragment_id: Identifier::new("suspend"),
            exported_operations: vec![QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("async_fn")])],
            required_capabilities: vec![CapabilityTag::new("suspend"), CapabilityTag::new("trait-witness")],
            theory_bundle: TheoryBundle { shared: RewriteTheory::default(), fragment: RewriteTheory::default() },
            entry_operation: Some(QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("async_fn")])),
            external_import_links: BTreeMap::new(),
            external_call_edges: Vec::new(),
            internal_call_edges: Vec::new(),
            operation_literal_returns: Default::default(),
            operation_void_returns: Default::default(),
            witness_tables: vec![WitnessSubmission {
                type_name: "ReadyFuture".to_string(),
                trait_name: "Future".to_string(),
                table_label: "witness_table_ReadyFuture_Future".to_string(),
                fat_ptr_label: "witness_fat_ReadyFuture_Future".to_string(),
                methods: vec![WitnessMethodSlotSubmission {
                    method_name: "poll".to_string(),
                    impl_symbol: "witness_ReadyFuture_Future_poll".to_string(),
                    method_index: 0,
                }],
                result_literal: String::new(),
            }],
            witness_calls: Vec::new(),
            control_flow: Some(ControlFlowPayload {
                functions: vec![SuspendFunctionArtifact {
                    symbol: QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("async_fn")]),
                    state_machine_type: "AsyncFnStateMachine".to_string(),
                    state_field: "__state".to_string(),
                    frame_fields: vec!["__witness_payload_0".to_string()],
                    dispatch_cases: Vec::new(),
                    states: vec![SuspendStateArtifact {
                        state_id: 0,
                        effect: "Await".to_string(),
                        resume_case_key: 1,
                        frame_carrier: "this".to_string(),
                        spill_fields: vec!["__witness_payload_0".to_string()],
                        suspend_block_label: "await".to_string(),
                        resume_block_label: "resume".to_string(),
                        resume_parameter_count: 0,
                        witness_bindings: vec![SuspendWitnessBinding {
                            trait_name: "Future".to_string(),
                            method_name: "poll".to_string(),
                            method_index: 0,
                            type_name: Some("ReadyFuture".to_string()),
                            impl_symbol: Some("witness_ReadyFuture_Future_poll".to_string()),
                        }],
                        continuation_index: None,
                    }],
                    continuations: Vec::new(),
                }],
            }),
            suspend_runtime: None,
        ..Default::default()
        };
        attach_minimal_mir(&mut submission);
        submission
    }

    #[test]
    fn writes_pe_for_await_witness_module() {
        let submission = await_submission();
        let mut module = clr::lower_fragment_to_msil(&submission).expect("CLR lowering");
        augment_msil_with_suspend(&submission, &mut module);
        assert!(module.global_methods.iter().any(|method| method.method.name == "witness_ReadyFuture_Future_poll"));
        PeWriter::new(PeWriterOptions {
            assembly_name: module.assembly.name.clone(),
            module_name: "demo.dll".to_string(),
            image_kind: ClrImageKind::Executable,
        })
        .write_module(&module)
        .expect("PE write should succeed for await witness module");
    }

    /// spec Task 5.4：当 `Future` impl 同时声明 `is_cancelled` 时，`await` 在 `poll` 返回 true
    /// 后必须先调用 `is_cancelled`；若返回 true（已取消）则跳过会触发 panic 的 `output` 调用，
    /// 以 `Ldnull` 将 `__current` 置空后直接跳到 `skip_output`，否则执行原有 `output` 调用。
    #[test]
    fn await_msil_skips_output_when_cancelled() {
        let mut submission = await_submission();
        submission.witness_tables[0].methods.push(WitnessMethodSlotSubmission {
            method_name: "output".to_string(),
            impl_symbol: "witness_ReadyFuture_Future_output".to_string(),
            method_index: 1,
        });
        submission.witness_tables[0].methods.push(WitnessMethodSlotSubmission {
            method_name: "is_cancelled".to_string(),
            impl_symbol: "witness_ReadyFuture_Future_is_cancelled".to_string(),
            method_index: 2,
        });
        let states = &mut submission.control_flow.as_mut().expect("payload").functions[0].states;
        states[0].witness_bindings.push(SuspendWitnessBinding {
            trait_name: "Future".to_string(),
            method_name: "output".to_string(),
            method_index: 1,
            type_name: Some("ReadyFuture".to_string()),
            impl_symbol: Some("witness_ReadyFuture_Future_output".to_string()),
        });
        states[0].witness_bindings.push(SuspendWitnessBinding {
            trait_name: "Future".to_string(),
            method_name: "is_cancelled".to_string(),
            method_index: 2,
            type_name: Some("ReadyFuture".to_string()),
            impl_symbol: Some("witness_ReadyFuture_Future_is_cancelled".to_string()),
        });

        let mut module = clr::lower_fragment_to_msil(&submission).expect("CLR lowering");
        augment_msil_with_suspend(&submission, &mut module);
        let move_next =
            module.types.iter().flat_map(|ty| ty.methods.iter()).find(|method| method.method.name == "MoveNext").expect("move next");

        let cancel_index = move_next
            .instructions
            .iter()
            .position(|instruction| matches!(
                &instruction.operand,
                Some(MsilInstructionOperand::Method(target)) if target.name == "witness_ReadyFuture_Future_is_cancelled"
            ))
            .expect("expected a Call to Future::is_cancelled");
        let _brfalse_after_cancel = move_next
            .instructions
            .iter()
            .skip(cancel_index)
            .position(|instruction| instruction.opcode == MsilOpcode::Brfalse)
            .expect("expected Brfalse after is_cancelled");
        assert!(move_next.instructions.iter().any(|instruction| instruction.opcode == MsilOpcode::Ldnull));
        assert!(move_next.instructions.iter().any(|instruction| matches!(
            &instruction.operand,
            Some(MsilInstructionOperand::Method(target)) if target.name == "witness_ReadyFuture_Future_output"
        )));
    }
