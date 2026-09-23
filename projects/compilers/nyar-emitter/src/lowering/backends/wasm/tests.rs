    use std::collections::BTreeMap;

    use nyar::{
        CapabilityTag, ControlFlowPayload, ExternalCallArgument, ExternalCallEdge, ExternalImportLink, HostProjectionBoundary, Identifier,
        QualifiedName, RewriteTheory, SuspendFunctionArtifact, SuspendStateArtifact, TheoryBundle,
    };

    use std_data::binary::wasm::{WasmExternalKind, WasmMiscOpcode, WasmOpcode};
    use crate::FragmentSubmission;
    use crate::lowering::wasm::{lower_fragment_to_wasm_module, suspend_run_loop_with_witness_wasm_bytes};

    fn wasm_export_names(module: &crate::nyar_backend_wasi::WasmBinaryModule) -> Vec<String> {
        let export_section = module.sections.iter().find(|section| section.id == 7).expect("export section");
        let bytes = &export_section.bytes;
        let mut offset = 0usize;
        let count = read_uleb128(bytes, &mut offset);
        let mut names = Vec::new();
        for _ in 0..count {
            let name_len = read_uleb128(bytes, &mut offset) as usize;
            let name = std::str::from_utf8(&bytes[offset..offset + name_len]).expect("utf8 export name");
            offset += name_len;
            offset += 1;
            let _index = read_uleb128(bytes, &mut offset);
            names.push(name.to_string());
        }
        names
    }

    fn read_uleb128(bytes: &[u8], offset: &mut usize) -> u32 {
        let mut result = 0u32;
        let mut shift = 0u32;
        loop {
            let byte = bytes[*offset];
            *offset += 1;
            result |= u32::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                return result;
            }
            shift += 7;
        }
    }

    #[test]
    fn lowers_js_glue_stdout_interop_with_emit_byte_import() {
        let main = QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("main")]);
        let console_write_line = QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("console_write_line")]);
        let (module, imports) = lower_fragment_to_wasm_module(
            &FragmentSubmission {
                module_name: "demo".to_string(),
                fragment_id: Identifier::new("main"),
                exported_operations: vec![main.clone()],
                required_capabilities: vec![CapabilityTag::new("host-interop")],
                theory_bundle: TheoryBundle { shared: RewriteTheory::default(), fragment: RewriteTheory::default() },
                entry_operation: Some(main.clone()),
                external_import_links: BTreeMap::from([(
                    console_write_line.clone(),
                    ExternalImportLink::host(Some(Identifier::new("wasm")), vec!["env".to_string(), "emit_byte".to_string()]),
                )]),
                external_call_edges: vec![ExternalCallEdge::new(
                    main,
                    console_write_line,
                    vec![ExternalCallArgument::StringLiteral("hello from node".to_string())],
                )],
                internal_call_edges: Vec::new(),
                operation_literal_returns: Default::default(),
                operation_void_returns: Default::default(),
                witness_tables: Vec::new(),
                witness_calls: Vec::new(),
                control_flow: None,
                suspend_runtime: None,
            ..Default::default()
            },
            HostProjectionBoundary::WasmJsGlue,
        )
        .expect("lower wasm js glue");

        assert_eq!(imports, vec![("env".to_string(), "emit_byte".to_string())]);
        assert!(module.sections.iter().any(|section| section.id == 2));
        assert!(module.sections.iter().any(|section| section.id == 7 && section.bytes.windows(4).any(|window| window == b"main")));
    }

    #[test]
    fn lowers_wasi_component_output_import_without_legacy_default() {
        let main = QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("main")]);
        let console_write_line = QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("console_write_line")]);
        let (module, imports) = lower_fragment_to_wasm_module(
            &FragmentSubmission {
                module_name: "demo".to_string(),
                fragment_id: Identifier::new("main"),
                exported_operations: vec![main.clone()],
                required_capabilities: vec![CapabilityTag::new("host-interop")],
                theory_bundle: TheoryBundle { shared: RewriteTheory::default(), fragment: RewriteTheory::default() },
                entry_operation: Some(main.clone()),
                external_import_links: BTreeMap::from([(
                    console_write_line.clone(),
                    ExternalImportLink::host(
                        Some(Identifier::new("wasi")),
                        vec!["wasi:io/streams".to_string(), "blocking-write-and-flush".to_string()],
                    ),
                )]),
                external_call_edges: vec![ExternalCallEdge::new(
                    main,
                    console_write_line,
                    vec![ExternalCallArgument::StringLiteral("hello from wasi".to_string())],
                )],
                internal_call_edges: Vec::new(),
                operation_literal_returns: Default::default(),
                operation_void_returns: Default::default(),
                witness_tables: Vec::new(),
                witness_calls: Vec::new(),
                control_flow: None,
                suspend_runtime: None,
            ..Default::default()
            },
            HostProjectionBoundary::WasiComponent,
        )
        .expect("lower wasi module");

        // Declared CM imports are forwarded for WIT packaging; core module must not
        // synthesize preview1 fd_write/iovec host calls.
        assert_eq!(imports, vec![("wasi:io/streams".to_string(), "blocking-write-and-flush".to_string())]);
        assert!(!module.sections.iter().any(|section| section.id == 2));
        assert_eq!(
            wasm_export_names(&module),
            vec!["_start", "run", "cabi_post_run", "memory", "cabi_realloc", "_initialize"]
        );
    }

    #[test]
    fn preserves_declared_wasi_component_imports_without_string_output() {
        let monotonic_now = QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("monotonic_now")]);
        let (module, imports) = lower_fragment_to_wasm_module(
            &FragmentSubmission {
                module_name: "demo".to_string(),
                fragment_id: Identifier::new("main"),
                exported_operations: vec![QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("main")])],
                required_capabilities: vec![CapabilityTag::new("host-interop")],
                theory_bundle: TheoryBundle { shared: RewriteTheory::default(), fragment: RewriteTheory::default() },
                entry_operation: None,
                external_import_links: BTreeMap::from([(
                    monotonic_now,
                    ExternalImportLink::host(
                        Some(Identifier::new("wasi")),
                        vec!["wasi:clocks/monotonic-clock".to_string(), "now".to_string()],
                    ),
                )]),
                external_call_edges: Vec::new(),
                internal_call_edges: Vec::new(),
                operation_literal_returns: Default::default(),
                operation_void_returns: Default::default(),
                witness_tables: Vec::new(),
                witness_calls: Vec::new(),
                control_flow: None,
                suspend_runtime: None,
            ..Default::default()
            },
            HostProjectionBoundary::WasiComponent,
        )
        .expect("lower wasi module");

        assert_eq!(imports, vec![("wasi:clocks/monotonic-clock".to_string(), "now".to_string())]);
        assert!(!module.sections.iter().any(|section| section.id == 2));
        assert_eq!(
            wasm_export_names(&module),
            vec!["_start", "run", "cabi_post_run", "memory", "cabi_realloc", "_initialize"]
        );
    }

    fn sample_control_flow_submission() -> FragmentSubmission {
        let symbol = QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("gen")]);
        FragmentSubmission {
            module_name: "demo".to_string(),
            fragment_id: Identifier::new("suspend"),
            exported_operations: Vec::new(),
            required_capabilities: vec![CapabilityTag::new("suspend")],
            theory_bundle: TheoryBundle { shared: RewriteTheory::default(), fragment: RewriteTheory::default() },
            entry_operation: None,
            external_import_links: BTreeMap::new(),
            external_call_edges: Vec::new(),
            internal_call_edges: Vec::new(),
            operation_literal_returns: Default::default(),
            operation_void_returns: Default::default(),
            witness_tables: Vec::new(),
            witness_calls: Vec::new(),
            control_flow: Some(ControlFlowPayload {
                functions: vec![SuspendFunctionArtifact {
                    symbol,
                    state_machine_type: "GenStateMachine".to_string(),
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
        }
    }

    fn assert_control_flow_custom_section(module: &crate::nyar_backend_wasi::WasmBinaryModule) {
        let section = module
            .custom_sections()
            .into_iter()
            .find(|section| section.name == "nyar.control_flow")
            .expect("nyar.control_flow custom section");
        let body = String::from_utf8(section.bytes).expect("utf8");
        assert!(body.contains("demo::gen"), "section body: {body}");
    }

    #[test]
    fn injects_control_flow_custom_section_for_js_glue() {
        let (module, _) = lower_fragment_to_wasm_module(&sample_control_flow_submission(), HostProjectionBoundary::WasmJsGlue).expect("lower");
        assert_control_flow_custom_section(&module);
    }

    #[test]
    fn injects_control_flow_custom_section_for_wasi() {
        let (module, _) =
            lower_fragment_to_wasm_module(&sample_control_flow_submission(), HostProjectionBoundary::WasiComponent).expect("lower");
        assert_control_flow_custom_section(&module);
    }

    #[test]
    fn lowers_wasi_witness_dispatch_module() {
        use nyar::{WitnessCallEdge, WitnessMethodSlotSubmission, WitnessSubmission};

        let (module, imports) = lower_fragment_to_wasm_module(
            &FragmentSubmission {
                module_name: "demo".to_string(),
                fragment_id: Identifier::new("main"),
                exported_operations: Vec::new(),
                required_capabilities: vec![CapabilityTag::new("trait-witness")],
                theory_bundle: TheoryBundle { shared: RewriteTheory::default(), fragment: RewriteTheory::default() },
                entry_operation: None,
                external_import_links: BTreeMap::from([(
                    QualifiedName::new(vec![Identifier::new("demo"), Identifier::new("stdout")]),
                    ExternalImportLink::host(
                        Some(Identifier::new("wasi")),
                        vec!["wasi:io/streams".to_string(), "blocking-write-and-flush".to_string()],
                    ),
                )]),
                external_call_edges: Vec::new(),
                internal_call_edges: Vec::new(),
                operation_literal_returns: Default::default(),
                operation_void_returns: Default::default(),
                witness_tables: vec![WitnessSubmission {
                    type_name: "Dog".to_string(),
                    trait_name: "Animal".to_string(),
                    table_label: "witness_table_Dog_Animal".to_string(),
                    fat_ptr_label: "witness_fat_Dog_Animal".to_string(),
                    methods: vec![WitnessMethodSlotSubmission {
                        method_name: "make_sound".to_string(),
                        impl_symbol: "witness_Dog_Animal_make_sound".to_string(),
                        method_index: 0,
                    }],
                    result_literal: "woof".to_string(),
                }],
                witness_calls: vec![WitnessCallEdge {
                    trait_name: "Animal".to_string(),
                    type_name: "Dog".to_string(),
                    method_index: 0,
                    print_result: true,
                }],
                control_flow: None,
                suspend_runtime: None,
            ..Default::default()
            },
            HostProjectionBoundary::WasiComponent,
        )
        .expect("lower witness wasi");

        assert_eq!(imports, vec![("wasi:io/streams".to_string(), "blocking-write-and-flush".to_string())]);
        assert!(!module.sections.iter().any(|section| section.id == 2));
        assert!(module.sections.iter().any(|section| section.id == 11));
        assert_eq!(
            wasm_export_names(&module),
            vec!["_start", "run", "cabi_post_run", "memory", "cabi_realloc", "_initialize"]
        );
    }

    fn delegate_suspend_submission() -> FragmentSubmission {
        use nyar::{SuspendFunctionArtifact, SuspendStateArtifact, SuspendWitnessBinding, WitnessMethodSlotSubmission, WitnessSubmission};

        FragmentSubmission {
            module_name: "demo".to_string(),
            fragment_id: Identifier::new("suspend"),
            exported_operations: Vec::new(),
            required_capabilities: vec![CapabilityTag::new("suspend"), CapabilityTag::new("trait-witness")],
            theory_bundle: TheoryBundle { shared: RewriteTheory::default(), fragment: RewriteTheory::default() },
            entry_operation: None,
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
    fn suspend_wasm_body_uses_state_dispatch_and_witness_spill_load() {
        use crate::lowering::wasm::suspend_run_loop_with_witness_wasm_bytes;

        let submission = delegate_suspend_submission();
        let artifact = &submission.control_flow.as_ref().expect("payload").functions[0];
        let body = suspend_run_loop_with_witness_wasm_bytes(artifact, 0, 0, 0, 0, false);
        assert!(body.windows(2).any(|window| window == [WasmOpcode::LocalGet.as_u8(), 0]), "expected local.get 0 for __state");
        assert!(body.contains(&WasmOpcode::BrTable.as_u8()), "expected br_table for contiguous dispatch cases");
        let load_spill = body.windows(4).position(|window| window == [WasmOpcode::LocalGet.as_u8(), 1, WasmOpcode::I32Load.as_u8(), 2]).expect("i32.load from spill slot");
        let call_indirect = body.iter().position(|byte| *byte == WasmOpcode::CallIndirect.as_u8()).expect("call_indirect");
        assert!(load_spill < call_indirect, "receiver load must precede call_indirect");
        assert!(body.contains(&WasmOpcode::I32Store.as_u8()), "expected i32.store spill init");
    }

    #[test]
    fn suspend_wasm_await_branches_on_false_poll() {
        use nyar::SuspendWitnessBinding;

        let mut submission = delegate_suspend_submission();
        submission.witness_tables[0] = nyar::WitnessSubmission {
            type_name: "ReadyFuture".to_string(),
            trait_name: "Future".to_string(),
            table_label: "witness_table_ReadyFuture_Future".to_string(),
            fat_ptr_label: "witness_fat_ReadyFuture_Future".to_string(),
            methods: vec![nyar::WitnessMethodSlotSubmission {
                method_name: "poll".to_string(),
                impl_symbol: "witness_ReadyFuture_Future_poll".to_string(),
                method_index: 0,
            }],
            result_literal: String::new(),
        };
        submission.control_flow.as_mut().expect("payload").functions[0].states[0].effect = "Await".to_string();
        submission.control_flow.as_mut().expect("payload").functions[0].states[0].witness_bindings = vec![SuspendWitnessBinding {
            trait_name: "Future".to_string(),
            method_name: "poll".to_string(),
            method_index: 0,
            type_name: Some("ReadyFuture".to_string()),
            impl_symbol: Some("witness_ReadyFuture_Future_poll".to_string()),
        }];

        let artifact = &submission.control_flow.as_ref().expect("payload").functions[0];
        let body = suspend_run_loop_with_witness_wasm_bytes(artifact, 0, 0, 0, 0, false);
        assert!(body.contains(&WasmOpcode::I32Eqz.as_u8()), "await should branch on false poll");
        assert!(body.contains(&WasmOpcode::If.as_u8()), "await should emit if branch");
    }

    /// spec Task 3.2：`await` 在 `Future::poll` 返回 ready 后必须显式调用 `Future::output`
    /// 取出恢复值 `T`。WASM 后端通过 [`secondary_witness_binding`] 获取 `output` 绑定，
    /// 从 witness 表的 `witness_offset + method_index * 4` 偏移加载函数索引并发射
    /// `call_indirect`，再 `local.set 0` 存入恢复值。
    #[test]
    fn suspend_wasm_await_calls_output_after_poll_true() {
        use nyar::SuspendWitnessBinding;

        let mut submission = delegate_suspend_submission();
        submission.witness_tables[0] = nyar::WitnessSubmission {
            type_name: "ReadyFuture".to_string(),
            trait_name: "Future".to_string(),
            table_label: "witness_table_ReadyFuture_Future".to_string(),
            fat_ptr_label: "witness_fat_ReadyFuture_Future".to_string(),
            methods: vec![
                nyar::WitnessMethodSlotSubmission {
                    method_name: "poll".to_string(),
                    impl_symbol: "witness_ReadyFuture_Future_poll".to_string(),
                    method_index: 0,
                },
                nyar::WitnessMethodSlotSubmission {
                    method_name: "output".to_string(),
                    impl_symbol: "witness_ReadyFuture_Future_output".to_string(),
                    method_index: 1,
                },
            ],
            result_literal: String::new(),
        };
        submission.control_flow.as_mut().expect("payload").functions[0].states[0].effect = "Await".to_string();
        submission.control_flow.as_mut().expect("payload").functions[0].states[0].witness_bindings = vec![
            SuspendWitnessBinding {
                trait_name: "Future".to_string(),
                method_name: "poll".to_string(),
                method_index: 0,
                type_name: Some("ReadyFuture".to_string()),
                impl_symbol: Some("witness_ReadyFuture_Future_poll".to_string()),
            },
            SuspendWitnessBinding {
                trait_name: "Future".to_string(),
                method_name: "output".to_string(),
                method_index: 1,
                type_name: Some("ReadyFuture".to_string()),
                impl_symbol: Some("witness_ReadyFuture_Future_output".to_string()),
            },
        ];

        let artifact = &submission.control_flow.as_ref().expect("payload").functions[0];
        let body = suspend_run_loop_with_witness_wasm_bytes(artifact, 0, 0, 0, 0, false);

        let call_indirect_count = body.iter().filter(|byte| **byte == WasmOpcode::CallIndirect.as_u8()).count();
        assert!(call_indirect_count >= 2, "expected at least two call_indirect (poll + output), got {}", call_indirect_count);

        let end_if_index = body
            .iter()
            .position(|byte| *byte == WasmOpcode::End.as_u8())
            .expect("expected end of if branch after poll");
        let output_sequence = [WasmOpcode::I32Const.as_u8(), 4, WasmOpcode::I32Load.as_u8(), 2, 0, WasmOpcode::CallIndirect.as_u8()];
        let output_call_index = body
            .windows(output_sequence.len())
            .skip(end_if_index)
            .position(|window| window == output_sequence)
            .map(|position| position + end_if_index)
            .expect("expected output call_indirect sequence (i32.const 4, i32.load, call_indirect) after if branch");
        assert!(output_call_index > end_if_index, "output call_indirect must follow the poll if branch end");

        let local_set_index = body
            .iter()
            .skip(output_call_index)
            .position(|byte| *byte == WasmOpcode::LocalSet.as_u8())
            .expect("expected local.set after output call_indirect")
            + output_call_index;
        assert!(local_set_index > output_call_index, "local.set must follow the output call_indirect");
    }

    #[test]
    fn lowers_wasi_suspend_witness_module_with_table_and_memory() {
        let (module, _) = lower_fragment_to_wasm_module(&delegate_suspend_submission(), HostProjectionBoundary::WasiComponent).expect("lower");
        assert!(module.sections.iter().any(|section| section.id == 4));
        assert!(module.sections.iter().any(|section| section.id == 5));
        assert!(module.sections.iter().any(|section| section.id == 6), "expected cabi heap global section");
        assert!(module.sections.iter().any(|section| section.id == 9));
        assert!(module.sections.iter().any(|section| section.id == 11));
        assert_eq!(
            wasm_export_names(&module),
            vec!["_start", "run", "cabi_post_run", "memory", "cabi_realloc", "_initialize"]
        );
    }

    #[test]
    fn cabi_realloc_is_bump_allocator_not_null_stub() {
        let (module, _) = lower_fragment_to_wasm_module(
            &FragmentSubmission {
                module_name: "demo".to_string(),
                fragment_id: Identifier::new("main"),
                exported_operations: Vec::new(),
                required_capabilities: Vec::new(),
                theory_bundle: TheoryBundle { shared: RewriteTheory::default(), fragment: RewriteTheory::default() },
                entry_operation: None,
                external_import_links: BTreeMap::new(),
                external_call_edges: Vec::new(),
                internal_call_edges: Vec::new(),
                operation_literal_returns: Default::default(),
                operation_void_returns: Default::default(),
                witness_tables: Vec::new(),
                witness_calls: Vec::new(),
                control_flow: None,
                suspend_runtime: None,
                ..Default::default()
            },
            HostProjectionBoundary::WasiComponent,
        )
        .expect("lower wasi module");

        assert!(module.sections.iter().any(|section| section.id == 6), "expected cabi heap global section");

        let exports = module.sections.iter().find(|section| section.id == 7).expect("export section");
        let realloc_func_index = {
            let bytes = &exports.bytes;
            let mut offset = 0usize;
            let count = read_uleb128(bytes, &mut offset);
            let mut found = None;
            for _ in 0..count {
                let name_len = read_uleb128(bytes, &mut offset) as usize;
                let name = std::str::from_utf8(&bytes[offset..offset + name_len]).expect("utf8");
                offset += name_len;
                let kind = bytes[offset];
                offset += 1;
                let index = read_uleb128(bytes, &mut offset);
                if name == "cabi_realloc" {
                    assert_eq!(kind, WasmExternalKind::Func.as_u8());
                    found = Some(index);
                }
            }
            found.expect("cabi_realloc export")
        };

        let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
        let bodies = {
            let bytes = &code.bytes;
            let mut offset = 0usize;
            let count = read_uleb128(bytes, &mut offset) as usize;
            let mut bodies = Vec::with_capacity(count);
            for _ in 0..count {
                let len = read_uleb128(bytes, &mut offset) as usize;
                bodies.push(bytes[offset..offset + len].to_vec());
                offset += len;
            }
            bodies
        };
        // No function imports on the minimal path, so export index == code body index.
        let realloc_body = &bodies[realloc_func_index as usize];
        assert_ne!(realloc_body.as_slice(), &[WasmOpcode::Unreachable.as_u8(), WasmOpcode::I32Const.as_u8(), 0, WasmOpcode::End.as_u8()], "cabi_realloc must not be the null stub");
        assert!(realloc_body.contains(&WasmOpcode::GlobalGet.as_u8()), "bump allocator must global.get the heap cursor");
        assert!(realloc_body.windows(2).any(|window| window == [WasmOpcode::MemorySize.as_u8(), 0]), "bump allocator must memory.size");
        assert!(realloc_body.windows(2).any(|window| window == [WasmOpcode::PrefixMisc.as_u8(), WasmMiscOpcode::MemoryCopy.as_u8()]), "bump allocator must memory.copy on realloc");
    }

    #[test]
    fn suspend_wasm_async_spawn_emits_awake_call_without_resume() {
        use nyar::SuspendWitnessBinding;

        let mut submission = delegate_suspend_submission();
        submission.witness_tables[0] = nyar::WitnessSubmission {
            type_name: "ReadyFuture".to_string(),
            trait_name: "Future".to_string(),
            table_label: "witness_table_ReadyFuture_Future".to_string(),
            fat_ptr_label: "witness_fat_ReadyFuture_Future".to_string(),
            methods: vec![nyar::WitnessMethodSlotSubmission {
                method_name: "awake".to_string(),
                impl_symbol: "witness_ReadyFuture_Future_awake".to_string(),
                method_index: 0,
            }],
            result_literal: String::new(),
        };
        submission.control_flow.as_mut().expect("payload").functions[0].states[0].effect = "AsyncSpawn".to_string();
        submission.control_flow.as_mut().expect("payload").functions[0].states[0].witness_bindings = vec![SuspendWitnessBinding {
            trait_name: "Future".to_string(),
            method_name: "awake".to_string(),
            method_index: 0,
            type_name: Some("ReadyFuture".to_string()),
            impl_symbol: Some("witness_ReadyFuture_Future_awake".to_string()),
        }];

        let artifact = &submission.control_flow.as_ref().expect("payload").functions[0];
        let body = suspend_run_loop_with_witness_wasm_bytes(artifact, 0, 0, 0, 0, false);
        assert!(body.contains(&WasmOpcode::CallIndirect.as_u8()), "async spawn should emit call_indirect for awake");
        assert!(body.contains(&WasmOpcode::Drop.as_u8()), "async spawn should drop the awake result without branching");
        assert!(!body.contains(&WasmOpcode::I32Eqz.as_u8()), "async spawn must not branch on the ready result");
    }

    #[test]
    fn suspend_wasm_async_block_branches_on_false_poll_like_await() {
        use nyar::SuspendWitnessBinding;

        let mut submission = delegate_suspend_submission();
        submission.witness_tables[0] = nyar::WitnessSubmission {
            type_name: "ReadyFuture".to_string(),
            trait_name: "Future".to_string(),
            table_label: "witness_table_ReadyFuture_Future".to_string(),
            fat_ptr_label: "witness_fat_ReadyFuture_Future".to_string(),
            methods: vec![nyar::WitnessMethodSlotSubmission {
                method_name: "poll".to_string(),
                impl_symbol: "witness_ReadyFuture_Future_poll".to_string(),
                method_index: 0,
            }],
            result_literal: String::new(),
        };
        submission.control_flow.as_mut().expect("payload").functions[0].states[0].effect = "AsyncBlock".to_string();
        submission.control_flow.as_mut().expect("payload").functions[0].states[0].witness_bindings = vec![SuspendWitnessBinding {
            trait_name: "Future".to_string(),
            method_name: "poll".to_string(),
            method_index: 0,
            type_name: Some("ReadyFuture".to_string()),
            impl_symbol: Some("witness_ReadyFuture_Future_poll".to_string()),
        }];

        let artifact = &submission.control_flow.as_ref().expect("payload").functions[0];
        let body = suspend_run_loop_with_witness_wasm_bytes(artifact, 0, 0, 0, 0, false);
        assert!(body.contains(&WasmOpcode::I32Eqz.as_u8()), "async block should branch on false poll like await");
        assert!(body.contains(&WasmOpcode::If.as_u8()), "async block should emit if branch like await");
    }

    #[test]
    fn suspend_wasm_raise_emits_yield_like_effect() {
        let mut raise_submission = delegate_suspend_submission();
        raise_submission.control_flow.as_mut().expect("payload").functions[0].states[0].effect = "Raise".to_string();

        let mut yield_submission = delegate_suspend_submission();
        yield_submission.control_flow.as_mut().expect("payload").functions[0].states[0].effect = "Yield".to_string();

        let raise_artifact = &raise_submission.control_flow.as_ref().expect("payload").functions[0];
        let yield_artifact = &yield_submission.control_flow.as_ref().expect("payload").functions[0];

        let raise_body = suspend_run_loop_with_witness_wasm_bytes(raise_artifact, 0, 0, 0, 0, false);
        let yield_body = suspend_run_loop_with_witness_wasm_bytes(yield_artifact, 0, 0, 0, 0, false);

        assert_eq!(raise_body, yield_body, "raise should emit the same bytecode as yield");
        assert!(
            raise_body.windows(4).any(|window| window == [WasmOpcode::LocalSet.as_u8(), 0, WasmOpcode::Br.as_u8(), 1]),
            "raise should emit yield's local.set 0 followed by br 1"
        );
    }

    /// spec Task 5.4：当 `Future` impl 同时声明 `is_cancelled` 时，`await` 在 `poll` 返回 true
    /// 后必须先调用 `is_cancelled`；若返回 true（已取消）则跳过 `output` 调用，以
    /// `i32.const 0 / local.set 0` 将恢复值置零后进入 `if` 分支，否则在 `else` 分支
    /// 执行原有 `output` 调用。`else` 字节是取消感知路径独有的标志。
    #[test]
    fn suspend_wasm_await_skips_output_when_cancelled() {
        use nyar::SuspendWitnessBinding;

        let mut submission = delegate_suspend_submission();
        submission.witness_tables[0] = nyar::WitnessSubmission {
            type_name: "ReadyFuture".to_string(),
            trait_name: "Future".to_string(),
            table_label: "witness_table_ReadyFuture_Future".to_string(),
            fat_ptr_label: "witness_fat_ReadyFuture_Future".to_string(),
            methods: vec![
                nyar::WitnessMethodSlotSubmission {
                    method_name: "poll".to_string(),
                    impl_symbol: "witness_ReadyFuture_Future_poll".to_string(),
                    method_index: 0,
                },
                nyar::WitnessMethodSlotSubmission {
                    method_name: "output".to_string(),
                    impl_symbol: "witness_ReadyFuture_Future_output".to_string(),
                    method_index: 1,
                },
                nyar::WitnessMethodSlotSubmission {
                    method_name: "is_cancelled".to_string(),
                    impl_symbol: "witness_ReadyFuture_Future_is_cancelled".to_string(),
                    method_index: 2,
                },
            ],
            result_literal: String::new(),
        };
        submission.control_flow.as_mut().expect("payload").functions[0].states[0].effect = "Await".to_string();
        submission.control_flow.as_mut().expect("payload").functions[0].states[0].witness_bindings = vec![
            SuspendWitnessBinding {
                trait_name: "Future".to_string(),
                method_name: "poll".to_string(),
                method_index: 0,
                type_name: Some("ReadyFuture".to_string()),
                impl_symbol: Some("witness_ReadyFuture_Future_poll".to_string()),
            },
            SuspendWitnessBinding {
                trait_name: "Future".to_string(),
                method_name: "output".to_string(),
                method_index: 1,
                type_name: Some("ReadyFuture".to_string()),
                impl_symbol: Some("witness_ReadyFuture_Future_output".to_string()),
            },
            SuspendWitnessBinding {
                trait_name: "Future".to_string(),
                method_name: "is_cancelled".to_string(),
                method_index: 2,
                type_name: Some("ReadyFuture".to_string()),
                impl_symbol: Some("witness_ReadyFuture_Future_is_cancelled".to_string()),
            },
        ];

        let artifact = &submission.control_flow.as_ref().expect("payload").functions[0];
        let body = suspend_run_loop_with_witness_wasm_bytes(artifact, 0, 0, 0, 0, false);

        let call_indirect_count = body.iter().filter(|byte| **byte == WasmOpcode::CallIndirect.as_u8()).count();
        assert!(
            call_indirect_count >= 3,
            "expected at least three call_indirect (poll + is_cancelled + output), got {}",
            call_indirect_count
        );
        assert!(body.contains(&WasmOpcode::Else.as_u8()), "expected else for cancel-aware if/else");
        assert!(body.contains(&WasmOpcode::End.as_u8()), "expected end for cancel-aware if/else");
    }
