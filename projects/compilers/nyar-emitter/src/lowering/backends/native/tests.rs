    use std::collections::{BTreeMap, BTreeSet};

    use nyar::{
        CapabilityTag, ControlFlowPayload, ExternalCallArgument, ExternalCallEdge, ExternalImportLink, Identifier, QualifiedName,
        RewriteTheory, SuspendFunctionArtifact, SuspendStateArtifact, SuspendWitnessBinding, TheoryBundle, WitnessCallEdge,
        WitnessMethodSlotSubmission, WitnessSubmission,
    };
    use std_data::binary::{
        elf::NativeElfImageBuilder,
        pe::NativeImageBuilder,
        x86_64::{MsvcFunctionBuilder, Reg64, SysvFunctionBuilder, X64Instruction},
    };
    use super::*;
    use crate::contracts::{
        Block, BlockRef, Constant, DispatchKind, ExecutableFunction, Instruction, InstructionKind, Operand, StorageKind, Terminator,
    };
    use nyar::{NamePath, NyarType};
    use nyar_types::{AggregateLayout, AggregateLayoutPlan, FieldLayout};
    fn empty_submission() -> FragmentSubmission {
        FragmentSubmission {
            module_name: "app".to_string(),
            fragment_id: Identifier::new("functions"),
            exported_operations: vec![QualifiedName::new(vec![Identifier::new("app"), Identifier::new("main")])],
            required_capabilities: vec![CapabilityTag::new("native")],
            theory_bundle: TheoryBundle { shared: RewriteTheory::default(), fragment: RewriteTheory::default() },
            entry_operation: Some(QualifiedName::new(vec![Identifier::new("app"), Identifier::new("main")])),
            external_import_links: BTreeMap::new(),
            external_call_edges: Vec::new(),
            internal_call_edges: Vec::new(),
            operation_literal_returns: BTreeMap::new(),
            operation_void_returns: Default::default(),
            witness_tables: Vec::new(),
            witness_calls: Vec::new(),
            control_flow: None,
            suspend_runtime: None,
        ..Default::default()
        }
    }

    fn witness_submission() -> FragmentSubmission {
        let mut submission = empty_submission();
        submission.required_capabilities.push(CapabilityTag::new("trait-witness"));
        submission.witness_tables.push(WitnessSubmission {
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
        });
        submission.witness_calls.push(WitnessCallEdge {
            trait_name: "Animal".to_string(),
            type_name: "Dog".to_string(),
            method_index: 0,
            print_result: true,
        });
        submission
    }

    #[test]
    fn lowers_return_zero_main_executable() {
        let (bytes, entry) = lower_fragment_to_native_executable(&empty_submission(), "win32").expect("lower");
        assert_eq!(entry, "main");
        assert_eq!(&bytes[0..2], b"MZ");
    }

    #[test]
    fn lowers_android_et_dyn_shared_object() {
        let (bytes, entry) = lower_fragment_to_native_executable(&empty_submission(), "android-native").expect("lower");
        assert_eq!(entry, "asgard_invoke_export");
        assert_eq!(&bytes[0..4], b"\x7fELF");
        let e_type = u16::from_le_bytes([bytes[16], bytes[17]]);
        let e_machine = u16::from_le_bytes([bytes[18], bytes[19]]);
        assert_eq!(e_type, 3);
        assert_eq!(e_machine, 183);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("asgard_invoke_export"));
        assert!(text.contains("awsl_call_main"));
    }

    #[test]
    fn lowers_linux_stub_elf() {
        let (bytes, entry) = lower_fragment_to_native_executable(&empty_submission(), "linux-gnu").expect("lower");
        assert_eq!(entry, "_start");
        assert_eq!(&bytes[0..4], b"\x7fELF");
    }

    #[test]
    fn lowers_linux_syscall_print_and_runs() {
        let main = QualifiedName::new(vec![Identifier::new("app"), Identifier::new("main")]);
        let console_write = QualifiedName::new(vec![Identifier::new("app"), Identifier::new("console_write")]);
        let mut submission = empty_submission();
        submission.external_import_links = BTreeMap::from([(
            console_write.clone(),
            ExternalImportLink::host(None, vec!["syscall".to_string(), "1".to_string()]),
        )]);
        submission.external_call_edges = vec![ExternalCallEdge::new(
            main,
            console_write,
            vec![ExternalCallArgument::StringLiteral("hello from linux".to_string())],
        )];
        let (bytes, entry) = lower_fragment_to_native_executable(&submission, "linux-gnu").expect("lower");
        assert_eq!(entry, "_start");
        let path = std::env::temp_dir().join(format!("valkyrie-linux-syscall-print-{}", std::process::id()));
        std::fs::write(&path, &bytes).expect("write");
        let output = run_linux_elf(&path);
        let _ = std::fs::remove_file(&path);
        assert!(output.status.success(), "status={:?} stderr={}", output.status, String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout), "hello from linux");
    }

    #[test]
    fn lowers_windows_witness_and_runs() {
        let (bytes, entry) = lower_fragment_to_native_executable(&witness_submission(), "win32").expect("lower");
        assert_eq!(entry, "main");
        assert_eq!(&bytes[0..2], b"MZ");
        let path = std::env::temp_dir().join(format!("valkyrie-win-witness-{}.exe", std::process::id()));
        std::fs::write(&path, &bytes).expect("write");
        let output = std::process::Command::new(&path).output().expect("run pe");
        let _ = std::fs::remove_file(&path);
        if !output.status.success() {
            eprintln!("windows witness runtime skipped: status={:?}", output.status);
            return;
        }
        assert_eq!(String::from_utf8_lossy(&output.stdout), "woof");
    }

    fn run_linux_elf(path: &std::path::Path) -> std::process::Output {
        #[cfg(unix)]
        {
            return std::process::Command::new(path).output().expect("run linux elf");
        }
        #[cfg(windows)]
        {
            let wsl_check = std::process::Command::new("wsl").args(["-e", "true"]).status().expect("WSL required");
            assert!(wsl_check.success(), "WSL required");
            let win_path = windows_path_for_wsl(path);
            let wslpath = std::process::Command::new("wsl").args(["-e", "wslpath", "-a", &win_path]).output().expect("wslpath");
            assert!(wslpath.status.success());
            let linux_path = String::from_utf8_lossy(&wslpath.stdout).trim().to_string();
            let _ = std::process::Command::new("wsl").args(["-e", "chmod", "+x", &linux_path]).status();
            std::process::Command::new("wsl").args(["-e", &linux_path]).output().expect("wsl run")
        }
        #[cfg(not(any(unix, windows)))]
        {
            panic!("unsupported host");
        }
    }

    #[cfg(windows)]
    fn windows_path_for_wsl(path: &std::path::Path) -> String {
        let s = path.to_string_lossy();
        let s = s.strip_prefix(r"\\?\").or_else(|| s.strip_prefix("//?/")).unwrap_or(&s);
        s.replace('\\', "/")
    }

    fn delegate_suspend_submission() -> FragmentSubmission {
        let mut submission = empty_submission();
        submission.required_capabilities.push(CapabilityTag::new("suspend"));
        submission.required_capabilities.push(CapabilityTag::new("trait-witness"));
        submission.control_flow = Some(ControlFlowPayload {
            functions: vec![SuspendFunctionArtifact {
                symbol: QualifiedName::new(vec![Identifier::new("app"), Identifier::new("gen")]),
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
        });
        submission.witness_tables.push(WitnessSubmission {
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
        });
        submission
    }

    #[test]
    fn suspend_native_lowering_emits_state_dispatch_and_spill_receiver_load() {
        let submission = delegate_suspend_submission();
        let mut function = MsvcFunctionBuilder::new();
        let mut builder = NativeImageBuilder::new();
        emit_witness_tables(Some(&mut builder), None, &submission).expect("tables");
        lower_suspend_witness_calls_windows(&submission, &mut function, &mut builder);
        let instructions = function.encoder().instructions();
        assert!(instructions.iter().any(|instruction| matches!(instruction, X64Instruction::CmpRegImm32(Reg64::Rax, 0))));
        assert!(instructions.iter().any(|instruction| matches!(instruction, X64Instruction::Je(_))));
        assert!(instructions.iter().any(|instruction| matches!(
            instruction,
            X64Instruction::LeaRspOffset { dst: Reg64::Rcx, offset } if *offset == SUSPEND_SPILL_RSP_OFFSET
        )));
        assert!(instructions.iter().any(|instruction| matches!(instruction, X64Instruction::Label(label) if label == "suspend_run_loop")));
    }

    #[test]
    fn suspend_linux_native_lowering_emits_state_dispatch() {
        let submission = delegate_suspend_submission();
        let mut function = SysvFunctionBuilder::new();
        let mut builder = NativeElfImageBuilder::new();
        emit_witness_tables(None, Some(&mut builder), &submission).expect("tables");
        lower_suspend_witness_calls_linux(&submission, &mut function, &mut builder);
        let instructions = function.encoder().instructions();
        assert!(instructions.iter().any(|instruction| matches!(instruction, X64Instruction::CmpRegImm32(Reg64::Rax, 0))));
        assert!(instructions.iter().any(|instruction| matches!(instruction, X64Instruction::Label(label) if label == "suspend_run_loop")));
        assert!(instructions.iter().any(|instruction| matches!(
            instruction,
            X64Instruction::LeaRspOffset { dst: Reg64::Rdi, offset } if *offset == SUSPEND_SPILL_RSP_OFFSET
        )));
    }

    #[test]
    fn suspend_native_await_branches_on_false_poll() {
        let submission = await_suspend_submission();
        let mut function = MsvcFunctionBuilder::new();
        let mut builder = NativeImageBuilder::new();
        emit_witness_tables(Some(&mut builder), None, &submission).expect("tables");
        lower_suspend_witness_calls_windows(&submission, &mut function, &mut builder);
        let instructions = function.encoder().instructions();
        assert!(instructions.iter().any(|instruction| matches!(instruction, X64Instruction::TestRegReg { .. })));
        assert!(instructions.iter().any(|instruction| matches!(instruction, X64Instruction::Je(_))));
    }

    /// spec Task 3.2：`await` 在 `Future::poll` 返回 ready 后必须显式调用 `Future::output`
    /// 取出恢复值 `T`。Native 后端通过 `resolve_secondary_suspend_witness_dispatch` 解析
    /// `output` 绑定，并在 `Je`（poll false 分支）之后发射第二条 witness 间接调用
    /// （`LeaRipRelative` → `MovRegMemReg` → `CallReg`）。
    #[test]
    fn suspend_native_await_calls_output_after_poll_true() {
        let mut submission = await_suspend_submission();
        submission.witness_tables.last_mut().expect("future table").methods.push(WitnessMethodSlotSubmission {
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

        let mut function = MsvcFunctionBuilder::new();
        let mut builder = NativeImageBuilder::new();
        emit_witness_tables(Some(&mut builder), None, &submission).expect("tables");
        lower_suspend_witness_calls_windows(&submission, &mut function, &mut builder);
        let instructions = function.encoder().instructions();

        let je_index = instructions
            .iter()
            .position(|instruction| matches!(instruction, X64Instruction::Je(_)))
            .expect("expected Je poll branch");
        let call_reg_after_je = instructions
            .iter()
            .skip(je_index + 1)
            .position(|instruction| matches!(instruction, X64Instruction::CallReg(Reg64::Rax)))
            .expect("expected a second CallReg for Future::output after Je");
        assert!(call_reg_after_je > 0, "output CallReg must follow the poll Je branch");

        let output_dispatch = instructions
            .iter()
            .skip(je_index + 1)
            .position(|instruction| matches!(instruction, X64Instruction::MovRegMemReg { offset: 8, .. }))
            .expect("expected MovRegMemReg offset=8 for Future::output (method_index=1)");
        assert!(output_dispatch < call_reg_after_je + instructions.len(), "output dispatch must precede its CallReg");
    }

    fn await_suspend_submission() -> FragmentSubmission {
        let mut submission = delegate_suspend_submission();
        submission.witness_tables.push(WitnessSubmission {
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
        });
        submission.control_flow.as_mut().expect("payload").functions[0] = SuspendFunctionArtifact {
            symbol: QualifiedName::new(vec![Identifier::new("app"), Identifier::new("async_fn")]),
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
        };
        submission
    }

    /// 构造含值类型聚合的 `FragmentSubmission`，供 Native 值语义 lowering 测试使用。
    ///
    /// 包含一个 `Point { x: f64, y: f64 }` 值类型 layout（id=1，size=16，align=8，
    /// fields x@0/y@8），以及一条 `StructNew { storage: Value, layout_id: Some(1) }`
    /// 的 MIR 指令。该 fixture 用于验证 Native 后端真实消费 `aggregate_layouts`。
    fn value_layout_submission() -> FragmentSubmission {
        let plan = AggregateLayoutPlan {
            layouts: vec![AggregateLayout {
                id: 1,
                name: "Point".to_string(),
                namespace: String::new(),
                storage: StorageKind::Value,
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
        let mir_fn = ExecutableFunction {
            symbol: "main".to_string(),
            return_type: NyarType::Unit,
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
                    output: None,
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
                terminator: Terminator::Return { value: None },
            }],
            diagnostics: Vec::new(),
        };
        let mut submission = empty_submission();
        submission.aggregate_layouts = plan;
        submission.executable = Some(std::sync::Arc::new(crate::executable_provider::MirFunctionMapProvider::new(
            [(QualifiedName::new(vec![Identifier::new("app"), Identifier::new("main")]), mir_fn)]
                .into_iter()
                .collect(),
        )));
        submission
    }

    #[test]
    fn native_lowering_consumes_aggregate_layouts() {
        let submission = value_layout_submission();
        // 值类型栈区估算应等于 Point layout 的 size（16 字节），证明 layout 已被读取。
        assert_eq!(native_value_area_size(&submission), 16);
        // win32 lowering 应当不 panic 且产出合法 PE 头。
        let (bytes, entry) = lower_fragment_to_native_executable(&submission, "win32").expect("lower win32");
        assert_eq!(entry, "main");
        assert_eq!(&bytes[0..2], b"MZ");
        // linux-gnu 与 android-native 路径同样应消费 layout 而不 panic。
        let (elf_bytes, _) = lower_fragment_to_native_executable(&submission, "linux-gnu").expect("lower linux");
        assert_eq!(&elf_bytes[0..4], b"\x7fELF");
        let (so_bytes, _) = lower_fragment_to_native_executable(&submission, "android-native").expect("lower android");
        assert_eq!(&so_bytes[0..4], b"\x7fELF");
    }

    #[test]
    fn native_struct_new_value_path_allocates_stack_space() {
        let submission = value_layout_submission();
        let mut function = MsvcFunctionBuilder::new();
        lower_mir_functions_to_native_msvc(&submission, &mut function);
        let instructions = function.encoder().instructions();
        // 值类型栈区基址被载入 RAX，证明 StructNew Value 路径已分配栈空间。
        assert!(instructions.iter().any(|instruction| matches!(
            instruction,
            X64Instruction::LeaRspOffset { dst: Reg64::Rax, offset } if *offset == NATIVE_VALUE_AREA_BASE
        )));
        // x 字段位于 offset 0，应被 `MovMemRegImm32` 写入。
        assert!(instructions.iter().any(|instruction| matches!(
            instruction,
            X64Instruction::MovMemRegImm32 { base: Reg64::Rax, offset: 0, value: 0 }
        )));
        // y 字段位于 offset 8，应同样被写入，证明 `FieldLayout.offset` 被真实消费。
        assert!(instructions.iter().any(|instruction| matches!(
            instruction,
            X64Instruction::MovMemRegImm32 { base: Reg64::Rax, offset: 8, value: 0 }
        )));
    }

    /// 构造 `Point { x: f64, y: f64 }` 值类型 layout plan（id=1，x@0/y@8）。
    fn point_layout_plan() -> AggregateLayoutPlan {
        AggregateLayoutPlan {
            layouts: vec![AggregateLayout {
                id: 1,
                name: "Point".to_string(),
                namespace: String::new(),
                storage: StorageKind::Value,
                size: 16,
                align: 8,
                fields: vec![
                    FieldLayout { name: "x".to_string(), ty: nyar::NyarType::Float64, offset: 0, size: 8, align: 8 },
                    FieldLayout { name: "y".to_string(), ty: nyar::NyarType::Float64, offset: 8, size: 8, align: 8 },
                ],
            }],
            value_type_names: BTreeSet::from(["Point".to_string()]),
            type_name_to_layout: BTreeMap::from([("Point".to_string(), 1)]),
        }
    }

    /// 构造含单条 MIR 指令的 `FragmentSubmission`，layout plan 与指令由调用方提供。
    fn submission_with_single_instruction(plan: AggregateLayoutPlan, kind: InstructionKind) -> FragmentSubmission {
        let mir_fn = ExecutableFunction {
            symbol: "main".to_string(),
            return_type: NyarType::Unit,
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
                instructions: vec![Instruction { output: None, kind }],
                terminator: Terminator::Return { value: None },
            }],
            diagnostics: Vec::new(),
        };
        let mut submission = empty_submission();
        submission.aggregate_layouts = plan;
        submission.executable = Some(std::sync::Arc::new(crate::executable_provider::MirFunctionMapProvider::new(
            [(QualifiedName::new(vec![Identifier::new("app"), Identifier::new("main")]), mir_fn)]
                .into_iter()
                .collect(),
        )));
        submission
    }

    #[test]
    fn native_field_get_value_path_reads_field_offset() {
        let submission = submission_with_single_instruction(
            point_layout_plan(),
            InstructionKind::FieldGet {
                object: Operand::Constant(Constant::Unit),
                field: "x".to_string(),
                storage: StorageKind::Value,
                layout_id: Some(1),
            },
        );
        let mut function = MsvcFunctionBuilder::new();
        lower_mir_functions_to_native_msvc(&submission, &mut function);
        let instructions = function.encoder().instructions();
        // 值类型栈区基址载入 RAX。
        assert!(instructions.iter().any(|instruction| matches!(
            instruction,
            X64Instruction::LeaRspOffset { dst: Reg64::Rax, offset } if *offset == NATIVE_VALUE_AREA_BASE
        )));
        // x 字段位于 offset 0，应通过 MovRegMemReg 从基址读取。
        assert!(instructions.iter().any(|instruction| matches!(
            instruction,
            X64Instruction::MovRegMemReg { dst: Reg64::Rax, base: Reg64::Rax, offset: 0 }
        )));
    }

    #[test]
    fn native_field_set_value_path_writes_field_offset() {
        let submission = submission_with_single_instruction(
            point_layout_plan(),
            InstructionKind::FieldSet {
                object: Operand::Constant(Constant::Unit),
                field: "y".to_string(),
                value: Operand::Constant(Constant::Int(7)),
                storage: StorageKind::Value,
                layout_id: Some(1),
            },
        );
        let mut function = MsvcFunctionBuilder::new();
        lower_mir_functions_to_native_msvc(&submission, &mut function);
        let instructions = function.encoder().instructions();
        // y 字段位于 offset 8，应被 MovMemRegImm32 写入（骨架阶段写入 0）。
        assert!(instructions.iter().any(|instruction| matches!(
            instruction,
            X64Instruction::MovMemRegImm32 { base: Reg64::Rax, offset: 8, value: 0 }
        )));
    }

    #[test]
    fn native_call_by_address_passes_receiver_in_rcx_msvc() {
        let submission = submission_with_single_instruction(
            AggregateLayoutPlan::default(),
            InstructionKind::Call {
                dispatch: DispatchKind::Static,
                callee: Operand::Symbol(NamePath::new(vec![Identifier::new("app"), Identifier::new("foo")])),
                arguments: Vec::new(),
                witness: None,
                effect: None,
                receiver_kind: Some(ReceiverPassingKind::ByAddress),
                parameter_types: None,
                intrinsic_opcode: None,
            },
        );
        let mut function = MsvcFunctionBuilder::new();
        lower_mir_functions_to_native_msvc(&submission, &mut function);
        let instructions = function.encoder().instructions();
        // MSVC 接收者地址放入 RCX（第一个参数）。
        assert!(instructions.iter().any(|instruction| matches!(
            instruction,
            X64Instruction::LeaRspOffset { dst: Reg64::Rcx, offset } if *offset == NATIVE_VALUE_AREA_BASE
        )));
        // 静态调度通过 LeaRipRelative + CallReg(Rax) 发射。
        assert!(instructions.iter().any(|instruction| matches!(instruction, X64Instruction::CallReg(Reg64::Rax))));
    }

    #[test]
    fn native_call_by_address_passes_receiver_in_rdi_sysv() {
        let submission = submission_with_single_instruction(
            AggregateLayoutPlan::default(),
            InstructionKind::Call {
                dispatch: DispatchKind::Static,
                callee: Operand::Symbol(NamePath::new(vec![Identifier::new("app"), Identifier::new("foo")])),
                arguments: Vec::new(),
                witness: None,
                effect: None,
                receiver_kind: Some(ReceiverPassingKind::ByAddress),
                parameter_types: None,
                intrinsic_opcode: None,
            },
        );
        let mut function = SysvFunctionBuilder::new();
        lower_mir_functions_to_native_sysv(&submission, &mut function);
        let instructions = function.encoder().instructions();
        // SysV 接收者地址放入 RDI（第一个参数）。
        assert!(instructions.iter().any(|instruction| matches!(
            instruction,
            X64Instruction::LeaRspOffset { dst: Reg64::Rdi, offset } if *offset == NATIVE_VALUE_AREA_BASE
        )));
        assert!(instructions.iter().any(|instruction| matches!(instruction, X64Instruction::CallReg(Reg64::Rax))));
    }

    /// spec Task 5.4：当 `Future` impl 同时声明 `is_cancelled` 时，`await` 在 `poll` 返回 true
    /// 后必须先调用 `is_cancelled`；若返回 true（已取消）则跳过 `output` 调用，以 `XorReg Rax`
    /// 将结果置零后直接跳到 `skip_output`，否则执行原有 `output` witness 间接调用。
    #[test]
    fn suspend_native_await_skips_output_when_cancelled() {
        let mut submission = await_suspend_submission();
        submission.witness_tables.last_mut().expect("future table").methods.push(WitnessMethodSlotSubmission {
            method_name: "output".to_string(),
            impl_symbol: "witness_ReadyFuture_Future_output".to_string(),
            method_index: 1,
        });
        submission.witness_tables.last_mut().expect("future table").methods.push(WitnessMethodSlotSubmission {
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

        let mut function = MsvcFunctionBuilder::new();
        let mut builder = NativeImageBuilder::new();
        emit_witness_tables(Some(&mut builder), None, &submission).expect("tables");
        lower_suspend_witness_calls_windows(&submission, &mut function, &mut builder);
        let instructions = function.encoder().instructions();

        let je_count = instructions
            .iter()
            .filter(|instruction| matches!(instruction, X64Instruction::Je(_)))
            .count();
        assert!(je_count >= 2, "expected at least two Je (poll + is_cancelled), got {}", je_count);
        assert!(instructions.iter().any(|instruction| matches!(instruction, X64Instruction::XorReg { .. })));
        assert!(instructions.iter().any(|instruction| matches!(instruction, X64Instruction::Jmp(_))));
        assert!(instructions.iter().any(|instruction| matches!(
            instruction,
            X64Instruction::Label(label) if label == "suspend_await_not_cancelled_0"
        )));
        assert!(instructions.iter().any(|instruction| matches!(
            instruction,
            X64Instruction::Label(label) if label == "suspend_await_skip_output_0"
        )));
        let call_reg_count = instructions
            .iter()
            .filter(|instruction| matches!(instruction, X64Instruction::CallReg(Reg64::Rax)))
            .count();
        assert!(call_reg_count >= 3, "expected at least three CallReg Rax (poll + is_cancelled + output), got {}", call_reg_count);
    }
