//! JVM state-machine lowering for suspend functions.

use crate::nyar_backend_jvm::{
    JvmClassFile, JvmCodeBody, JvmInstruction, JvmMethodDescriptor, JvmMethodRef, JvmMethodSignature, JvmTypeDescriptor,
};
use nyar::{SuspendFunctionArtifact, SuspendStateArtifact};

use super::{
    executable::executable_has_state_machine as mir_has_state_machine,
    jvm_witness, sanitize_jvm_method_symbol, sanitize_symbol,
    suspend_sm::{dispatch_case_keys, resolve_state_for_case},
    suspend_witness::{
        frame_has_field, primary_witness_binding, resolve_witness_slot, secondary_witness_binding, tertiary_witness_binding,
        witness_receiver_field,
    },
};
use crate::FragmentSubmission;

const JVM_OBJECT: &str = "java/lang/Object";

fn state_array_type() -> JvmTypeDescriptor {
    JvmTypeDescriptor::array(JvmTypeDescriptor::Int)
}

fn frame_array_type() -> JvmTypeDescriptor {
    JvmTypeDescriptor::array(JvmTypeDescriptor::Object(JVM_OBJECT.to_string()))
}

fn move_next_descriptor() -> JvmMethodDescriptor {
    JvmMethodDescriptor::new(vec![state_array_type(), frame_array_type()], JvmTypeDescriptor::Boolean)
}

pub(crate) fn append_suspend_state_machine_methods(class_file: &mut JvmClassFile, submission: &FragmentSubmission) {
    let Some(payload) = &submission.control_flow
    else {
        return;
    };
    jvm_witness::emit_witness_impl_methods(class_file, submission);
    for function in &payload.functions {
        // 仅当函数 MIR 含状态机终止符时才生成状态机方法。catch+resume 等被优化为
        // 直接线性流的场景不含 StateDispatch/YieldToRuntime，正常 MIR lowering 已
        // 生成方法，再生成状态机方法会导致 JVM "Duplicate method name" 错误。
        // 当 MIR 不存在时（纯状态机提交，如 suspend 单元测试），control_flow.functions
        // 已表明该函数为 suspend 函数，此时默认生成状态机方法。
        let has_state_machine = submission
            .executable
            .as_ref()
            .and_then(|exec| exec.get_function(&function.symbol))
            .map(|view| mir_has_state_machine(&view.function))
            .unwrap_or(true);
        if has_state_machine {
            append_function_state_machine(class_file, function, submission);
        }
    }
}

fn class_owner(submission: &FragmentSubmission) -> String {
    format!("{}/{}", sanitize_symbol(&submission.module_name), sanitize_symbol(submission.fragment_id.as_str()))
}

fn frame_slot_count(artifact: &SuspendFunctionArtifact) -> i32 {
    let witness_slots =
        artifact.frame_fields.iter().filter(|field| field.starts_with("__witness_payload_") || field.starts_with("slot_")).count().max(2);
    i32::try_from(witness_slots).unwrap_or(i32::MAX)
}

fn append_function_state_machine(class_file: &mut JvmClassFile, artifact: &SuspendFunctionArtifact, submission: &FragmentSubmission) {
    let symbol = sanitize_jvm_method_symbol(&artifact.symbol);
    let owner = class_owner(submission);
    let frame_slots = frame_slot_count(artifact);
    class_file.methods.push(JvmMethodSignature {
        name: format!("sm_{symbol}_move_next"),
        descriptor: move_next_descriptor(),
        access_flags: 0x0001 | 0x0008,
        code: Some(JvmCodeBody { max_stack: 8, max_locals: 4, instructions: jvm_move_next_instructions(artifact, submission, &owner) }),
    });
    class_file.methods.push(JvmMethodSignature {
        name: format!("sm_{symbol}_run"),
        descriptor: JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Int),
        access_flags: 0x0001 | 0x0008,
        code: Some(JvmCodeBody {
            max_stack: 8,
            max_locals: 4,
            instructions: jvm_run_loop_instructions(&format!("sm_{symbol}_move_next"), &owner, frame_slots, artifact),
        }),
    });
    if submission.exported_operations.iter().any(|op| op == &artifact.symbol) {
        let return_descriptor = submission
            .executable
            .as_ref()
            .and_then(|exec| exec.get_function(&artifact.symbol))
            .map(|view| super::jvm_mir::effective_return_descriptor(submission, &view.function, None))
            .unwrap_or(JvmTypeDescriptor::Int);
        // 调用方 `infer_call_descriptor` 对 Void 返回类型会转换为 Int（存在 output value 时），
        // wrapper 的返回描述符必须与之保持一致，否则 JVM 按 name+descriptor 查找方法时
        // 会抛出 `NoSuchMethodError`。与 CLR 后端 wrapper 恒返回 Int32 的行为对齐。
        let return_descriptor = match return_descriptor {
            JvmTypeDescriptor::Void => JvmTypeDescriptor::Int,
            other => other,
        };
        let wrapper_instructions = jvm_entry_wrapper_instructions(&format!("sm_{symbol}_run"), &owner, &return_descriptor);
        class_file.methods.push(JvmMethodSignature {
            name: sanitize_jvm_method_symbol(&artifact.symbol),
            descriptor: JvmMethodDescriptor::new(Vec::new(), return_descriptor),
            access_flags: 0x0001 | 0x0008,
            code: Some(JvmCodeBody { max_stack: 4, max_locals: 0, instructions: wrapper_instructions }),
        });
    }
}

fn jvm_move_next_instructions(artifact: &SuspendFunctionArtifact, submission: &FragmentSubmission, owner: &str) -> Vec<JvmInstruction> {
    let case_keys = dispatch_case_keys(artifact);
    let mut instructions = lower_move_next_dispatch(&case_keys);
    for case_key in &case_keys {
        instructions.extend(lower_move_next_case_body(submission, artifact, owner, *case_key));
    }
    instructions.extend(lower_move_next_done("default"));
    instructions
}

fn lower_move_next_dispatch(case_keys: &[u32]) -> Vec<JvmInstruction> {
    let mut instructions = vec![JvmInstruction::ALoad(0), JvmInstruction::IConst(0), JvmInstruction::IALoad, JvmInstruction::IStore(2)];
    for case_key in case_keys {
        let case = i32::try_from(*case_key).unwrap_or(i32::MAX);
        instructions.push(JvmInstruction::ILoad(2));
        instructions.push(JvmInstruction::IConst(case));
        instructions.push(JvmInstruction::IfICmpEq(format!("case_{case_key}")));
    }
    instructions.push(JvmInstruction::Goto("default".to_string()));
    instructions
}

fn lower_move_next_case_body(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    owner: &str,
    case_key: u32,
) -> Vec<JvmInstruction> {
    let label = format!("case_{case_key}");
    let Some(state) = resolve_state_for_case(artifact, case_key)
    else {
        return lower_move_next_done(&label);
    };
    match state.effect.as_str() {
        // Resume after a plain Yield must advance; re-running Yield would loop forever.
        "Yield" if case_key != 0 && case_key == state.resume_case_key => {
            let mut instructions = vec![JvmInstruction::Label(label)];
            instructions.extend(lower_complete_state(submission, artifact, owner, state));
            instructions
        }
        "Yield" => lower_yield_case(state, &label),
        "DelegateYield" => lower_delegate_yield_case(submission, artifact, owner, &label, state),
        "Await" if case_key != 0 && case_key == state.resume_case_key => {
            let mut instructions = vec![JvmInstruction::Label(label)];
            instructions.extend(lower_complete_state(submission, artifact, owner, state));
            instructions
        }
        "Await" => lower_await_case(submission, artifact, owner, &label, state),
        "AsyncSpawn" => lower_async_spawn_case(submission, artifact, owner, &label, state),
        "AsyncBlock" if case_key != 0 && case_key == state.resume_case_key => {
            let mut instructions = vec![JvmInstruction::Label(label)];
            instructions.extend(lower_complete_state(submission, artifact, owner, state));
            instructions
        }
        "AsyncBlock" => lower_async_block_case(submission, artifact, owner, &label, state),
        "Raise" if case_key != 0 && case_key == state.resume_case_key => {
            let mut instructions = vec![JvmInstruction::Label(label)];
            instructions.extend(lower_complete_state(submission, artifact, owner, state));
            instructions
        }
        "Raise" => lower_raise_case(state, &label),
        _ => lower_move_next_done(&label),
    }
}

fn lower_yield_case(state: &SuspendStateArtifact, label: &str) -> Vec<JvmInstruction> {
    let resume_key = i32::try_from(state.resume_case_key).unwrap_or(i32::MAX);
    vec![
        JvmInstruction::Label(label.to_string()),
        JvmInstruction::ALoad(0),
        JvmInstruction::IConst(0),
        JvmInstruction::IConst(resume_key),
        JvmInstruction::IAStore,
        JvmInstruction::IConst(1),
        JvmInstruction::IReturn,
    ]
}

/// `Raise` 挂起路径（JVM 字节码）。
///
/// 未捕获的 `raise` 在状态机层面与 `Yield` 同构：payload 已在 suspend 块体内存入
/// frame 槽位，此 case 仅需将 `state` 置为 `resume_case_key` 并返回 suspended(1)。
/// 调用方根据 `Effectful::Resume` 关联类型决定是否 resume；`Resume = !` 时 resume 路径
/// 不可达，`resume_case_key` 分支仅作结构占位。
fn lower_raise_case(state: &SuspendStateArtifact, label: &str) -> Vec<JvmInstruction> {
    lower_yield_case(state, label)
}

fn lower_delegate_yield_case(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    owner: &str,
    label: &str,
    state: &SuspendStateArtifact,
) -> Vec<JvmInstruction> {
    let Some(binding) = primary_witness_binding(state)
    else {
        return lower_yield_case(state, label);
    };
    let Some(slot) = resolve_witness_slot(submission, binding)
    else {
        return lower_yield_case(state, label);
    };
    let exhausted_label = format!("{label}_exhausted");
    let mut instructions = vec![JvmInstruction::Label(label.to_string())];
    instructions.extend(emit_witness_receiver_load(artifact, state));
    instructions.push(JvmInstruction::InvokeStatic(JvmMethodRef {
        owner: owner.to_string(),
        name: slot.impl_symbol.clone(),
        descriptor: JvmMethodDescriptor::new(
            vec![JvmTypeDescriptor::Object(JVM_OBJECT.to_string())],
            JvmTypeDescriptor::Object(JVM_OBJECT.to_string()),
        ),
    }));
    // Store first: ifnull pops the reference, so a post-branch astore would see an empty stack.
    instructions.push(JvmInstruction::AStore(3));
    instructions.push(JvmInstruction::ALoad(3));
    instructions.push(JvmInstruction::IfNull(exhausted_label.clone()));
    instructions.push(JvmInstruction::ALoad(1));
    instructions.push(JvmInstruction::IConst(1));
    instructions.push(JvmInstruction::ALoad(3));
    instructions.push(JvmInstruction::AAStore);
    instructions.extend(lower_yield_case(state, &format!("{label}_yield")));
    instructions.push(JvmInstruction::Label(exhausted_label));
    instructions.extend(lower_complete_state(submission, artifact, owner, state));
    instructions
}

fn lower_await_case(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    owner: &str,
    label: &str,
    state: &SuspendStateArtifact,
) -> Vec<JvmInstruction> {
    let Some(binding) = primary_witness_binding(state)
    else {
        return lower_move_next_done(label);
    };
    let Some(slot) = resolve_witness_slot(submission, binding)
    else {
        return lower_move_next_done(label);
    };
    let suspend_label = format!("{label}_suspend");
    let complete_label = format!("{label}_complete");
    let mut instructions = vec![JvmInstruction::Label(label.to_string())];
    instructions.extend(emit_witness_receiver_load(artifact, state));
    instructions.push(JvmInstruction::InvokeStatic(JvmMethodRef {
        owner: owner.to_string(),
        name: slot.impl_symbol.clone(),
        descriptor: JvmMethodDescriptor::new(vec![JvmTypeDescriptor::Object(JVM_OBJECT.to_string())], JvmTypeDescriptor::Boolean),
    }));
    instructions.push(JvmInstruction::IfEq(suspend_label.clone()));
    instructions.extend(emit_jvm_cancel_check_or_output(submission, artifact, owner, state, &format!("{label}_cancel")));
    instructions.push(JvmInstruction::Goto(complete_label.clone()));
    instructions.push(JvmInstruction::Label(suspend_label));
    instructions.extend(lower_yield_case(state, &format!("{label}_yield")));
    instructions.push(JvmInstruction::Label(complete_label));
    instructions.extend(lower_complete_state(submission, artifact, owner, state));
    instructions
}

/// 在 `Future::poll` 返回 ready 后调用 `Future::output` 取出恢复值 `T`，并写入 frame 数组的索引 1。
///
/// spec Task 3.2 要求后端在 `poll` 返回 true 后显式调用 `output` 获取 `T`。JVM 后端没有
/// `__current` 字段，而是通过 frame 数组（`ALoad(1)`）的 slot 1 携带 yielded/resume 值
/// （与 `lower_delegate_yield_case` 的存储方式一致）。该函数通过 [`secondary_witness_binding`]
/// 获取 `output` 绑定并解析槽位，发射 `InvokeStatic output` → `AStore(3)`（暂存）→
/// `ALoad(1)` / `IConst(1)` / `ALoad(3)` / `AAStore`（`frame[1] = T`）的序列。
///
/// 当 `secondary_witness_binding` 返回 `None`（旧单绑定工件）或槽位无法解析时返回空 `Vec`，
/// 后端回退到旧有行为（不调用 `output`），保持向后兼容。
fn emit_jvm_output_call_and_store(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    owner: &str,
    state: &SuspendStateArtifact,
) -> Vec<JvmInstruction> {
    let Some(binding) = secondary_witness_binding(state)
    else {
        return Vec::new();
    };
    let Some(slot) = resolve_witness_slot(submission, binding)
    else {
        return Vec::new();
    };
    let mut instructions = emit_witness_receiver_load(artifact, state);
    instructions.push(JvmInstruction::InvokeStatic(JvmMethodRef {
        owner: owner.to_string(),
        name: slot.impl_symbol.clone(),
        descriptor: JvmMethodDescriptor::new(
            vec![JvmTypeDescriptor::Object(JVM_OBJECT.to_string())],
            JvmTypeDescriptor::Object(JVM_OBJECT.to_string()),
        ),
    }));
    instructions.push(JvmInstruction::AStore(3));
    instructions.push(JvmInstruction::ALoad(1));
    instructions.push(JvmInstruction::IConst(1));
    instructions.push(JvmInstruction::ALoad(3));
    instructions.push(JvmInstruction::AAStore);
    instructions
}

/// spec Task 5.2：在 `Future::poll` 返回 true 后，先检查 `is_cancelled`，若被取消则跳过 `output` 调用，
/// 以 null 作为恢复值写入 frame 数组 slot 1，直接跳过 output；否则执行原有 `output` 调用取出 `T`。
///
/// 该辅助在 [`emit_jvm_output_call_and_store`] 前后包裹可选的 cancel 检查：
/// - 当 [`tertiary_witness_binding`] 返回 `None`（impl 未声明 `is_cancelled`）或槽位无法解析时，
///   仅返回 [`emit_jvm_output_call_and_store`] 的结果，行为与 Task 3.2 完全一致，保持向后兼容；
/// - 当存在第三条 witness 绑定且槽位可解析时，发射
///   `InvokeStatic is_cancelled → IfEq not_cancelled → AConstNull → AStore(3) → ALoad(1) → IConst(1) →
///   ALoad(3) → AAStore → Goto skip_output → not_cancelled: → output 调用 → skip_output:` 的序列。
///
/// `label_prefix` 用于生成本次 cancel 检查内唯一的分支标签，调用方需保证在同函数内不冲突。
fn emit_jvm_cancel_check_or_output(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    owner: &str,
    state: &SuspendStateArtifact,
    label_prefix: &str,
) -> Vec<JvmInstruction> {
    let cancel_slot = tertiary_witness_binding(state).and_then(|binding| resolve_witness_slot(submission, binding));
    let mut instructions = Vec::new();
    if let Some(slot) = cancel_slot {
        let not_cancelled_label = format!("{label_prefix}_not_cancelled");
        let skip_output_label = format!("{label_prefix}_skip_output");
        instructions.extend(emit_witness_receiver_load(artifact, state));
        instructions.push(JvmInstruction::InvokeStatic(JvmMethodRef {
            owner: owner.to_string(),
            name: slot.impl_symbol.clone(),
            descriptor: JvmMethodDescriptor::new(vec![JvmTypeDescriptor::Object(JVM_OBJECT.to_string())], JvmTypeDescriptor::Boolean),
        }));
        instructions.push(JvmInstruction::IfEq(not_cancelled_label.clone()));
        instructions.push(JvmInstruction::AConstNull);
        instructions.push(JvmInstruction::AStore(3));
        instructions.push(JvmInstruction::ALoad(1));
        instructions.push(JvmInstruction::IConst(1));
        instructions.push(JvmInstruction::ALoad(3));
        instructions.push(JvmInstruction::AAStore);
        instructions.push(JvmInstruction::Goto(skip_output_label.clone()));
        instructions.push(JvmInstruction::Label(not_cancelled_label));
        instructions.extend(emit_jvm_output_call_and_store(submission, artifact, owner, state));
        instructions.push(JvmInstruction::Label(skip_output_label));
    }
    else {
        instructions.extend(emit_jvm_output_call_and_store(submission, artifact, owner, state));
    }
    instructions
}

fn lower_async_spawn_case(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    owner: &str,
    label: &str,
    state: &SuspendStateArtifact,
) -> Vec<JvmInstruction> {
    let mut instructions = vec![JvmInstruction::Label(label.to_string())];
    if let Some(binding) = primary_witness_binding(state) {
        if let Some(slot) = resolve_witness_slot(submission, binding) {
            instructions.extend(emit_witness_receiver_load(artifact, state));
            instructions.push(JvmInstruction::InvokeStatic(JvmMethodRef {
                owner: owner.to_string(),
                name: slot.impl_symbol.clone(),
                descriptor: JvmMethodDescriptor::new(vec![JvmTypeDescriptor::Object(JVM_OBJECT.to_string())], JvmTypeDescriptor::Void),
            }));
        }
    }
    instructions.extend(lower_complete_state(submission, artifact, owner, state));
    instructions
}

fn lower_async_block_case(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    owner: &str,
    label: &str,
    state: &SuspendStateArtifact,
) -> Vec<JvmInstruction> {
    let Some(binding) = primary_witness_binding(state)
    else {
        return lower_move_next_done(label);
    };
    let Some(slot) = resolve_witness_slot(submission, binding)
    else {
        return lower_move_next_done(label);
    };
    let poll_label = format!("{label}_poll");
    let mut instructions = vec![JvmInstruction::Label(label.to_string()), JvmInstruction::Label(poll_label.clone())];
    instructions.extend(emit_witness_receiver_load(artifact, state));
    instructions.push(JvmInstruction::InvokeStatic(JvmMethodRef {
        owner: owner.to_string(),
        name: slot.impl_symbol.clone(),
        descriptor: JvmMethodDescriptor::new(vec![JvmTypeDescriptor::Object(JVM_OBJECT.to_string())], JvmTypeDescriptor::Boolean),
    }));
    instructions.push(JvmInstruction::IfEq(poll_label));
    instructions.extend(emit_jvm_cancel_check_or_output(submission, artifact, owner, state, &format!("{label}_cancel")));
    instructions.extend(lower_complete_state(submission, artifact, owner, state));
    instructions
}

fn lower_complete_state(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    owner: &str,
    state: &SuspendStateArtifact,
) -> Vec<JvmInstruction> {
    if let Some(next) = artifact.states.iter().find(|candidate| candidate.state_id == state.state_id + 1) {
        return lower_effect_for_state(submission, artifact, owner, next);
    }
    lower_move_next_done("")
}

fn lower_effect_for_state(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    owner: &str,
    state: &SuspendStateArtifact,
) -> Vec<JvmInstruction> {
    match state.effect.as_str() {
        "Yield" => lower_yield_case(state, &format!("next_{}", state.state_id)),
        "DelegateYield" => lower_delegate_yield_case(submission, artifact, owner, &format!("next_{}", state.state_id), state),
        "Await" => lower_await_case(submission, artifact, owner, &format!("next_{}", state.state_id), state),
        "AsyncSpawn" => lower_async_spawn_case(submission, artifact, owner, &format!("next_{}", state.state_id), state),
        "AsyncBlock" => lower_async_block_case(submission, artifact, owner, &format!("next_{}", state.state_id), state),
        "Raise" => lower_raise_case(state, &format!("next_{}", state.state_id)),
        _ => lower_move_next_done(&format!("next_{}", state.state_id)),
    }
}

fn emit_witness_receiver_load(artifact: &SuspendFunctionArtifact, state: &SuspendStateArtifact) -> Vec<JvmInstruction> {
    let Some(field) = witness_receiver_field(state).filter(|field| frame_has_field(artifact, field))
    else {
        return vec![JvmInstruction::AConstNull];
    };
    let _ = field;
    vec![JvmInstruction::ALoad(1), JvmInstruction::IConst(0), JvmInstruction::AALoad]
}

fn lower_move_next_done(label: &str) -> Vec<JvmInstruction> {
    let mut instructions = Vec::new();
    if !label.is_empty() {
        instructions.push(JvmInstruction::Label(label.to_string()));
    }
    instructions.extend([JvmInstruction::IConst(0), JvmInstruction::IReturn]);
    instructions
}

fn jvm_run_loop_instructions(move_next_name: &str, owner: &str, frame_slots: i32, artifact: &SuspendFunctionArtifact) -> Vec<JvmInstruction> {
    let mut instructions = vec![
        JvmInstruction::IConst(1),
        JvmInstruction::NewIntArray,
        JvmInstruction::Dup,
        JvmInstruction::IConst(0),
        JvmInstruction::IConst(0),
        JvmInstruction::IAStore,
        JvmInstruction::AStore(1),
        JvmInstruction::IConst(frame_slots),
        JvmInstruction::ANewArray(JVM_OBJECT.to_string()),
        JvmInstruction::AStore(2),
    ];
    if artifact.frame_fields.iter().any(|field| field.starts_with("__witness_payload_") || field.starts_with("slot_")) {
        instructions.extend(init_witness_payload_slot());
    }
    instructions.extend([
        JvmInstruction::Label("loop".to_string()),
        JvmInstruction::ALoad(1),
        JvmInstruction::ALoad(2),
        JvmInstruction::InvokeStatic(JvmMethodRef {
            owner: owner.to_string(),
            name: move_next_name.to_string(),
            descriptor: move_next_descriptor(),
        }),
        JvmInstruction::IfEq("exit".to_string()),
        JvmInstruction::Goto("loop".to_string()),
        JvmInstruction::Label("exit".to_string()),
        JvmInstruction::IConst(0),
        JvmInstruction::IReturn,
    ]);
    instructions
}

fn init_witness_payload_slot() -> Vec<JvmInstruction> {
    vec![
        JvmInstruction::ALoad(2),
        JvmInstruction::IConst(0),
        JvmInstruction::IConst(1),
        JvmInstruction::NewIntArray,
        JvmInstruction::Dup,
        JvmInstruction::IConst(0),
        JvmInstruction::IConst(0),
        JvmInstruction::IAStore,
        JvmInstruction::AAStore,
    ]
}

/// 生成入口 wrapper 方法指令。
///
/// `sm_{symbol}_run` 恒返回 `int`（退出码），但 wrapper 的返回类型必须匹配 MIR 函数
/// 的返回类型，使调用方能以正确描述符 `InvokeStatic`。void 返回时 `Pop` 丢弃退出码后
/// `Return`；int 返回时直接 `IReturn`；其他类型返回默认零值（`sm_run` 的退出码不代表
/// 函数返回值，实际返回值通过 frame 数组传递，此处仅保证类型一致通过验证）。
fn jvm_entry_wrapper_instructions(run_name: &str, owner: &str, return_descriptor: &JvmTypeDescriptor) -> Vec<JvmInstruction> {
    let mut instructions = vec![JvmInstruction::InvokeStatic(JvmMethodRef {
        owner: owner.to_string(),
        name: run_name.to_string(),
        descriptor: JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Int),
    })];
    match return_descriptor {
        JvmTypeDescriptor::Void => {
            instructions.push(JvmInstruction::Pop);
            instructions.push(JvmInstruction::Return);
        }
        JvmTypeDescriptor::Int | JvmTypeDescriptor::Boolean | JvmTypeDescriptor::Byte | JvmTypeDescriptor::Short | JvmTypeDescriptor::Char => {
            instructions.push(JvmInstruction::IReturn);
        }
        JvmTypeDescriptor::Long => {
            instructions.push(JvmInstruction::I2L);
            instructions.push(JvmInstruction::LReturn);
        }
        JvmTypeDescriptor::Double => {
            instructions.push(JvmInstruction::I2D);
            instructions.push(JvmInstruction::DReturn);
        }
        JvmTypeDescriptor::Float => {
            instructions.push(JvmInstruction::Pop);
            instructions.push(JvmInstruction::FConst0);
            instructions.push(JvmInstruction::FReturn);
        }
        JvmTypeDescriptor::Object(_) | JvmTypeDescriptor::Array(_) => {
            instructions.push(JvmInstruction::Pop);
            instructions.push(JvmInstruction::AConstNull);
            instructions.push(JvmInstruction::AReturn);
        }
    }
    instructions
}

pub(crate) fn lower_suspend_main_entry(submission: &FragmentSubmission) -> Option<Vec<JvmInstruction>> {
    let entry = submission.entry_operation.as_ref()?;
    let payload = submission.control_flow.as_ref()?;
    let artifact = payload.functions.iter().find(|function| &function.symbol == entry)?;
    // catch+resume 等被优化为直接线性流的场景，MIR 不含 StateDispatch/YieldToRuntime，
    // 此时不应走状态机包装入口（其恒返回 0），而应回退到直接调用入口方法。
    let entry_has_state_machine = submission
        .executable
        .as_ref()
        .and_then(|exec| exec.get_function(entry))
        .map(|view| mir_has_state_machine(&view.function))
        .unwrap_or(false);
    if !entry_has_state_machine {
        return None;
    }
    let symbol = sanitize_jvm_method_symbol(&artifact.symbol);
    Some(jvm_entry_wrapper_instructions(&format!("sm_{symbol}_run"), &class_owner(submission), &JvmTypeDescriptor::Int))
}
