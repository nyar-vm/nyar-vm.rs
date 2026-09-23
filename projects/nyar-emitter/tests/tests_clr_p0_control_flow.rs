//! CLR P0 闭环测试：验证 `continue` / `break` / `fallthrough` / `return` / `yield` / `yield from` / `await`
//! 的核心端到端用例在 CLR state-machine lane 上稳定运行。
//!
//! 这些测试覆盖 spec `unify-high-order-control-flow` Task 7 的 SubTask 7.2（7 个核心用例）与
//! SubTask 7.3（跨层 state/frame/resume 一致性断言）。

use std::{collections::BTreeMap, path::Path};

use nyar::{
    CapabilityTag, ClrSuspendStrategy, HostProjectionBoundary, Identifier, QualifiedName, TargetBackendFamily, TargetLane, VmSuspendStrategy,
    WitnessMethodSlotSubmission, WitnessSubmission,
};
use nyar_emitter::{
    FragmentSubmission, LoweredBackendInput, build_state_machine_suspend_payload,
    executable_provider::{ExecutableFunction, MirFunctionMapProvider},
};
use nyar_language::MirLowerer;
use std::sync::Arc;

#[allow(dead_code)]
#[path = "../../nyar-language/tests/valkyrie/control_flow/fixtures.rs"]
mod control_flow_fixtures;

use control_flow_fixtures::{
    AWAIT_FUTURE, BREAK_IN_LOOP, CONTINUE_IN_LOOP, EXPLICIT_RETURN, FALLTHROUGH_IN_CASE, NULLABLE_TRY_PROPAGATE, YIELD_FROM_ITERATOR,
    YIELD_GENERATOR, assert_await_state_machine_shape, assert_break_mir_shape, assert_continue_mir_shape, assert_fallthrough_mir_shape,
    assert_nullable_try_mir_shape, assert_return_mir_shape, assert_yield_from_state_machine_shape, assert_yield_state_machine_shape,
    compile_fixture,
};

/// 把 MIR function symbol 字符串（如 `main::gen`）拆分为 `QualifiedName` 片段。
fn qualified_symbol_from_string(symbol: &str) -> QualifiedName {
    let parts: Vec<Identifier> = symbol.split("::").map(Identifier::new).collect();
    QualifiedName::new(parts)
}

/// 从 `HirModule` 提取所有 `MirFunction`，构造 `mir_functions` 映射供 `FragmentSubmission` 携带。
fn extract_mir_functions(hir: &nyar_language::types::hir::HirModule) -> BTreeMap<QualifiedName, ExecutableFunction> {
    let mir = MirLowerer::lower_module(hir);
    mir.functions.iter().map(|function| (qualified_symbol_from_string(&function.symbol), function.clone().into())).collect()
}

/// 调用 `LoweredBackendInput::from_fragment_submission` 把提交送到 CLR state-machine lane，断言成功。
fn clr_lane_accept(submission: FragmentSubmission) {
    LoweredBackendInput::from_fragment_submission(
        &submission,
        TargetBackendFamily::Clr,
        HostProjectionBoundary::Clr,
        Path::new("."),
        TargetLane::Clr,
        ClrSuspendStrategy::StateMachine,
        VmSuspendStrategy::default(),
        "win32",
    )
    .expect("CLR state-machine lane should accept submission");
}

/// 构造普通（非 suspend）函数的 `FragmentSubmission`，仅携带 `mir_functions`。
fn plain_submission(module: &str, fragment: &str, hir: &nyar_language::types::hir::HirModule) -> FragmentSubmission {
    let mir_map = extract_mir_functions(hir);
    FragmentSubmission {
        module_name: module.to_string(),
        fragment_id: Identifier::new(fragment),
        executable: Some(Arc::new(MirFunctionMapProvider::new(mir_map))),
        ..Default::default()
    }
}

/// 构造 `Iterator.next` 的合成 witness 表，供 `yield from` 用例使用。
fn iterator_witness_table() -> WitnessSubmission {
    WitnessSubmission {
        type_name: "Iterator".to_string(),
        trait_name: "Iterator".to_string(),
        table_label: "witness_table_Iterator_Iterator".to_string(),
        fat_ptr_label: "witness_fat_Iterator_Iterator".to_string(),
        methods: vec![WitnessMethodSlotSubmission {
            method_name: "next".to_string(),
            impl_symbol: "witness_Iterator_Iterator_next".to_string(),
            method_index: 0,
        }],
        result_literal: String::new(),
    }
}

/// 构造 `Future.poll` 的合成 witness 表，供 `await` 用例使用。
fn future_witness_table() -> WitnessSubmission {
    WitnessSubmission {
        type_name: "Future".to_string(),
        trait_name: "Future".to_string(),
        table_label: "witness_table_Future_Future".to_string(),
        fat_ptr_label: "witness_fat_Future_Future".to_string(),
        methods: vec![WitnessMethodSlotSubmission {
            method_name: "poll".to_string(),
            impl_symbol: "witness_Future_Future_poll".to_string(),
            method_index: 0,
        }],
        result_literal: String::new(),
    }
}

/// 构造 suspend 函数的 `FragmentSubmission`，携带 `control_flow` payload、witness 表与 `mir_functions`。
fn suspend_submission(
    module: &str,
    fragment: &str,
    function_name: &str,
    hir: &nyar_language::types::hir::HirModule,
    witness_tables: Vec<WitnessSubmission>,
    capabilities: Vec<CapabilityTag>,
) -> FragmentSubmission {
    let mir = MirLowerer::lower_module(hir);
    let function = mir
        .functions
        .iter()
        .find(|function| function.symbol.ends_with(function_name))
        .unwrap_or_else(|| panic!("expected mir function ending with `{function_name}`"));
    let symbol = qualified_symbol_from_string(&function.symbol);
    let payload = build_state_machine_suspend_payload(hir, &[symbol]);
    let mir_map = extract_mir_functions(hir);
    FragmentSubmission {
        module_name: module.to_string(),
        fragment_id: Identifier::new(fragment),
        required_capabilities: capabilities,
        witness_tables,
        control_flow: Some(payload),
        executable: Some(Arc::new(MirFunctionMapProvider::new(mir_map))),
        ..Default::default()
    }
}

/// 断言前端 `SuspendLoweringPlan` descriptor 与 backend `SuspendFunctionArtifact` 的 state_id / effect /
/// resume_parameter_count / frame_carrier / continuation_index / spill_fields 一致（Task 7.3 跨层一致性）。
fn assert_cross_layer_consistency(hir: &nyar_language::types::hir::HirModule, function_name: &str) {
    let mir = MirLowerer::lower_module(hir);
    let function = mir.functions.iter().find(|f| f.symbol.contains(function_name)).expect("suspend function");
    let plan = function.suspend_plan.as_ref().expect("suspend plan");
    assert_eq!(plan.states.len(), 1, "expected single suspend state in frontend plan");

    let symbol = qualified_symbol_from_string(&function.symbol);
    let payload = build_state_machine_suspend_payload(hir, &[symbol]);
    let artifact = payload.functions.first().expect("state machine artifact");
    assert_eq!(artifact.states.len(), 1, "expected single suspend state in backend artifact");

    let frontend_state = &plan.states[0];
    let backend_state = &artifact.states[0];

    assert_eq!(frontend_state.state_id, backend_state.state_id, "state_id must match across layers");
    assert_eq!(format!("{:?}", frontend_state.effect), backend_state.effect, "effect must match across layers");
    assert_eq!(frontend_state.resume_parameter_count, backend_state.resume_parameter_count, "resume_parameter_count must match across layers");
    assert_eq!(frontend_state.frame_carrier, backend_state.frame_carrier, "frame_carrier must match across layers");
    assert_eq!(frontend_state.continuation_index, backend_state.continuation_index, "continuation_index must match across layers");

    let frontend_spill_count = frontend_state.spill_slots.len();
    let backend_spill_count = backend_state.spill_fields.len();
    if frontend_spill_count > 0 {
        assert_eq!(frontend_spill_count, backend_spill_count, "spill_fields count must match when spill_slots non-empty");
    }
}

#[test]
fn clr_lane_consumes_explicit_return_payload() {
    let hir = compile_fixture(EXPLICIT_RETURN);
    assert_return_mir_shape(&hir);
    clr_lane_accept(plain_submission("return_demo", "return_main", &hir));
}

#[test]
fn clr_lane_consumes_break_expr_payload() {
    let hir = compile_fixture(BREAK_IN_LOOP);
    assert_break_mir_shape(&hir);
    clr_lane_accept(plain_submission("break_demo", "break_main", &hir));
}

#[test]
fn clr_lane_consumes_continue_payload() {
    let hir = compile_fixture(CONTINUE_IN_LOOP);
    assert_continue_mir_shape(&hir);
    clr_lane_accept(plain_submission("continue_demo", "continue_main", &hir));
}

#[test]
fn clr_lane_consumes_fallthrough_payload() {
    let hir = compile_fixture(FALLTHROUGH_IN_CASE);
    assert_fallthrough_mir_shape(&hir);
    clr_lane_accept(plain_submission("fallthrough_demo", "fallthrough_main", &hir));
}

#[test]
fn clr_lane_consumes_try_propagate_payload() {
    let hir = compile_fixture(NULLABLE_TRY_PROPAGATE);
    assert_nullable_try_mir_shape(&hir);
    clr_lane_accept(plain_submission("try_propagate_demo", "try_propagate_main", &hir));
}

#[test]
fn clr_lane_consumes_yield_payload() {
    let hir = compile_fixture(YIELD_GENERATOR);
    assert_yield_state_machine_shape(&hir);
    assert_cross_layer_consistency(&hir, "gen");
    clr_lane_accept(suspend_submission("yield_demo", "yield_main", "gen", &hir, Vec::new(), vec![CapabilityTag::new("suspend")]));
}

#[test]
fn clr_lane_consumes_yield_from_payload() {
    let hir = compile_fixture(YIELD_FROM_ITERATOR);
    assert_yield_from_state_machine_shape(&hir);
    clr_lane_accept(suspend_submission(
        "yield_from_demo",
        "yield_from_main",
        "gen",
        &hir,
        vec![iterator_witness_table()],
        vec![CapabilityTag::new("suspend"), CapabilityTag::new("trait-witness")],
    ));
}

#[test]
fn clr_lane_consumes_await_payload() {
    let hir = compile_fixture(AWAIT_FUTURE);
    assert_await_state_machine_shape(&hir);
    assert_cross_layer_consistency(&hir, "async_fn");
    clr_lane_accept(suspend_submission(
        "await_demo",
        "await_main",
        "async_fn",
        &hir,
        vec![future_witness_table()],
        vec![CapabilityTag::new("suspend"), CapabilityTag::new("trait-witness")],
    ));
}
