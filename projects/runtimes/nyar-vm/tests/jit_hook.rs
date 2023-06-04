use nyar_vm::{jit::JitError, NyarVm};

use nyar_bytecode::{NyarConstant, NyarExport, NyarExportKind, NyarFunction, NyarHeadCode, NyarModuleData, NYAR_VERSION, encode_module};

fn empty_module() -> nyar_vm::module::LoadedModule {
    let mut code = Vec::new();
    code.push(NyarHeadCode::Const as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Return as u8);
    let data = NyarModuleData {
        version: NYAR_VERSION,
        name: "jit-hook".to_string(),
        constants: vec![NyarConstant::Integer32(0)],
        functions: vec![NyarFunction {
            name: "main".to_string(),
            arity: 0,
            local_count: 0,
            code_offset: 0,
            code_length: code.len() as i32,
        }],
        imports: Vec::new(),
        exports: vec![NyarExport { kind: NyarExportKind::Function, symbol_name: "main".to_string(), function_index: 0 }],
        witness_entries: Vec::new(),
        code_bytes: code,
        globals: Vec::new(),
        init_function_indices: Vec::new(),
        layouts: Vec::new(),
    };
    let bytes = encode_module(&data);
    NyarVm::new().load(&bytes).expect("load module")
}

#[test]
fn executor_default_jit_is_disabled() {
    let module = empty_module();
    let mut vm = NyarVm::new();
    assert!(!vm.jit_enabled());
    assert_eq!(vm.try_jit_compile(&module, 0), Err(JitError::Unsupported));
}

#[test]
fn stack_maps_available_without_jit_backend() {
    use nyar_bytecode::NyarHeadCode;

    let mut code = Vec::new();
    code.push(NyarHeadCode::ObjectNew as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Return as u8);
    let data = NyarModuleData {
        version: NYAR_VERSION,
        name: "stack-map".to_string(),
        constants: Vec::new(),
        functions: vec![NyarFunction {
            name: "main".to_string(),
            arity: 0,
            local_count: 2,
            code_offset: 0,
            code_length: code.len() as i32,
        }],
        imports: Vec::new(),
        exports: vec![NyarExport { kind: NyarExportKind::Function, symbol_name: "main".to_string(), function_index: 0 }],
        witness_entries: Vec::new(),
        code_bytes: code,
        globals: Vec::new(),
        init_function_indices: Vec::new(),
        layouts: vec![nyar_bytecode::NyarLayout { field_count: 0 }],
    };
    let module = NyarVm::new().load(&encode_module(&data)).expect("load");
    let maps = NyarVm::new().stack_maps_for(&module, 0).expect("stack maps");
    assert_eq!(maps.function_index, 0);
    assert!(!maps.entries.is_empty());
    assert_eq!(maps.entries[0].local_root_slots, vec![0, 1]);
    // ObjectNew 与 Return 均为 safepoint
    assert!(maps.entries.len() >= 2);
}

#[test]
fn stack_map_jit_backend_returns_artifact_maps() {
    use nyar_vm::jit::StackMapJit;

    let module = empty_module();
    let mut vm = NyarVm::new();
    vm.set_jit(Box::new(StackMapJit));
    assert!(vm.jit_enabled());
    let artifact = vm.try_jit_compile(&module, 0).expect("stack-map jit");
    assert_eq!(artifact.function_index, 0);
    assert!(!artifact.stack_maps.entries.is_empty());
    assert!(!artifact.deopt_map.entries.is_empty());
}

#[test]
fn baseline_scalar_jit_fast_path_adds_i32_args() {
    use nyar_vm::jit::BaselineScalarJit;
    use nyar_vm::Value;

    let mut code = Vec::new();
    code.push(NyarHeadCode::LoadArg as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::LoadArg as u8);
    code.extend_from_slice(&1i32.to_le_bytes());
    code.push(NyarHeadCode::I32Add as u8);
    code.push(NyarHeadCode::Return as u8);
    let data = NyarModuleData {
        version: NYAR_VERSION,
        name: "nj1-add".to_string(),
        constants: Vec::new(),
        functions: vec![NyarFunction {
            name: "add".to_string(),
            arity: 2,
            local_count: 2,
            code_offset: 0,
            code_length: code.len() as i32,
        }],
        imports: Vec::new(),
        exports: vec![NyarExport { kind: NyarExportKind::Function, symbol_name: "add".to_string(), function_index: 0 }],
        witness_entries: Vec::new(),
        code_bytes: code,
        globals: Vec::new(),
        init_function_indices: Vec::new(),
        layouts: Vec::new(),
    };
    let module = NyarVm::new().load(&encode_module(&data)).expect("load");
    let mut vm = NyarVm::new();
    vm.set_jit(Box::new(BaselineScalarJit));
    let artifact = vm.try_jit_compile(&module, 0).expect("compile");
    assert!(artifact.machine_code.is_some());
    let result = vm.run(&module, "add", vec![Value::I32(40), Value::I32(2)]).expect("run");
    assert_eq!(result, Value::I32(42));
    assert_eq!(vm.nj1_cache_len(), 1);
    // 第二次 run 应命中缓存
    let again = vm.run(&module, "add", vec![Value::I32(1), Value::I32(2)]).expect("run cached");
    assert_eq!(again, Value::I32(3));
    assert_eq!(vm.nj1_cache_len(), 1);
}

fn load_i32_binop_module(name: &str, export: &str, op: NyarHeadCode) -> nyar_vm::module::LoadedModule {
    let mut code = Vec::new();
    code.push(NyarHeadCode::LoadArg as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::LoadArg as u8);
    code.extend_from_slice(&1i32.to_le_bytes());
    code.push(op as u8);
    code.push(NyarHeadCode::Return as u8);
    let data = NyarModuleData {
        version: NYAR_VERSION,
        name: name.to_string(),
        constants: Vec::new(),
        functions: vec![NyarFunction {
            name: export.to_string(),
            arity: 2,
            local_count: 2,
            code_offset: 0,
            code_length: code.len() as i32,
        }],
        imports: Vec::new(),
        exports: vec![NyarExport {
            kind: NyarExportKind::Function,
            symbol_name: export.to_string(),
            function_index: 0,
        }],
        witness_entries: Vec::new(),
        code_bytes: code,
        globals: Vec::new(),
        init_function_indices: Vec::new(),
        layouts: Vec::new(),
    };
    NyarVm::new().load(&encode_module(&data)).expect("load")
}

#[test]
fn baseline_scalar_jit_fast_path_cmp_and_select() {
    use nyar_vm::jit::BaselineScalarJit;
    use nyar_vm::Value;

    let mut vm = NyarVm::new();
    vm.set_jit(Box::new(BaselineScalarJit));

    // LoadArg0; LoadArg1; I32LtS; Return
    let mut cmp_code = Vec::new();
    cmp_code.push(NyarHeadCode::LoadArg as u8);
    cmp_code.extend_from_slice(&0i32.to_le_bytes());
    cmp_code.push(NyarHeadCode::LoadArg as u8);
    cmp_code.extend_from_slice(&1i32.to_le_bytes());
    cmp_code.push(NyarHeadCode::I32LtS as u8);
    cmp_code.push(NyarHeadCode::Return as u8);
    let cmp = load_named_module("nj1-lt", "lt", 2, cmp_code);
    assert_eq!(
        vm.run(&cmp, "lt", vec![Value::I32(1), Value::I32(2)]).expect("lt"),
        Value::I32(1)
    );

    // select: a==b ? c : d
    let mut sel = Vec::new();
    sel.push(NyarHeadCode::LoadArg as u8);
    sel.extend_from_slice(&0i32.to_le_bytes());
    sel.push(NyarHeadCode::LoadArg as u8);
    sel.extend_from_slice(&1i32.to_le_bytes());
    sel.push(NyarHeadCode::I32Eq as u8);
    let br_pc = sel.len();
    sel.push(NyarHeadCode::JumpIfFalse as u8);
    let offset_pos = sel.len();
    sel.extend_from_slice(&0i32.to_le_bytes());
    sel.push(NyarHeadCode::LoadArg as u8);
    sel.extend_from_slice(&2i32.to_le_bytes());
    sel.push(NyarHeadCode::Return as u8);
    let else_pc = sel.len();
    let rel = (else_pc as i32) - (br_pc as i32);
    sel[offset_pos..offset_pos + 4].copy_from_slice(&rel.to_le_bytes());
    sel.push(NyarHeadCode::LoadArg as u8);
    sel.extend_from_slice(&3i32.to_le_bytes());
    sel.push(NyarHeadCode::Return as u8);
    let select = load_named_module("nj1-sel", "sel", 4, sel);
    assert_eq!(
        vm.run(
            &select,
            "sel",
            vec![Value::I32(1), Value::I32(1), Value::I32(7), Value::I32(9)]
        )
        .expect("sel true"),
        Value::I32(7)
    );
    assert_eq!(
        vm.run(
            &select,
            "sel",
            vec![Value::I32(1), Value::I32(0), Value::I32(7), Value::I32(9)]
        )
        .expect("sel false"),
        Value::I32(9)
    );
}

fn load_named_module(name: &str, export: &str, arity: i32, code: Vec<u8>) -> nyar_vm::module::LoadedModule {
    let data = NyarModuleData {
        version: NYAR_VERSION,
        name: name.to_string(),
        constants: Vec::new(),
        functions: vec![NyarFunction {
            name: export.to_string(),
            arity,
            local_count: arity,
            code_offset: 0,
            code_length: code.len() as i32,
        }],
        imports: Vec::new(),
        exports: vec![NyarExport {
            kind: NyarExportKind::Function,
            symbol_name: export.to_string(),
            function_index: 0,
        }],
        witness_entries: Vec::new(),
        code_bytes: code,
        globals: Vec::new(),
        init_function_indices: Vec::new(),
        layouts: Vec::new(),
    };
    NyarVm::new().load(&encode_module(&data)).expect("load")
}

#[test]
fn baseline_scalar_jit_fast_path_load_dup_mul() {
    use nyar_vm::jit::BaselineScalarJit;
    use nyar_vm::Value;

    let mut code = Vec::new();
    code.push(NyarHeadCode::LoadArg as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Dup as u8);
    code.push(NyarHeadCode::I32Mul as u8);
    code.push(NyarHeadCode::Return as u8);
    let module = load_named_module("nj1-sq", "sq", 1, code);
    let mut vm = NyarVm::new();
    vm.set_jit(Box::new(BaselineScalarJit));
    assert_eq!(vm.run(&module, "sq", vec![Value::I32(7)]).expect("run"), Value::I32(49));
}

#[test]
fn baseline_scalar_jit_folds_const_const_binop() {
    use nyar_vm::jit::BaselineScalarJit;
    use nyar_vm::Value;

    let mut code = Vec::new();
    code.push(NyarHeadCode::Const as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Const as u8);
    code.extend_from_slice(&1i32.to_le_bytes());
    code.push(NyarHeadCode::I32Add as u8);
    code.push(NyarHeadCode::Return as u8);
    let data = NyarModuleData {
        version: NYAR_VERSION,
        name: "nj1-fold".to_string(),
        constants: vec![NyarConstant::Integer32(20), NyarConstant::Integer32(22)],
        functions: vec![NyarFunction {
            name: "forty_two".to_string(),
            arity: 0,
            local_count: 0,
            code_offset: 0,
            code_length: code.len() as i32,
        }],
        imports: Vec::new(),
        exports: vec![NyarExport {
            kind: NyarExportKind::Function,
            symbol_name: "forty_two".to_string(),
            function_index: 0,
        }],
        witness_entries: Vec::new(),
        code_bytes: code,
        globals: Vec::new(),
        init_function_indices: Vec::new(),
        layouts: Vec::new(),
    };
    let module = NyarVm::new().load(&encode_module(&data)).expect("load");
    let mut vm = NyarVm::new();
    vm.set_jit(Box::new(BaselineScalarJit));
    assert_eq!(vm.run(&module, "forty_two", vec![]).expect("run"), Value::I32(42));
}

#[test]
fn workload_hints_tighten_gray_budget_on_vm() {
    use nyar_gc::WorkloadHints;

    let mut vm = NyarVm::new();
    assert_eq!(vm.gray_budget_per_slice(), 64);
    vm.apply_workload_hints(WorkloadHints {
        pause_budget_ms: Some(1),
        ..WorkloadHints::default()
    });
    assert_eq!(vm.gray_budget_per_slice(), 8);
}

#[test]
fn concurrent_interactive_fixture_wires_vm_evidence_accessors() {
    use nyar_gc::GcMode;
    use nyar_vm::workload_json::parse_workload_intent_json;

    let text = include_str!("../fixtures/workload/concurrent-interactive.json");
    let intent = parse_workload_intent_json(text).expect("parse");
    let mut vm = NyarVm::new();
    let decision = vm.apply_workload_intent(intent).expect("apply");
    assert_eq!(decision.mode, GcMode::ConcurrentMarkReserved);
    assert_eq!(decision.scenario_id.as_deref(), Some("concurrent-interactive"));
    assert!(vm.strategy_transition_history().len() >= 1);
    let _ = vm.last_trace_poll();
    let _ = vm.last_root_handshake();
    let _ = vm.last_relocate_map();
    assert!(vm.last_promotion_failure().is_none());
}

#[test]
fn install_inline_deopt_chain_orders_outer_then_inner() {
    use nyar_vm::jit::{InlineFrameSpec, build_inline_deopt_map, materialize_interpreter_frames};

    let map = build_inline_deopt_map(
        1,
        &[
            InlineFrameSpec {
                function_index: 1,
                local_count: 1,
                resume_instruction: None,
            },
            InlineFrameSpec {
                function_index: 0,
                local_count: 2,
                resume_instruction: Some(40),
            },
        ],
        &[7],
    );
    let entry = map.entry_at(7).expect("entry");
    let restored = materialize_interpreter_frames(entry, &[vec![], vec![]]).expect("materialize");
    let mut vm = NyarVm::new();
    vm.install_deopt_frames(&restored, 0, None).expect("install");
    assert_eq!(vm.frame_count(), 2);
}

#[test]
fn install_deopt_frames_invalidates_nj1_and_restores_locals() {
    use nyar_vm::jit::{BaselineScalarJit, build_baseline_deopt_map, materialize_interpreter_frames};
    use nyar_vm::Value;

    let mut code = Vec::new();
    code.push(NyarHeadCode::LoadArg as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Return as u8);
    let module = load_named_module("nj1-deopt", "id", 1, code);
    let mut vm = NyarVm::new();
    vm.set_jit(Box::new(BaselineScalarJit));
    assert_eq!(vm.run(&module, "id", vec![Value::I32(1)]).expect("run"), Value::I32(1));
    assert_eq!(vm.nj1_cache_len(), 1);

    let map = build_baseline_deopt_map(0, 2, &[0]);
    let entry = map.entry_at(0).expect("entry");
    let restored = materialize_interpreter_frames(entry, &[vec![]]).expect("materialize");
    vm.install_deopt_frames(&restored, 0, None).expect("install");
    assert_eq!(vm.nj1_cache_len(), 0);
    assert_eq!(vm.frame_count(), 1);
}

#[test]
fn invalidate_assumption_drops_scalar_leaf_cache() {
    use nyar_vm::jit::{BaselineScalarJit, JitAssumption};
    use nyar_vm::Value;

    let mut code = Vec::new();
    code.push(NyarHeadCode::LoadArg as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Return as u8);
    let module = load_named_module("nj1-assume", "id", 1, code);
    let mut vm = NyarVm::new();
    vm.set_jit(Box::new(BaselineScalarJit));
    assert_eq!(vm.run(&module, "id", vec![Value::I32(9)]).expect("run"), Value::I32(9));
    assert_eq!(vm.nj1_cache_len(), 1);
    vm.invalidate_assumption(JitAssumption::ScalarLeafShape);
    assert_eq!(vm.nj1_cache_len(), 0);
}

#[test]
fn invalidate_nj1_cache_clears_compiled_blobs() {
    use nyar_vm::jit::BaselineScalarJit;
    use nyar_vm::Value;

    let mut code = Vec::new();
    code.push(NyarHeadCode::LoadArg as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::Return as u8);
    let module = load_named_module("nj1-inv", "id", 1, code);
    let mut vm = NyarVm::new();
    vm.set_jit(Box::new(BaselineScalarJit));
    assert_eq!(vm.run(&module, "id", vec![Value::I32(3)]).expect("run"), Value::I32(3));
    assert_eq!(vm.nj1_cache_len(), 1);
    vm.invalidate_nj1_module(&module);
    assert_eq!(vm.nj1_cache_len(), 0);
    assert_eq!(vm.run(&module, "id", vec![Value::I32(4)]).expect("rerun"), Value::I32(4));
    assert_eq!(vm.nj1_cache_len(), 1);
    vm.invalidate_nj1_cache();
    assert_eq!(vm.nj1_cache_len(), 0);
}

#[test]
fn baseline_scalar_jit_fast_path_const_local_binop() {
    use nyar_vm::jit::BaselineScalarJit;
    use nyar_vm::Value;

    let mut code = Vec::new();
    code.push(NyarHeadCode::Const as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::LoadArg as u8);
    code.extend_from_slice(&0i32.to_le_bytes());
    code.push(NyarHeadCode::I32Add as u8);
    code.push(NyarHeadCode::Return as u8);
    let data = NyarModuleData {
        version: NYAR_VERSION,
        name: "nj1-imm-add".to_string(),
        constants: vec![NyarConstant::Integer32(10)],
        functions: vec![NyarFunction {
            name: "add10".to_string(),
            arity: 1,
            local_count: 1,
            code_offset: 0,
            code_length: code.len() as i32,
        }],
        imports: Vec::new(),
        exports: vec![NyarExport {
            kind: NyarExportKind::Function,
            symbol_name: "add10".to_string(),
            function_index: 0,
        }],
        witness_entries: Vec::new(),
        code_bytes: code,
        globals: Vec::new(),
        init_function_indices: Vec::new(),
        layouts: Vec::new(),
    };
    let module = NyarVm::new().load(&encode_module(&data)).expect("load");
    let mut vm = NyarVm::new();
    vm.set_jit(Box::new(BaselineScalarJit));
    assert_eq!(vm.run(&module, "add10", vec![Value::I32(7)]).expect("run"), Value::I32(17));
    assert_eq!(vm.nj1_cache_len(), 1);
}

#[test]
fn baseline_scalar_jit_fast_path_sub_and_mul() {
    use nyar_vm::jit::BaselineScalarJit;
    use nyar_vm::Value;

    let mut vm = NyarVm::new();
    vm.set_jit(Box::new(BaselineScalarJit));

    let sub = load_i32_binop_module("nj1-sub", "sub", NyarHeadCode::I32Sub);
    assert_eq!(
        vm.run(&sub, "sub", vec![Value::I32(40), Value::I32(2)]).expect("sub"),
        Value::I32(38)
    );

    let mul = load_i32_binop_module("nj1-mul", "mul", NyarHeadCode::I32Mul);
    assert_eq!(
        vm.run(&mul, "mul", vec![Value::I32(40), Value::I32(2)]).expect("mul"),
        Value::I32(80)
    );
    assert_eq!(vm.nj1_cache_len(), 2);
}
