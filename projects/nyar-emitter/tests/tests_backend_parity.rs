//! 多后端 parity 一致性矩阵：验证 CLR/JVM/WASM/Native/NyarVM 在高阶控制流与状态机消费上的结构一致性。
//!
//! 这些测试覆盖 spec `unify-high-order-control-flow` Task 8 的 SubTask 8.3（多后端最小一致性矩阵）。
//!
//! 核心断言：同一挂起函数在 state-machine 载荷（`ControlFlowPayload`，CLR/JVM/WASM/Native 消费）
//! 与 first-class 载荷（`SuspendRuntimePayload`，NyarVM 默认消费）之间，`state_id` / `effect` /
//! `resume_parameter_count` / `frame_fields` / `witness_bindings` 保持一致；且每条 lane 只消费一种
//! suspend 模型（双轨漂移已被 `validate_suspend_submission` 在 API 边界拒绝）。

use std::{collections::BTreeMap, path::Path};

use nyar::{
    CapabilityTag, ClrSuspendStrategy, ControlFlowPayload, HostProjectionBoundary, Identifier, QualifiedName, SuspendConsumptionModel,
    SuspendRuntimePayload, TargetBackendFamily, TargetLane, VmSuspendStrategy, WitnessMethodSlotSubmission, WitnessSubmission,
    suspend_consumption_model_for_lane,
};
use nyar_emitter::{
    FragmentSubmission, LoweredBackendInput, build_first_class_suspend_payload, build_state_machine_suspend_payload,
    executable_provider::{ExecutableFunction, MirFunctionMapProvider},
};
use nyar_language::MirLowerer;
use std::sync::Arc;

#[allow(dead_code)]
#[path = "../../nyar-language/tests/valkyrie/control_flow/fixtures.rs"]
mod control_flow_fixtures;

use control_flow_fixtures::{AWAIT_FUTURE, YIELD_FROM_ITERATOR, YIELD_GENERATOR, compile_fixture};

/// 将 MIR function symbol 字符串（如 `main::gen`）拆分为 `QualifiedName` 片段。
fn qualified_symbol_from_string(symbol: &str) -> QualifiedName {
    let parts: Vec<Identifier> = symbol.split("::").map(Identifier::new).collect();
    QualifiedName::new(parts)
}

/// 从 `HirModule` 提取所有 `MirFunction`，构造 `mir_functions` 映射供 `FragmentSubmission` 携带。
fn extract_mir_functions(hir: &nyar_language::types::hir::HirModule) -> BTreeMap<QualifiedName, ExecutableFunction> {
    let mir = MirLowerer::lower_module(hir);
    mir.functions.iter().map(|function| (qualified_symbol_from_string(&function.symbol), function.clone().into())).collect()
}

/// 从 `HirModule` 中查找指定后缀的函数符号，返回其 `QualifiedName`。
fn find_function_symbol(hir: &nyar_language::types::hir::HirModule, function_suffix: &str) -> QualifiedName {
    let mir = MirLowerer::lower_module(hir);
    let function = mir
        .functions
        .iter()
        .find(|function| function.symbol.ends_with(function_suffix))
        .unwrap_or_else(|| panic!("expected mir function ending with `{function_suffix}`"));
    qualified_symbol_from_string(&function.symbol)
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

/// 构造 state-machine `FragmentSubmission`（携带 `control_flow` payload）。
fn state_machine_submission(
    module: &str,
    fragment: &str,
    function_name: &str,
    hir: &nyar_language::types::hir::HirModule,
    witness_tables: Vec<WitnessSubmission>,
    capabilities: Vec<CapabilityTag>,
) -> FragmentSubmission {
    let symbol = find_function_symbol(hir, function_name);
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

/// 构造 first-class `FragmentSubmission`（携带 `suspend_runtime` payload）。
fn first_class_submission(
    module: &str,
    fragment: &str,
    function_name: &str,
    hir: &nyar_language::types::hir::HirModule,
    witness_tables: Vec<WitnessSubmission>,
    capabilities: Vec<CapabilityTag>,
) -> FragmentSubmission {
    let symbol = find_function_symbol(hir, function_name);
    let payload = build_first_class_suspend_payload(hir, &[symbol]);
    let mir_map = extract_mir_functions(hir);
    FragmentSubmission {
        module_name: module.to_string(),
        fragment_id: Identifier::new(fragment),
        required_capabilities: capabilities,
        witness_tables,
        suspend_runtime: Some(payload),
        executable: Some(Arc::new(MirFunctionMapProvider::new(mir_map))),
        ..Default::default()
    }
}

/// 提交到 CLR state-machine lane 并断言成功。
fn clr_state_machine_lane_accept(submission: &FragmentSubmission) {
    LoweredBackendInput::from_fragment_submission(
        submission,
        TargetBackendFamily::Clr,
        HostProjectionBoundary::Clr,
        Path::new("."),
        TargetLane::Clr,
        ClrSuspendStrategy::StateMachine,
        VmSuspendStrategy::default(),
        "win32",
    )
    .expect("CLR state-machine lane should accept control_flow submission");
}

/// 提交到 NyarVM first-class lane 并断言成功。
fn nyar_vm_first_class_lane_accept(submission: &FragmentSubmission) {
    LoweredBackendInput::from_fragment_submission(
        submission,
        TargetBackendFamily::NyarVm,
        HostProjectionBoundary::Vm,
        Path::new("."),
        TargetLane::Vm,
        ClrSuspendStrategy::default(),
        VmSuspendStrategy::FirstClass,
        "default",
    )
    .expect("NyarVM first-class lane should accept suspend_runtime submission");
}

/// 构造仅携带 `suspend` 能力的最小 state-machine 提交（用于校验路径，不依赖真实 fixture）。
fn minimal_state_machine_submission(fragment: &str) -> FragmentSubmission {
    FragmentSubmission {
        module_name: "parity".to_string(),
        fragment_id: Identifier::new(fragment),
        required_capabilities: vec![CapabilityTag::new("suspend")],
        control_flow: Some(ControlFlowPayload { functions: Vec::new() }),
        ..Default::default()
    }
}

/// 构造仅携带 `suspend` 能力的最小 first-class 提交（用于校验路径，不依赖真实 fixture）。
fn minimal_first_class_submission(fragment: &str) -> FragmentSubmission {
    FragmentSubmission {
        module_name: "parity".to_string(),
        fragment_id: Identifier::new(fragment),
        required_capabilities: vec![CapabilityTag::new("suspend")],
        suspend_runtime: Some(SuspendRuntimePayload { functions: Vec::new() }),
        ..Default::default()
    }
}

#[test]
fn clr_and_first_class_lane_produce_equivalent_yield_state_count() {
    let hir = compile_fixture(YIELD_GENERATOR);
    let symbol = find_function_symbol(&hir, "gen");

    let sm_payload = build_state_machine_suspend_payload(&hir, &[symbol.clone()]);
    let fc_payload = build_first_class_suspend_payload(&hir, &[symbol]);

    let sm_artifact = sm_payload.functions.first().expect("state-machine artifact");
    let fc_artifact = fc_payload.functions.first().expect("first-class artifact");

    assert_eq!(sm_artifact.states.len(), fc_artifact.states.len(), "state count must match between state-machine and first-class payloads");
    assert_eq!(sm_artifact.states.len(), 1, "yield generator must produce exactly one suspend state");

    clr_state_machine_lane_accept(&state_machine_submission(
        "parity",
        "yield_sm",
        "gen",
        &hir,
        Vec::new(),
        vec![CapabilityTag::new("suspend")],
    ));
    nyar_vm_first_class_lane_accept(&first_class_submission(
        "parity",
        "yield_fc",
        "gen",
        &hir,
        Vec::new(),
        vec![CapabilityTag::new("suspend")],
    ));
}

#[test]
fn clr_and_first_class_lane_produce_equivalent_yield_effect_label() {
    let hir = compile_fixture(YIELD_GENERATOR);
    let symbol = find_function_symbol(&hir, "gen");

    let sm_payload = build_state_machine_suspend_payload(&hir, &[symbol.clone()]);
    let fc_payload = build_first_class_suspend_payload(&hir, &[symbol]);

    let sm_state = sm_payload.functions.first().expect("state-machine artifact").states.first().expect("state-machine state");
    let fc_state = fc_payload.functions.first().expect("first-class artifact").states.first().expect("first-class state");

    assert_eq!(sm_state.effect, fc_state.effect, "effect label must match between state-machine and first-class payloads");
    assert_eq!(sm_state.effect, "Yield", "yield generator must produce Yield effect");
}

#[test]
fn clr_and_first_class_lane_produce_equivalent_await_resume_parameter_count() {
    let hir = compile_fixture(AWAIT_FUTURE);
    let symbol = find_function_symbol(&hir, "async_fn");

    let sm_payload = build_state_machine_suspend_payload(&hir, &[symbol.clone()]);
    let fc_payload = build_first_class_suspend_payload(&hir, &[symbol]);

    let sm_state = sm_payload.functions.first().expect("state-machine artifact").states.first().expect("state-machine state");
    let fc_state = fc_payload.functions.first().expect("first-class artifact").states.first().expect("first-class state");

    assert_eq!(sm_state.effect, fc_state.effect, "effect label must match for await fixture");
    assert_eq!(sm_state.effect, "Await", "await fixture must produce Await effect");

    assert_eq!(
        sm_state.resume_parameter_count, fc_state.resume_parameter_count,
        "resume_parameter_count must match between state-machine and first-class payloads"
    );
}

#[test]
fn clr_and_first_class_lane_produce_equivalent_frame_fields_count() {
    let hir = compile_fixture(AWAIT_FUTURE);
    let symbol = find_function_symbol(&hir, "async_fn");

    let sm_payload = build_state_machine_suspend_payload(&hir, &[symbol.clone()]);
    let fc_payload = build_first_class_suspend_payload(&hir, &[symbol]);

    let sm_artifact = sm_payload.functions.first().expect("state-machine artifact");
    let fc_artifact = fc_payload.functions.first().expect("first-class artifact");

    assert_eq!(
        sm_artifact.frame_fields.len(),
        fc_artifact.frame_fields.len(),
        "frame_fields count must match between state-machine and first-class payloads"
    );

    let sm_state = sm_artifact.states.first().expect("state-machine state");
    let fc_state = fc_artifact.states.first().expect("first-class state");
    assert_eq!(
        sm_state.spill_fields.len(),
        fc_state.spill_fields.len(),
        "spill_fields count must match between state-machine and first-class payloads"
    );
}

#[test]
fn nyar_vm_lane_consumes_single_suspend_model() {
    assert_eq!(
        suspend_consumption_model_for_lane(TargetLane::Vm, ClrSuspendStrategy::default(), VmSuspendStrategy::FirstClass),
        SuspendConsumptionModel::FirstClass,
        "NyarVM default strategy must map to FirstClass consumption model"
    );
    assert_eq!(
        suspend_consumption_model_for_lane(TargetLane::Vm, ClrSuspendStrategy::default(), VmSuspendStrategy::StateMachine),
        SuspendConsumptionModel::StateMachine,
        "NyarVM StateMachine strategy must map to StateMachine consumption model"
    );

    let sm_submission = minimal_state_machine_submission("vm_single_sm");
    let fc_submission = minimal_first_class_submission("vm_single_fc");

    LoweredBackendInput::from_fragment_submission(
        &fc_submission,
        TargetBackendFamily::NyarVm,
        HostProjectionBoundary::Vm,
        Path::new("."),
        TargetLane::Vm,
        ClrSuspendStrategy::default(),
        VmSuspendStrategy::FirstClass,
        "default",
    )
    .expect("NyarVM FirstClass must accept suspend_runtime payload");

    let err = LoweredBackendInput::from_fragment_submission(
        &sm_submission,
        TargetBackendFamily::NyarVm,
        HostProjectionBoundary::Vm,
        Path::new("."),
        TargetLane::Vm,
        ClrSuspendStrategy::default(),
        VmSuspendStrategy::FirstClass,
        "default",
    )
    .expect_err("NyarVM FirstClass must reject control_flow payload");
    assert!(err.to_string().contains("first-class suspend lane"), "unexpected rejection message: {err}");

    LoweredBackendInput::from_fragment_submission(
        &sm_submission,
        TargetBackendFamily::NyarVm,
        HostProjectionBoundary::Vm,
        Path::new("."),
        TargetLane::Vm,
        ClrSuspendStrategy::default(),
        VmSuspendStrategy::StateMachine,
        "default",
    )
    .expect("NyarVM StateMachine must accept control_flow payload");

    let err = LoweredBackendInput::from_fragment_submission(
        &fc_submission,
        TargetBackendFamily::NyarVm,
        HostProjectionBoundary::Vm,
        Path::new("."),
        TargetLane::Vm,
        ClrSuspendStrategy::default(),
        VmSuspendStrategy::StateMachine,
        "default",
    )
    .expect_err("NyarVM StateMachine must reject suspend_runtime payload");
    assert!(err.to_string().contains("state-machine lane"), "unexpected rejection message: {err}");
}

#[test]
fn cross_lane_state_id_consistency_for_yield_from() {
    let hir = compile_fixture(YIELD_FROM_ITERATOR);
    let symbol = find_function_symbol(&hir, "gen");

    let sm_payload = build_state_machine_suspend_payload(&hir, &[symbol.clone()]);
    let fc_payload = build_first_class_suspend_payload(&hir, &[symbol]);

    let sm_state = sm_payload.functions.first().expect("state-machine artifact").states.first().expect("state-machine state");
    let fc_state = fc_payload.functions.first().expect("first-class artifact").states.first().expect("first-class state");

    assert_eq!(sm_state.effect, "DelegateYield", "yield from must produce DelegateYield effect");
    assert_eq!(sm_state.effect, fc_state.effect, "effect must match across state-machine and first-class lanes");
    assert_eq!(sm_state.state_id, fc_state.state_id, "state_id must be consistent across state-machine and first-class lanes");
    assert_eq!(sm_state.resume_parameter_count, fc_state.resume_parameter_count, "resume_parameter_count must be consistent across lanes");

    assert!(
        sm_state.witness_bindings.iter().any(|binding| binding.trait_name == "Iterator" && binding.method_name == "next"),
        "state-machine payload must carry Iterator.next witness binding"
    );
    assert!(
        fc_state.witness_bindings.iter().any(|binding| binding.trait_name == "Iterator" && binding.method_name == "next"),
        "first-class payload must carry Iterator.next witness binding"
    );

    let sm_submission = state_machine_submission(
        "parity",
        "yield_from_sm",
        "gen",
        &hir,
        vec![iterator_witness_table()],
        vec![CapabilityTag::new("suspend"), CapabilityTag::new("trait-witness")],
    );
    let fc_submission = first_class_submission(
        "parity",
        "yield_from_fc",
        "gen",
        &hir,
        vec![iterator_witness_table()],
        vec![CapabilityTag::new("suspend"), CapabilityTag::new("trait-witness")],
    );

    clr_state_machine_lane_accept(&sm_submission);
    nyar_vm_first_class_lane_accept(&fc_submission);
}
