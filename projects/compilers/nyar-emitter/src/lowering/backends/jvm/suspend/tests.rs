    use std::collections::BTreeMap;

    use nyar::{
        CapabilityTag, ControlFlowPayload, Identifier, QualifiedName, RewriteTheory, SuspendFunctionArtifact, SuspendStateArtifact,
        SuspendWitnessBinding, TheoryBundle, WitnessMethodSlotSubmission, WitnessSubmission,
    };
    use crate::nyar_backend_jvm::JvmInstruction;

    use super::*;
    use crate::{FragmentSubmission, lowering::jvm};

    fn delegate_submission() -> FragmentSubmission {
        FragmentSubmission {
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
        }
    }

    #[test]
    fn dispatch_cases_nonempty_path() {
        let submission = delegate_submission();
        let artifact = &submission.control_flow.as_ref().expect("payload").functions[0];
        assert_eq!(dispatch_case_keys(artifact), vec![0, 1]);
    }

    #[test]
    fn witness_receiver_load_emits_aaload_from_frame_slot_zero() {
        let submission = delegate_submission();
        let class_file = jvm::lower_fragment_to_jvm_class(&submission).expect("lower");
        let move_next = class_file
            .methods
            .iter()
            .find(|method| method.name == "sm_demo__gen_move_next")
            .and_then(|method| method.code.as_ref())
            .expect("move_next");
        assert!(move_next.instructions.iter().any(|instruction| matches!(instruction, JvmInstruction::AALoad)));
    }

    #[test]
    fn delegate_yield_emits_witness_call() {
        let submission = delegate_submission();
        let class_file = jvm::lower_fragment_to_jvm_class(&submission).expect("lower");
        assert!(class_file.methods.iter().any(|method| method.name == "witness_CounterIterator_Iterator_next"));
        let move_next = class_file
            .methods
            .iter()
            .find(|method| method.name == "sm_demo__gen_move_next")
            .and_then(|method| method.code.as_ref())
            .expect("move_next");
        assert!(move_next.instructions.iter().any(|instruction| matches!(
            instruction,
            JvmInstruction::InvokeStatic(method) if method.name == "witness_CounterIterator_Iterator_next"
        )));
    }

    #[test]
    fn await_future_jvm_poll_branches() {
        let submission = await_submission();
        let class_file = jvm::lower_fragment_to_jvm_class(&submission).expect("lower");
        let move_next = class_file
            .methods
            .iter()
            .find(|method| method.name == "sm_demo__async_fn_move_next")
            .and_then(|method| method.code.as_ref())
            .expect("move_next");
        assert!(move_next.instructions.iter().any(|instruction| matches!(instruction, JvmInstruction::IfEq(_))));
    }

    /// spec Task 3.2：`await` 在 `Future::poll` 返回 ready 后必须显式调用 `Future::output`
    /// 取出恢复值 `T`，并将其写入 frame 数组的 slot 1 供 resume 路径读取。
    #[test]
    fn await_jvm_calls_output_after_poll_true() {
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

        let class_file = jvm::lower_fragment_to_jvm_class(&submission).expect("lower");
        let move_next = class_file
            .methods
            .iter()
            .find(|method| method.name == "sm_demo__async_fn_move_next")
            .and_then(|method| method.code.as_ref())
            .expect("move_next");

        let ifeq_index = move_next
            .instructions
            .iter()
            .position(|instruction| matches!(instruction, JvmInstruction::IfEq(_)))
            .expect("expected IfEq poll branch");
        let output_index = move_next
            .instructions
            .iter()
            .skip(ifeq_index)
            .position(|instruction| matches!(
                instruction,
                JvmInstruction::InvokeStatic(method) if method.name == "witness_ReadyFuture_Future_output"
            ))
            .expect("expected InvokeStatic Future::output after IfEq")
            + ifeq_index;
        assert!(output_index > ifeq_index, "output call must follow the poll IfEq branch");
        let aastore_index = move_next
            .instructions
            .iter()
            .skip(output_index)
            .position(|instruction| matches!(instruction, JvmInstruction::AAStore))
            .expect("expected AAStore frame[1] = T after output call")
            + output_index;
        assert!(aastore_index > output_index, "AAStore must follow the output call");
    }

    #[test]
    fn encodes_class_bytes_for_yield_state_machine_module() {
        let symbol = QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("main")]);
        let submission = FragmentSubmission {
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
        let class_file = jvm::lower_fragment_to_jvm_class(&submission).expect("lower");
        assert!(!class_file.to_bytes().expect("encode").is_empty());
    }

    /// spec Task 5.4：当 `Future` impl 同时声明 `is_cancelled` 时，`await` 在 `poll` 返回 true
    /// 后必须先调用 `is_cancelled`；若返回 true（已取消）则跳过 `output` 调用，以 `AConstNull`
    /// 将 frame[1] 置空后直接跳到 `skip_output`，否则执行原有 `output` 调用。
    #[test]
    fn await_jvm_skips_output_when_cancelled() {
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

        let class_file = jvm::lower_fragment_to_jvm_class(&submission).expect("lower");
        let move_next = class_file
            .methods
            .iter()
            .find(|method| method.name == "sm_demo__async_fn_move_next")
            .and_then(|method| method.code.as_ref())
            .expect("move_next");

        let cancel_index = move_next
            .instructions
            .iter()
            .position(|instruction| matches!(
                instruction,
                JvmInstruction::InvokeStatic(method) if method.name == "witness_ReadyFuture_Future_is_cancelled"
            ))
            .expect("expected InvokeStatic Future::is_cancelled");
        let _ifeq_after_cancel = move_next
            .instructions
            .iter()
            .skip(cancel_index)
            .position(|instruction| matches!(instruction, JvmInstruction::IfEq(_)))
            .expect("expected IfEq after is_cancelled");
        assert!(move_next.instructions.iter().any(|instruction| matches!(instruction, JvmInstruction::AConstNull)));
        assert!(move_next.instructions.iter().any(|instruction| matches!(
            instruction,
            JvmInstruction::InvokeStatic(method) if method.name == "witness_ReadyFuture_Future_output"
        )));
    }

    fn await_submission() -> FragmentSubmission {
        FragmentSubmission {
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
        }
    }
