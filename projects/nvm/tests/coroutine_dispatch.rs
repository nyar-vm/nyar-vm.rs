//! Yield / Resume / PerformEffect dispatch round-trip tests for the nyar-vm interpreter.
//!
//! These tests construct bytecode directly to verify that the VM-side opcode handlers
//! for `Yield`, `Resume`, and `PerformEffect` form a coherent suspend/resume pipeline,
//! independent of any emitter-side lowering path.

use nvm::{NyarVm, Value};
use std_data::binary::nyar_ir::{NyarConstant, NyarExport, NyarExportKind, NyarFunction, NyarHeadCode, NyarModuleData, encode_module};

/// Encodes a 5-byte `Imm1` instruction (opcode + i32 operand).
fn emit_imm1(code: &mut Vec<u8>, opcode: NyarHeadCode, operand: i32) {
    code.push(opcode as u8);
    code.extend_from_slice(&operand.to_le_bytes());
}

/// Encodes a 1-byte `Plain` instruction.
fn emit_plain(code: &mut Vec<u8>, opcode: NyarHeadCode) {
    code.push(opcode as u8);
}

/// Builds a module with two functions: a generator `gen` that yields once then returns the
/// resumed value, and `main` that calls `gen`, stores the resulting coroutine, then resumes
/// it with a sentinel value. `main` returns the value the coroutine returns on resume.
fn build_yield_resume_module() -> NyarModuleData {
    // Function 0: gen
    //   Const 0 (42)        ; push yielded value
    //   Yield 0             ; suspend, caller observes coroutine(yielded=42)
    //   Return              ; on resume: return resume_value (still on stack)
    let mut code = Vec::new();
    let gen_offset = 0i32;
    emit_imm1(&mut code, NyarHeadCode::Const, 0); // const index 0 -> 42
    emit_imm1(&mut code, NyarHeadCode::Yield, 0); // operand1 unused by VM, but encoded as Imm1
    emit_plain(&mut code, NyarHeadCode::Return);

    // Function 1: main
    //   Call 0              ; call gen -> suspend -> coroutine pushed
    //   StoreLocal 0        ; local 0 = coroutine
    //   LoadLocal 0         ; push coroutine
    //   Const 1 (999)       ; push resume value
    //   Resume              ; pop resume_value(999), pop coroutine, ResumeCoroutine
    //   Return              ; main returns whatever gen returned (999)
    let main_offset = code.len() as i32;
    emit_imm1(&mut code, NyarHeadCode::Call, 0);
    emit_imm1(&mut code, NyarHeadCode::StoreLocal, 0);
    emit_imm1(&mut code, NyarHeadCode::LoadLocal, 0);
    emit_imm1(&mut code, NyarHeadCode::Const, 1); // const index 1 -> 999
    emit_plain(&mut code, NyarHeadCode::Resume);
    emit_plain(&mut code, NyarHeadCode::Return);

    NyarModuleData {
        version: 1,
        name: "coroutine_dispatch".to_string(),
        constants: vec![NyarConstant::Integer32(42), NyarConstant::Integer32(999)],
        functions: vec![
            NyarFunction { name: "gen".to_string(), arity: 0, local_count: 0, code_offset: gen_offset, code_length: main_offset - gen_offset },
            NyarFunction {
                name: "main".to_string(),
                arity: 0,
                local_count: 1,
                code_offset: main_offset,
                code_length: code.len() as i32 - main_offset,
            },
        ],
        imports: Vec::new(),
        exports: vec![NyarExport { kind: NyarExportKind::Function, symbol_name: "main".to_string(), function_index: 1 }],
        witness_entries: Vec::new(),
        code_bytes: code,
        globals: Vec::new(),
        init_function_indices: Vec::new(),
    }
}

#[test]
fn yield_resume_roundtrip_returns_resumed_value() {
    let module = build_yield_resume_module();
    let bytes = encode_module(&module);
    let mut vm = NyarVm::new();
    let loaded = vm.load(&bytes).expect("load module");
    let result = vm.run(&loaded, "main", Vec::new()).expect("execute main");

    // gen yields 42 to suspend; main resumes with 999; gen returns the resumed value (999);
    // main returns whatever gen returned. The closed loop proves Yield + Resume dispatch.
    assert_eq!(result, Value::I32(999));
}

#[test]
fn yield_without_call_returns_coroutine_at_top_level() {
    // When the top-level function itself yields, there is no parent frame to resume it.
    // The executor must return the captured coroutine value instead of looping forever.
    let mut code = Vec::new();
    emit_imm1(&mut code, NyarHeadCode::Const, 0); // 42
    emit_imm1(&mut code, NyarHeadCode::Yield, 0);
    // If resumed, the next instruction would be Return; but at top level we never get here.
    emit_plain(&mut code, NyarHeadCode::Return);

    let module = NyarModuleData {
        version: 1,
        name: "top_yield".to_string(),
        constants: vec![NyarConstant::Integer32(42)],
        functions: vec![NyarFunction { name: "gen".to_string(), arity: 0, local_count: 0, code_offset: 0, code_length: code.len() as i32 }],
        imports: Vec::new(),
        exports: vec![NyarExport { kind: NyarExportKind::Function, symbol_name: "gen".to_string(), function_index: 0 }],
        witness_entries: Vec::new(),
        code_bytes: code,
        globals: Vec::new(),
        init_function_indices: Vec::new(),
    };
    let bytes = encode_module(&module);
    let mut vm = NyarVm::new();
    let loaded = vm.load(&bytes).expect("load module");
    let result = vm.run(&loaded, "gen", Vec::new()).expect("execute gen");

    let coroutine_id = match result {
        Value::Coroutine(id) => id,
        other => panic!("expected coroutine, got {other:?}"),
    };
    let state = vm.heap().get_coroutine(coroutine_id).expect("coroutine in heap");
    assert!(!state.done, "freshly suspended coroutine must not be done");
    assert_eq!(state.yielded_value, Value::I32(42), "yielded value must be 42");
}

#[test]
fn resuming_a_completed_coroutine_is_rejected() {
    // After a coroutine has run to completion (Return), its `done` flag flips to `true`
    // in the heap entry. A second Resume issued from a stale local copy of the same
    // coroutine must be rejected by the Resume handler — proving the `done` writeback
    // path through `Frame::coroutine_origin` is wired correctly.
    //
    // Layout:
    //   gen (function 0): Const 42, Yield, Const 100, Return
    //   main (function 1):
    //     Call 0                 ; suspend, push coroutine(yielded=42)
    //     StoreLocal 0           ; local 0 = coroutine
    //     LoadLocal 0            ; push coroutine
    //     Const 1 (999)          ; push resume value
    //     Resume                 ; first resume: ok, gen returns 100
    //     Pop                    ; drop the returned value, exposing coroutine? No —
    //                            ; the coroutine was consumed by Resume. To attempt a
    //                            ; second resume we re-LoadLocal 0: that copy still
    //                            ; references the same heap id, now marked `done`.
    //     LoadLocal 0            ; push stale coroutine copy (same heap id)
    //     Const 1 (999)          ; push resume value
    //     Resume                 ; second resume: must error
    //     Return                 ; unreachable on success path
    let mut code = Vec::new();
    let gen_offset = 0i32;
    emit_imm1(&mut code, NyarHeadCode::Const, 0); // 42
    emit_imm1(&mut code, NyarHeadCode::Yield, 0);
    emit_imm1(&mut code, NyarHeadCode::Const, 1); // 100
    emit_plain(&mut code, NyarHeadCode::Return);

    let main_offset = code.len() as i32;
    emit_imm1(&mut code, NyarHeadCode::Call, 0);
    emit_imm1(&mut code, NyarHeadCode::StoreLocal, 0);
    emit_imm1(&mut code, NyarHeadCode::LoadLocal, 0);
    emit_imm1(&mut code, NyarHeadCode::Const, 2); // 999
    emit_plain(&mut code, NyarHeadCode::Resume);
    // First resume succeeded; gen returned 100 (on stack). Drop it so the stack is clean
    // before the second resume attempt.
    emit_plain(&mut code, NyarHeadCode::Pop);
    emit_imm1(&mut code, NyarHeadCode::LoadLocal, 0); // stale coroutine copy
    emit_imm1(&mut code, NyarHeadCode::Const, 2); // 999
    emit_plain(&mut code, NyarHeadCode::Resume);
    emit_plain(&mut code, NyarHeadCode::Return);

    let module = NyarModuleData {
        version: 1,
        name: "coroutine_double_resume".to_string(),
        constants: vec![NyarConstant::Integer32(42), NyarConstant::Integer32(100), NyarConstant::Integer32(999)],
        functions: vec![
            NyarFunction { name: "gen".to_string(), arity: 0, local_count: 0, code_offset: gen_offset, code_length: main_offset - gen_offset },
            NyarFunction {
                name: "main".to_string(),
                arity: 0,
                local_count: 1,
                code_offset: main_offset,
                code_length: code.len() as i32 - main_offset,
            },
        ],
        imports: Vec::new(),
        exports: vec![NyarExport { kind: NyarExportKind::Function, symbol_name: "main".to_string(), function_index: 1 }],
        witness_entries: Vec::new(),
        code_bytes: code,
        globals: Vec::new(),
        init_function_indices: Vec::new(),
    };
    let bytes = encode_module(&module);
    let mut vm = NyarVm::new();
    let loaded = vm.load(&bytes).expect("load module");
    let result = vm.run(&loaded, "main", Vec::new());

    match result {
        Err(nvm::NyarRuntimeError::TypeMismatch { expected, actual }) if expected == "active coroutine" && actual == "completed coroutine" => {
            // expected: Resume handler rejected the second resume because `done == true`
        }
        other => panic!("expected TypeMismatch(active coroutine vs completed coroutine), got {other:?}"),
    }
}
