//! Cross-backend parity tests for value semantics (Task 9).
//!
//! These integration tests compile shared value-semantic fixtures from source,
//! lower them to MIR, and assert that all five bundled backend lowering paths
//! (CLR / JVM / WASM / NyarVM / Native) consume the resulting `FragmentSubmission`
//! without panic. This establishes a minimal parity guarantee: the same MIR
//! fixture is accepted by every backend, even if bytecode details differ.
//!
//! Additionally, driver-level regression tests verify that `layout_id` metadata
//! and `ByAddress` receiver semantics flow through the driver boundary correctly.

use nyar::{ClrSuspendStrategy, HostProjectionBoundary, Identifier, QualifiedName, TargetBackendFamily, TargetLane, VmSuspendStrategy};
use nyar_emitter::{FragmentSubmission, LoweredBackendInput, executable_provider::MirFunctionMapProvider};
use nyar_language::{MirInstructionKind, MirLowerer, MirModule, ReceiverPassingKind, ValkyrieCompiler, types::SourceID};
use std::sync::Arc;
use tempfile::tempdir;

/// Compile source code to MIR and build a `FragmentSubmission` that carries
/// the module's aggregate layout plan and the `main` function's MIR body.
///
/// The submission is the minimal payload required for all five backend lowering
/// paths to exercise value-semantic instructions (`StructNew`, `AggregateCopy`,
/// `FieldGet`, `Call` with `ByAddress` receiver, etc.) without requiring a full
/// frontend artifact plan.
fn build_submission_from_source(source: &str, version_id: u32) -> (MirModule, FragmentSubmission) {
    let hir = ValkyrieCompiler::new(SourceID { version_id }).compile_source(source).expect("HIR compilation should succeed");
    let mir = MirLowerer::lower_module_semantic(&hir);
    let plan = mir.aggregate_layouts.clone();
    let mut submission = FragmentSubmission::default();
    submission.module_name = "parity".to_string();
    submission.aggregate_layouts = plan;
    let main_function = mir.functions.iter().find(|function| function.symbol.ends_with("main")).expect("main MIR function should exist");
    let main_operation = QualifiedName::new(vec![Identifier::new("main")]);
    submission.exported_operations.push(main_operation.clone());
    submission.entry_operation = Some(main_operation.clone());
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new([(main_operation, main_function.clone().into())].into_iter().collect())));
    (mir, submission)
}

/// Assert that the given submission can be consumed by all five bundled backend
/// lowering paths without panic.
///
/// Each backend is invoked via `LoweredBackendInput::from_fragment_submission`,
/// which internally dispatches to the backend-specific lowering function
/// (`lower_fragment_to_msil`, `lower_fragment_to_jvm_class`,
/// `lower_fragment_to_wasm_module`, `lower_fragment_to_native_executable`,
/// `lower_fragment_to_nyar_module`). A successful `Ok` return means the backend
/// consumed the value-semantic MIR without panic and produced a non-empty
/// backend input.
fn assert_all_backends_consume(submission: &FragmentSubmission) {
    let output_dir = tempdir().expect("temp dir");
    let clr_strategy = ClrSuspendStrategy::default();
    let vm_strategy = VmSuspendStrategy::default();

    LoweredBackendInput::from_fragment_submission(
        submission,
        TargetBackendFamily::Clr,
        HostProjectionBoundary::Clr,
        output_dir.path(),
        TargetLane::Clr,
        clr_strategy,
        vm_strategy,
        "linux-gnu",
    )
    .expect("CLR backend should consume value-semantic submission without panic");

    LoweredBackendInput::from_fragment_submission(
        submission,
        TargetBackendFamily::Jvm,
        HostProjectionBoundary::Jvm,
        output_dir.path(),
        TargetLane::Jvm,
        clr_strategy,
        vm_strategy,
        "linux-gnu",
    )
    .expect("JVM backend should consume value-semantic submission without panic");

    LoweredBackendInput::from_fragment_submission(
        submission,
        TargetBackendFamily::Wasm,
        HostProjectionBoundary::WasmJsGlue,
        output_dir.path(),
        TargetLane::Wasm,
        clr_strategy,
        vm_strategy,
        "linux-gnu",
    )
    .expect("WASM backend should consume value-semantic submission without panic");

    LoweredBackendInput::from_fragment_submission(
        submission,
        TargetBackendFamily::NyarVm,
        HostProjectionBoundary::Vm,
        output_dir.path(),
        TargetLane::Vm,
        clr_strategy,
        vm_strategy,
        "linux-gnu",
    )
    .expect("NyarVM backend should consume value-semantic submission without panic");

    LoweredBackendInput::from_fragment_submission(
        submission,
        TargetBackendFamily::Native,
        HostProjectionBoundary::Native,
        output_dir.path(),
        TargetLane::Native,
        clr_strategy,
        vm_strategy,
        "linux-gnu",
    )
    .expect("Native backend should consume value-semantic submission without panic");
}

// ============================================================================
// SubTask 9.1: Cross-backend shared fixtures (scenarios A-E)
// ============================================================================

/// Scenario A: value-type `structure Point { x: f64, y: f64 }` construction
/// followed by `AggregateCopy` via `let b = a`.
///
/// All five backends must consume the resulting MIR without panic.
#[test]
fn scenario_a_struct_value_aggregate_copy_parity() {
    let source = r#"
structure Point {
    x: f64,
    y: f64,
}

micro main() {
    let a = Point { x: 1.0, y: 2.0 };
    let b = a;
}
"#;
    let (_mir, submission) = build_submission_from_source(source, 9700);
    assert_all_backends_consume(&submission);
}

/// Scenario B: tuple `(i32, i32)` construction via `tuple(1, 2)`.
///
/// All five backends must consume the resulting MIR without panic.
#[test]
fn scenario_b_tuple_value_aggregate_parity() {
    let source = r#"
micro main() {
    let pair = tuple(1, 2);
}
"#;
    let (_mir, submission) = build_submission_from_source(source, 9701);
    assert_all_backends_consume(&submission);
}

/// Scenario C: fixed-array `[i32; 2]` construction.
///
/// All five backends must consume the resulting MIR without panic.
#[test]
fn scenario_c_fixed_array_value_aggregate_parity() {
    let source = r#"
micro main() {
    let items: [i32; 2] = [1, 2];
}
"#;
    let (_mir, submission) = build_submission_from_source(source, 9702);
    assert_all_backends_consume(&submission);
}

/// Scenario D: nested value type — a `structure Line` whose fields are
/// themselves value-type `Point` structures.
///
/// All five backends must consume the resulting MIR without panic.
#[test]
fn scenario_d_nested_value_type_parity() {
    let source = r#"
structure Point {
    x: f64,
    y: f64,
}

structure Line {
    start: Point,
    end: Point,
}

micro main() {
    let line = Line {
        start: Point { x: 1.0, y: 2.0 },
        end: Point { x: 3.0, y: 4.0 },
    };
}
"#;
    let (_mir, submission) = build_submission_from_source(source, 9703);
    assert_all_backends_consume(&submission);
}

/// Scenario E: value-receiver method call `p.length()` where `Point` is a
/// value type and `length` is an `imply` method taking `self`.
///
/// All five backends must consume the resulting MIR without panic. The
/// `Call` instruction carries `receiver_kind: Some(ByAddress)`, which each
/// backend must handle (or safely stub for Native).
#[test]
fn scenario_e_value_receiver_method_call_parity() {
    let source = r#"
structure Point {
    x: f64,
    y: f64,
}

imply Point {
    micro length(self) -> f64 {
        return self.x;
    }
}

micro main() -> f64 {
    let p = Point { x: 1.0, y: 2.0 };
    return p.length();
}
"#;
    let (_mir, submission) = build_submission_from_source(source, 9704);
    assert_all_backends_consume(&submission);
}

// ============================================================================
// SubTask 9.2: Driver-level layout_id regression + ByAddress receiver
// ============================================================================

/// Assert that `FieldGet { layout_id: Some(_) }` instructions emitted by MIR
/// lowering carry a `layout_id` that resolves in `aggregate_layouts`, and that
/// all five backends can consume the submission without panic.
///
/// This test proactively constructs a legal layout (via `MirLowerer::lower_module_semantic`)
/// and verifies the resolution path, rather than relying solely on the runtime
/// `panic!` guard in backend lowering.
#[test]
fn driver_layout_id_resolution_does_not_panic() {
    let source = r#"
structure Point {
    x: f64,
    y: f64,
}

micro main() -> f64 {
    let p = Point { x: 1.0, y: 2.0 };
    return p.x;
}
"#;
    let (mir, submission) = build_submission_from_source(source, 9705);

    let field_get_layout_ids: Vec<_> = mir
        .functions
        .iter()
        .flat_map(|function| function.blocks.iter())
        .flat_map(|block| block.instructions.iter())
        .filter_map(|instruction| match &instruction.kind {
            MirInstructionKind::FieldGet { layout_id: Some(id), .. } => Some(*id),
            _ => None,
        })
        .collect();
    assert!(!field_get_layout_ids.is_empty(), "expected at least one FieldGet with layout_id");
    for layout_id in &field_get_layout_ids {
        assert!(
            mir.aggregate_layouts.layouts.iter().any(|layout| layout.id == *layout_id),
            "FieldGet layout_id {layout_id} must resolve in mir.aggregate_layouts"
        );
    }
    assert_all_backends_consume(&submission);
}

/// Assert that `Call { receiver_kind: Some(ByAddress) }` is consumed by the
/// CLR, JVM, and WASM backends without panic.
///
/// Each of these three backends has explicit `ByAddress` handling in its MIR
/// lowering path. The test verifies the driver boundary passes the receiver
/// kind through and the backend lowering emits a calling instruction (the
/// exact bytecode differs per ABI, so only "no panic + Ok" is asserted).
#[test]
fn driver_by_address_receiver_consumed_by_clr_jvm_wasm() {
    let source = r#"
structure Point {
    x: f64,
    y: f64,
}

imply Point {
    micro length(self) -> f64 {
        return self.x;
    }
}

micro main() -> f64 {
    let p = Point { x: 1.0, y: 2.0 };
    return p.length();
}
"#;
    let (mir, submission) = build_submission_from_source(source, 9706);

    let has_by_address_call =
        mir.functions.iter().flat_map(|function| function.blocks.iter()).flat_map(|block| block.instructions.iter()).any(|instruction| {
            matches!(&instruction.kind, MirInstructionKind::Call { receiver_kind: Some(ReceiverPassingKind::ByAddress), .. })
        });
    assert!(has_by_address_call, "expected at least one Call with receiver_kind: Some(ByAddress)");

    let output_dir = tempdir().expect("temp dir");
    let clr_strategy = ClrSuspendStrategy::default();
    let vm_strategy = VmSuspendStrategy::default();

    LoweredBackendInput::from_fragment_submission(
        &submission,
        TargetBackendFamily::Clr,
        HostProjectionBoundary::Clr,
        output_dir.path(),
        TargetLane::Clr,
        clr_strategy,
        vm_strategy,
        "linux-gnu",
    )
    .expect("CLR backend should consume ByAddress receiver call without panic");

    LoweredBackendInput::from_fragment_submission(
        &submission,
        TargetBackendFamily::Jvm,
        HostProjectionBoundary::Jvm,
        output_dir.path(),
        TargetLane::Jvm,
        clr_strategy,
        vm_strategy,
        "linux-gnu",
    )
    .expect("JVM backend should consume ByAddress receiver call without panic");

    LoweredBackendInput::from_fragment_submission(
        &submission,
        TargetBackendFamily::Wasm,
        HostProjectionBoundary::WasmJsGlue,
        output_dir.path(),
        TargetLane::Wasm,
        clr_strategy,
        vm_strategy,
        "linux-gnu",
    )
    .expect("WASM backend should consume ByAddress receiver call without panic");
}
