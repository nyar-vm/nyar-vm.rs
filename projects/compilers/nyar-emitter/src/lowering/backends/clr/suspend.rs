//! CLR state-machine augmentation for `control_flow` payloads.

use crate::nyar_backend_clr::{
    MsilInstruction, MsilInstructionOperand, MsilMethodBody, MsilMethodRef, MsilMethodSignature, MsilModule, MsilOpcode, MsilType, MsilTypeDef,
};
use nyar::{Identifier, QualifiedName, SuspendFunctionArtifact, SuspendStateArtifact};
use std_data::text::msil::MsilField;

use super::{
    executable::executable_has_state_machine as mir_has_state_machine,
    sanitize_operation_symbol, sanitize_symbol,
    suspend_sm::{dispatch_case_keys, resolve_state_for_case},
    suspend_witness::{
        WitnessSlot, frame_has_field, primary_witness_binding, resolve_witness_slot, secondary_witness_binding, tertiary_witness_binding,
        witness_receiver_field,
    },
};
use crate::FragmentSubmission;

/// 优先解析真实 Valkyrie 方法符号 `{type_name}_{method_name}`。
///
/// 当真实方法在 executable 中存在时返回其 MSIL 符号；否则返回 `impl_symbol`——
/// 由于本模块不再生成 `witness_*` 桩，`reject_unresolved_local_calls` 会将其
/// 作为未解析本地调用报编译错误，绝不在 Rust 端写一份返回默认值的 mock。
fn real_witness_call_target(submission: &FragmentSubmission, slot: &WitnessSlot) -> String {
    let real_operation = QualifiedName::new(vec![Identifier::new(&slot.type_name), Identifier::new(&slot.method_name)]);
    if let Some(exec) = &submission.executable {
        if exec.get_function(&real_operation).is_some() {
            return sanitize_operation_symbol(&real_operation);
        }
    }
    slot.impl_symbol.clone()
}

pub(crate) fn augment_msil_with_suspend(submission: &FragmentSubmission, module: &mut MsilModule) {
    let Some(payload) = &submission.control_flow
    else {
        return;
    };
    let namespace = sanitize_symbol(&submission.module_name);
    for artifact in &payload.functions {
        let has_state_machine = submission
            .executable
            .as_ref()
            .and_then(|exec| exec.get_function(&artifact.symbol))
            .map(|view| mir_has_state_machine(&view.function))
            .unwrap_or(false);
        module.types.push(build_state_machine_type(submission, artifact, &namespace));
        if has_state_machine && submission.exported_operations.iter().any(|op| op == &artifact.symbol) {
            replace_operation_method(module, artifact, &namespace);
        }
    }
    if let Some(entry) = &submission.entry_operation {
        if payload.functions.iter().any(|f| &f.symbol == entry) {
            let entry_has_state_machine = submission
                .executable
                .as_ref()
                .and_then(|exec| exec.get_function(entry))
                .map(|view| mir_has_state_machine(&view.function))
                .unwrap_or(false);
            if entry_has_state_machine {
                replace_entry_with_suspend_call(module, entry);
            }
        }
    }
}

fn build_state_machine_type(submission: &FragmentSubmission, artifact: &SuspendFunctionArtifact, namespace: &str) -> MsilTypeDef {
    let qualified = qualified_state_machine_type(artifact, namespace);
    let mut fields = vec![
        MsilField { name: artifact.state_field.clone(), ty: MsilType::Int32 { signed: true }, is_static: false },
        MsilField { name: "__current".to_string(), ty: MsilType::Object, is_static: false },
    ];
    for field in &artifact.frame_fields {
        if !fields.iter().any(|existing| existing.name == *field) {
            fields.push(MsilField { name: field.clone(), ty: MsilType::Object, is_static: false });
        }
    }
    for state in &artifact.states {
        for field in &state.spill_fields {
            if !fields.iter().any(|existing| existing.name == *field) {
                fields.push(MsilField { name: field.clone(), ty: MsilType::Object, is_static: false });
            }
        }
    }
    MsilTypeDef {
        full_name: artifact.state_machine_type.clone(),
        namespace: namespace.to_string(),
        fields,
        methods: vec![
            lower_state_machine_ctor(artifact, &qualified),
            lower_move_next_method(submission, artifact, &qualified),
            lower_run_wrapper(artifact, &qualified),
        ],
        is_value_type: false,
    }
}

fn lower_state_machine_ctor(artifact: &SuspendFunctionArtifact, qualified_type: &str) -> MsilMethodBody {
    let mut instructions = vec![
        MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::Call, operand: Some(MsilInstructionOperand::Method(object_ctor_ref())) },
        MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None },
        MsilInstruction {
            label: None,
            opcode: MsilOpcode::Stfld,
            operand: Some(MsilInstructionOperand::Field(qualified_type.to_string(), artifact.state_field.clone())),
        },
    ];
    for field in witness_spill_field_names(artifact) {
        instructions.extend(emit_witness_payload_placeholder_store(qualified_type, &field));
    }
    instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None });
    MsilMethodBody {
        method: MsilMethodRef {
            owner: Some(artifact.state_machine_type.clone()),
            name: ".ctor".to_string(),
            signature: MsilMethodSignature::new(MsilType::Void, Vec::new()),
        },
        locals: Vec::new(),
        instructions,
        max_stack: 3,
        is_entry_point: false,
        is_async: false,
    }
}

fn witness_spill_field_names(artifact: &SuspendFunctionArtifact) -> Vec<String> {
    let mut fields = artifact.frame_fields.clone();
    for state in &artifact.states {
        for field in &state.spill_fields {
            if !fields.iter().any(|existing| existing == field) {
                fields.push(field.clone());
            }
        }
    }
    fields.into_iter().filter(|field| field.starts_with("__witness_payload_") || field.starts_with("slot_")).collect()
}

fn emit_witness_payload_placeholder_store(qualified_type: &str, field: &str) -> Vec<MsilInstruction> {
    vec![
        MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_1, operand: None },
        MsilInstruction {
            label: None,
            opcode: MsilOpcode::Newarr,
            operand: Some(MsilInstructionOperand::Type("[mscorlib]System.Int32".to_string())),
        },
        MsilInstruction {
            label: None,
            opcode: MsilOpcode::Stfld,
            operand: Some(MsilInstructionOperand::Field(qualified_type.to_string(), field.to_string())),
        },
    ]
}

fn lower_move_next_method(submission: &FragmentSubmission, artifact: &SuspendFunctionArtifact, qualified_type: &str) -> MsilMethodBody {
    let case_keys = dispatch_case_keys(artifact);
    let mut instructions = lower_move_next_dispatch(artifact, qualified_type, &case_keys);
    for case_key in &case_keys {
        instructions.extend(lower_move_next_case_body(submission, artifact, qualified_type, *case_key));
    }
    instructions.extend(lower_move_next_done("default"));
    MsilMethodBody {
        method: MsilMethodRef {
            owner: Some(artifact.state_machine_type.clone()),
            name: "MoveNext".to_string(),
            signature: MsilMethodSignature::new(MsilType::Bool, Vec::new()),
        },
        locals: vec![MsilType::Int32 { signed: true }, MsilType::Object],
        instructions,
        max_stack: 16,
        is_entry_point: false,
        is_async: false,
    }
}

fn lower_move_next_dispatch(artifact: &SuspendFunctionArtifact, qualified_type: &str, case_keys: &[u32]) -> Vec<MsilInstruction> {
    let mut instructions = vec![
        MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
        MsilInstruction {
            label: None,
            opcode: MsilOpcode::Ldfld,
            operand: Some(MsilInstructionOperand::Field(qualified_type.to_string(), artifact.state_field.clone())),
        },
        MsilInstruction { label: None, opcode: MsilOpcode::Stloc0, operand: None },
    ];
    for case_key in case_keys {
        let case = *case_key as usize;
        instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ldloc0, operand: None });
        if case <= 8 {
            instructions.push(MsilInstruction {
                label: None,
                opcode: match case {
                    0 => MsilOpcode::LdcI4_0,
                    1 => MsilOpcode::LdcI4_1,
                    2 => MsilOpcode::LdcI4_2,
                    3 => MsilOpcode::LdcI4_3,
                    4 => MsilOpcode::LdcI4_4,
                    5 => MsilOpcode::LdcI4_5,
                    6 => MsilOpcode::LdcI4_6,
                    7 => MsilOpcode::LdcI4_7,
                    8 => MsilOpcode::LdcI4_8,
                    _ => unreachable!(),
                },
                operand: None,
            });
        }
        else {
            instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::LdcI4,
                operand: Some(MsilInstructionOperand::Integer(case as i64)),
            });
        }
        instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Beq,
            operand: Some(MsilInstructionOperand::BranchTarget(format!("case_{case_key}"))),
        });
    }
    instructions.push(MsilInstruction {
        label: None,
        opcode: MsilOpcode::Br,
        operand: Some(MsilInstructionOperand::BranchTarget("default".to_string())),
    });
    instructions
}

fn lower_move_next_case_body(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    qualified_type: &str,
    case_key: u32,
) -> Vec<MsilInstruction> {
    let label = format!("case_{case_key}");
    let Some(state) = resolve_state_for_case(artifact, case_key)
    else {
        return lower_move_next_done(&label);
    };
    match state.effect.as_str() {
        // Resume after a plain Yield must advance; re-running Yield would loop forever.
        "Yield" if case_key != 0 && case_key == state.resume_case_key => {
            let mut instructions = vec![MsilInstruction { label: Some(label), opcode: MsilOpcode::Nop, operand: None }];
            instructions.extend(lower_complete_state(submission, artifact, qualified_type, state));
            instructions
        }
        "Yield" => lower_yield_case(artifact, qualified_type, &label, state),
        "DelegateYield" => lower_delegate_yield_case(submission, artifact, qualified_type, &label, state),
        "Await" => lower_await_case(submission, artifact, qualified_type, &label, state),
        "AsyncSpawn" => lower_async_spawn_case(submission, artifact, qualified_type, &label, state),
        "AsyncBlock" => lower_async_block_case(submission, artifact, qualified_type, &label, state),
        "Raise" if case_key != 0 && case_key == state.resume_case_key => {
            let mut instructions = vec![MsilInstruction { label: Some(label), opcode: MsilOpcode::Nop, operand: None }];
            instructions.extend(lower_complete_state(submission, artifact, qualified_type, state));
            instructions
        }
        "Raise" => lower_raise_case(artifact, qualified_type, &label, state),
        _ => lower_move_next_done(&label),
    }
}

fn lower_yield_case(
    artifact: &SuspendFunctionArtifact,
    qualified_type: &str,
    label: &str,
    state: &SuspendStateArtifact,
) -> Vec<MsilInstruction> {
    vec![
        MsilInstruction { label: Some(label.to_string()), opcode: MsilOpcode::Ldarg0, operand: None },
        MsilInstruction {
            label: None,
            opcode: MsilOpcode::LdcI4,
            operand: Some(MsilInstructionOperand::Integer(state.resume_case_key as i64)),
        },
        MsilInstruction {
            label: None,
            opcode: MsilOpcode::Stfld,
            operand: Some(MsilInstructionOperand::Field(qualified_type.to_string(), artifact.state_field.clone())),
        },
        MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_1, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
    ]
}

/// `Raise` 挂起路径（CLR / MSIL）。
///
/// 未捕获的 `raise` 在状态机层面与 `Yield` 同构：payload 已在 suspend 块体内存入
/// frame 槽位，此 case 仅需将 `state_field` 置为 `resume_case_key` 并返回 suspended(1)。
/// 调用方根据 `Effectful::Resume` 关联类型决定是否 resume；`Resume = !` 时 resume 路径
/// 不可达，`resume_case_key` 分支仅作结构占位。
fn lower_raise_case(
    artifact: &SuspendFunctionArtifact,
    qualified_type: &str,
    label: &str,
    state: &SuspendStateArtifact,
) -> Vec<MsilInstruction> {
    lower_yield_case(artifact, qualified_type, label, state)
}

fn lower_delegate_yield_case(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    qualified_type: &str,
    label: &str,
    state: &SuspendStateArtifact,
) -> Vec<MsilInstruction> {
    let Some(binding) = primary_witness_binding(state)
    else {
        return lower_yield_case(artifact, qualified_type, label, state);
    };
    let Some(slot) = resolve_witness_slot(submission, binding)
    else {
        return lower_yield_case(artifact, qualified_type, label, state);
    };
    let exhausted_label = format!("{label}_exhausted");
    let yield_label = format!("{label}_yield");
    let mut instructions = vec![MsilInstruction { label: Some(label.to_string()), opcode: MsilOpcode::Nop, operand: None }];
    instructions.extend(emit_witness_receiver_load(qualified_type, artifact, state));
    instructions.push(MsilInstruction {
        label: None,
        opcode: MsilOpcode::Call,
        operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
            owner: None,
            name: real_witness_call_target(submission, &slot),
            signature: MsilMethodSignature::new(MsilType::Object, vec![MsilType::Object]),
        })),
    });
    instructions.push(MsilInstruction {
        label: None,
        opcode: MsilOpcode::Brfalse,
        operand: Some(MsilInstructionOperand::BranchTarget(exhausted_label.clone())),
    });
    instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Stloc1, operand: None });
    instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None });
    instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ldloc1, operand: None });
    instructions.push(MsilInstruction {
        label: None,
        opcode: MsilOpcode::Stfld,
        operand: Some(MsilInstructionOperand::Field(qualified_type.to_string(), "__current".to_string())),
    });
    instructions.push(MsilInstruction { label: Some(yield_label), opcode: MsilOpcode::Nop, operand: None });
    instructions.extend(lower_yield_case(artifact, qualified_type, &format!("{label}_resume"), state));
    instructions.push(MsilInstruction { label: Some(exhausted_label), opcode: MsilOpcode::Nop, operand: None });
    instructions.extend(lower_complete_state(submission, artifact, qualified_type, state));
    instructions
}

fn lower_await_case(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    qualified_type: &str,
    label: &str,
    state: &SuspendStateArtifact,
) -> Vec<MsilInstruction> {
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
    let mut instructions = vec![MsilInstruction { label: Some(label.to_string()), opcode: MsilOpcode::Nop, operand: None }];
    instructions.extend(emit_witness_receiver_load(qualified_type, artifact, state));
    instructions.push(MsilInstruction {
        label: None,
        opcode: MsilOpcode::Call,
        operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
            owner: None,
            name: real_witness_call_target(submission, &slot),
            signature: MsilMethodSignature::new(MsilType::Bool, vec![MsilType::Object]),
        })),
    });
    instructions.push(MsilInstruction {
        label: None,
        opcode: MsilOpcode::Brfalse,
        operand: Some(MsilInstructionOperand::BranchTarget(suspend_label.clone())),
    });
    instructions.extend(emit_cancel_check_or_output_call(submission, artifact, qualified_type, state, &format!("{label}_cancel")));
    instructions.push(MsilInstruction {
        label: None,
        opcode: MsilOpcode::Br,
        operand: Some(MsilInstructionOperand::BranchTarget(complete_label.clone())),
    });
    instructions.push(MsilInstruction { label: Some(suspend_label), opcode: MsilOpcode::Nop, operand: None });
    instructions.extend(lower_yield_case(artifact, qualified_type, &format!("{label}_yield"), state));
    instructions.push(MsilInstruction { label: Some(complete_label), opcode: MsilOpcode::Nop, operand: None });
    instructions.extend(lower_complete_state(submission, artifact, qualified_type, state));
    instructions
}

/// 在 `Future::poll` 返回 ready 后，调用 `Future::output` 取出恢复值 `T` 并存入 `__current` 字段。
///
/// spec Task 3.2 要求后端在 `poll` 返回 true 后显式调用 `output` 获取 `T`，而不是仅靠
/// 布尔分支脑补输出值。该函数通过 [`secondary_witness_binding`] 获取 `output` 绑定，
/// 解析槽位后发射 `Call output` → `Stloc1`（暂存）→ `Ldarg0`（this）→ `Ldloc1` → `Stfld __current`
/// 的 MSIL 序列，使后续 resume 路径能从 `__current` 读取 `T`。
///
/// 当 `secondary_witness_binding` 返回 `None`（旧单绑定工件）或 `output` 槽位无法解析时，
/// 返回空 `Vec`，后端回退到旧有行为（不调用 `output`，直接进入 complete），保持向后兼容。
fn emit_output_call_and_store_current(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    qualified_type: &str,
    state: &SuspendStateArtifact,
) -> Vec<MsilInstruction> {
    let Some(binding) = secondary_witness_binding(state)
    else {
        return Vec::new();
    };
    let Some(slot) = resolve_witness_slot(submission, binding)
    else {
        return Vec::new();
    };
    let mut instructions = emit_witness_receiver_load(qualified_type, artifact, state);
    instructions.push(MsilInstruction {
        label: None,
        opcode: MsilOpcode::Call,
        operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
            owner: None,
            name: real_witness_call_target(submission, &slot),
            signature: MsilMethodSignature::new(MsilType::Object, vec![MsilType::Object]),
        })),
    });
    instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Stloc1, operand: None });
    instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None });
    instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ldloc1, operand: None });
    instructions.push(MsilInstruction {
        label: None,
        opcode: MsilOpcode::Stfld,
        operand: Some(MsilInstructionOperand::Field(qualified_type.to_string(), "__current".to_string())),
    });
    instructions
}

/// spec Task 5.2：在 `Future::poll` 返回 true 后，先检查 `is_cancelled`，若被取消则跳过 `output` 调用，
/// 以 null 作为恢复值，直接跳转到 output 之后的 complete 路径；否则执行原有 `output` 调用取出 `T`。
///
/// 该辅助在 [`emit_output_call_and_store_current`] 前后包裹可选的 cancel 检查：
/// - 当 [`tertiary_witness_binding`] 返回 `None`（impl 未声明 `is_cancelled`）或槽位无法解析时，
///   仅返回 [`emit_output_call_and_store_current`] 的结果，行为与 Task 3.2 完全一致，保持向后兼容；
/// - 当存在第三条 witness 绑定且槽位可解析时，发射
///   `Call is_cancelled → Brfalse not_cancelled → Ldarg0 → Ldnull → Stfld __current → Br skip_output →
///   not_cancelled: → output 调用 → skip_output:` 的序列。
///
/// `label_prefix` 用于生成本次 cancel 检查内唯一的分支标签，调用方需保证在同函数内不冲突。
fn emit_cancel_check_or_output_call(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    qualified_type: &str,
    state: &SuspendStateArtifact,
    label_prefix: &str,
) -> Vec<MsilInstruction> {
    let cancel_slot = tertiary_witness_binding(state).and_then(|binding| resolve_witness_slot(submission, binding));
    let mut instructions = Vec::new();
    if let Some(slot) = cancel_slot {
        let not_cancelled_label = format!("{label_prefix}_not_cancelled");
        let skip_output_label = format!("{label_prefix}_skip_output");
        instructions.extend(emit_witness_receiver_load(qualified_type, artifact, state));
        instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Call,
            operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                owner: None,
                name: real_witness_call_target(submission, &slot),
                signature: MsilMethodSignature::new(MsilType::Bool, vec![MsilType::Object]),
            })),
        });
        instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Brfalse,
            operand: Some(MsilInstructionOperand::BranchTarget(not_cancelled_label.clone())),
        });
        instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None });
        instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ldnull, operand: None });
        instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Stfld,
            operand: Some(MsilInstructionOperand::Field(qualified_type.to_string(), "__current".to_string())),
        });
        instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Br,
            operand: Some(MsilInstructionOperand::BranchTarget(skip_output_label.clone())),
        });
        instructions.push(MsilInstruction { label: Some(not_cancelled_label), opcode: MsilOpcode::Nop, operand: None });
        instructions.extend(emit_output_call_and_store_current(submission, artifact, qualified_type, state));
        instructions.push(MsilInstruction { label: Some(skip_output_label), opcode: MsilOpcode::Nop, operand: None });
    }
    else {
        instructions.extend(emit_output_call_and_store_current(submission, artifact, qualified_type, state));
    }
    instructions
}

fn lower_async_spawn_case(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    qualified_type: &str,
    label: &str,
    state: &SuspendStateArtifact,
) -> Vec<MsilInstruction> {
    let mut instructions = vec![MsilInstruction { label: Some(label.to_string()), opcode: MsilOpcode::Nop, operand: None }];
    if let Some(binding) = primary_witness_binding(state) {
        if let Some(slot) = resolve_witness_slot(submission, binding) {
            instructions.extend(emit_witness_receiver_load(qualified_type, artifact, state));
            instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Call,
                operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                    owner: None,
                    name: real_witness_call_target(submission, &slot),
                    signature: MsilMethodSignature::new(MsilType::Void, vec![MsilType::Object]),
                })),
            });
        }
    }
    instructions.extend(lower_complete_state(submission, artifact, qualified_type, state));
    instructions
}

fn lower_async_block_case(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    qualified_type: &str,
    label: &str,
    state: &SuspendStateArtifact,
) -> Vec<MsilInstruction> {
    let Some(binding) = primary_witness_binding(state)
    else {
        return lower_move_next_done(label);
    };
    let Some(slot) = resolve_witness_slot(submission, binding)
    else {
        return lower_move_next_done(label);
    };
    let poll_label = format!("{label}_poll");
    let mut instructions = vec![MsilInstruction { label: Some(label.to_string()), opcode: MsilOpcode::Nop, operand: None }];
    instructions.push(MsilInstruction { label: Some(poll_label.clone()), opcode: MsilOpcode::Nop, operand: None });
    instructions.extend(emit_witness_receiver_load(qualified_type, artifact, state));
    instructions.push(MsilInstruction {
        label: None,
        opcode: MsilOpcode::Call,
        operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
            owner: None,
            name: real_witness_call_target(submission, &slot),
            signature: MsilMethodSignature::new(MsilType::Bool, vec![MsilType::Object]),
        })),
    });
    instructions.push(MsilInstruction {
        label: None,
        opcode: MsilOpcode::Brfalse,
        operand: Some(MsilInstructionOperand::BranchTarget(poll_label)),
    });
    instructions.extend(emit_cancel_check_or_output_call(submission, artifact, qualified_type, state, &format!("{label}_cancel")));
    instructions.extend(lower_complete_state(submission, artifact, qualified_type, state));
    instructions
}

fn lower_complete_state(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    qualified_type: &str,
    state: &SuspendStateArtifact,
) -> Vec<MsilInstruction> {
    if let Some(next) = artifact.states.iter().find(|candidate| candidate.state_id == state.state_id + 1) {
        return lower_effect_for_state(submission, artifact, qualified_type, next);
    }
    lower_move_next_done("")
}

fn lower_effect_for_state(
    submission: &FragmentSubmission,
    artifact: &SuspendFunctionArtifact,
    qualified_type: &str,
    state: &SuspendStateArtifact,
) -> Vec<MsilInstruction> {
    match state.effect.as_str() {
        "Yield" => lower_yield_case(artifact, qualified_type, &format!("next_{}", state.state_id), state),
        "DelegateYield" => lower_delegate_yield_case(submission, artifact, qualified_type, &format!("next_{}", state.state_id), state),
        "Await" => lower_await_case(submission, artifact, qualified_type, &format!("next_{}", state.state_id), state),
        "AsyncSpawn" => lower_async_spawn_case(submission, artifact, qualified_type, &format!("next_{}", state.state_id), state),
        "AsyncBlock" => lower_async_block_case(submission, artifact, qualified_type, &format!("next_{}", state.state_id), state),
        "Raise" => lower_raise_case(artifact, qualified_type, &format!("next_{}", state.state_id), state),
        _ => lower_move_next_done(&format!("next_{}", state.state_id)),
    }
}

fn emit_witness_receiver_load(qualified_type: &str, artifact: &SuspendFunctionArtifact, state: &SuspendStateArtifact) -> Vec<MsilInstruction> {
    let Some(field) = witness_receiver_field(state).filter(|field| frame_has_field(artifact, field))
    else {
        return vec![MsilInstruction { label: None, opcode: MsilOpcode::Ldnull, operand: None }];
    };
    vec![
        MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
        MsilInstruction {
            label: None,
            opcode: MsilOpcode::Ldfld,
            operand: Some(MsilInstructionOperand::Field(qualified_type.to_string(), field.to_string())),
        },
    ]
}

fn lower_move_next_done(label: &str) -> Vec<MsilInstruction> {
    vec![
        MsilInstruction { label: Some(label.to_string()), opcode: MsilOpcode::LdcI4_0, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
    ]
}

fn lower_run_wrapper(artifact: &SuspendFunctionArtifact, qualified_type: &str) -> MsilMethodBody {
    MsilMethodBody {
        method: MsilMethodRef {
            owner: Some(artifact.state_machine_type.clone()),
            name: run_name(artifact),
            signature: MsilMethodSignature::new(MsilType::Int32 { signed: true }, Vec::new()),
        },
        locals: Vec::new(),
        instructions: vec![
            MsilInstruction { label: Some("loop".to_string()), opcode: MsilOpcode::Ldarg0, operand: None },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Callvirt,
                operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                    owner: Some(qualified_type.to_string()),
                    name: "MoveNext".to_string(),
                    signature: MsilMethodSignature::new(MsilType::Bool, Vec::new()),
                })),
            },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Brtrue,
                operand: Some(MsilInstructionOperand::BranchTarget("loop".to_string())),
            },
            MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
        ],
        max_stack: 2,
        is_entry_point: false,
        is_async: false,
    }
}

/// 将 suspend 函数的全局方法替换为「构造状态机实例并调用 `run`」的包装方法。
///
/// 注册时同时写入两个名称：经 `sanitize_symbol` 处理的完整限定符号
/// （如 `module__await_value`）与符号末段裸名（如 `await_value`）。
/// 后者作为别名，使调用方仅凭裸名（`resolve_static_call_symbol` 回退路径）
/// 也能在 `build_local_method_token_map` 中查到本地方法 token。
fn replace_operation_method(module: &mut MsilModule, artifact: &SuspendFunctionArtifact, namespace: &str) {
    let name = sanitize_symbol(&artifact.symbol.to_string());
    let bare = artifact.symbol.parts().last().map(|part| sanitize_symbol(part.as_str())).unwrap_or_else(|| name.clone());
    let qualified = qualified_state_machine_type(artifact, namespace);
    let build_wrapper = |method_name: String| MsilMethodBody {
        method: MsilMethodRef {
            owner: None,
            name: method_name,
            signature: MsilMethodSignature::new(MsilType::Int32 { signed: true }, Vec::new()),
        },
        locals: Vec::new(),
        instructions: vec![
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Newobj,
                operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                    owner: Some(qualified.clone()),
                    name: ".ctor".to_string(),
                    signature: MsilMethodSignature::new(MsilType::Void, Vec::new()),
                })),
            },
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Callvirt,
                operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                    owner: Some(qualified.clone()),
                    name: run_name(artifact),
                    signature: MsilMethodSignature::new(MsilType::Int32 { signed: true }, Vec::new()),
                })),
            },
            MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
        ],
        max_stack: 1,
        is_entry_point: false,
        is_async: false,
    };
    let mut names = vec![name.clone()];
    if bare != name {
        names.push(bare.clone());
    }
    for method_name in names {
        let body = build_wrapper(method_name.clone());
        if let Some(method) = module.global_methods.iter_mut().find(|m| m.method.name == method_name) {
            *method = body;
        }
        else {
            module.global_methods.push(body);
        }
    }
}

fn replace_entry_with_suspend_call(module: &mut MsilModule, entry: &QualifiedName) {
    let name = sanitize_operation_symbol(entry);
    if let Some(entry_method) = module.global_methods.iter_mut().find(|m| m.is_entry_point) {
        entry_method.instructions = vec![
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Call,
                operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                    owner: None,
                    name: name.clone(),
                    signature: MsilMethodSignature::new(MsilType::Int32 { signed: true }, Vec::new()),
                })),
            },
            MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
        ];
    }
}

fn qualified_state_machine_type(artifact: &SuspendFunctionArtifact, namespace: &str) -> String {
    if namespace.is_empty() { artifact.state_machine_type.clone() } else { format!("{namespace}.{}", artifact.state_machine_type) }
}

fn run_name(artifact: &SuspendFunctionArtifact) -> String {
    format!("run_{}", sanitize_symbol(&artifact.symbol.to_string()))
}

fn object_ctor_ref() -> MsilMethodRef {
    MsilMethodRef {
        owner: Some("[mscorlib]System.Object".to_string()),
        name: ".ctor".to_string(),
        signature: MsilMethodSignature::new(MsilType::Void, Vec::new()),
    }
}
