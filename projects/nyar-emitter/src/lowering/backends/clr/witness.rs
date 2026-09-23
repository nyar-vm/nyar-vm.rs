//! CLR witness 入口 dispatch 改写。
//!
//! 本模块**不再**生成任何 `witness_*` 转发桩或占位桩。所有 witness 调用
//! （无论来自 `mir.rs` 普通调用还是 `suspend.rs` 状态机）都必须直接打到
//! 真实 Valkyrie 方法符号 `{Type}_{method}`（由 `lower_fragment_to_msil` 发出）。
//! 若真实方法不在当前模块，则由 `reject_unresolved_local_calls` 报编译错误，
//! 绝不在 Rust 端写一份返回默认值的 mock——那属于虚假自举。

use crate::{
    FragmentSubmission,
    lowering::shared::witness_abi::witness_slot_msil_signature,
    nyar_backend_clr::{MsilInstruction, MsilInstructionOperand, MsilMethodRef, MsilMethodSignature, MsilModule, MsilOpcode, MsilType},
};
use miette::{Result, miette};
use nyar::{WitnessCallEdge, WitnessMethodSlotSubmission, WitnessSubmission};

use super::sanitize_symbol;

pub(crate) fn augment_msil_with_witness(submission: &FragmentSubmission, module: &mut MsilModule) -> Result<()> {
    if submission.witness_tables.is_empty() {
        return Ok(());
    }
    if submission.witness_calls.is_empty() {
        return Ok(());
    }
    replace_entry_with_witness_instructions(submission, module)?;
    ensure_console_extern(module);
    Ok(())
}

/// 构造 witness impl 方法对应的真实 Valkyrie 方法 MSIL 符号。
///
/// MIR 层 `lower_impl_method_functions` 按 `{Type}.{method}` 约定为 impl 方法生成
/// 独立 MIR 函数，CLR 后端经 `sanitize_operation_symbol` → `sanitize_symbol` 把它
/// 转成合法 MethodDef 名（`.` → `_`）。这里复用同一套规则。
pub(crate) fn real_method_msil_name(table: &WitnessSubmission, method: &WitnessMethodSlotSubmission) -> String {
    sanitize_symbol(&format!("{}.{}", table.type_name, method.method_name))
}

/// 检查真实 Valkyrie 方法是否已经被 `lower_fragment_to_msil` 发出到当前模块。
pub(crate) fn real_method_exists(table: &WitnessSubmission, method: &WitnessMethodSlotSubmission, module: &MsilModule) -> bool {
    let target = real_method_msil_name(table, method);
    module.global_methods.iter().any(|body| body.method.name == target)
}

fn replace_entry_with_witness_instructions(submission: &FragmentSubmission, module: &mut MsilModule) -> Result<()> {
    // 先在不可变借用下算出每个 witness 调用的目标符号和签名，避免和下面
    // `global_methods.iter_mut()` 的可变借用冲突。
    let mut call_targets: Vec<(String, MsilMethodSignature, bool)> = Vec::new();
    for edge in &submission.witness_calls {
        let Some(slot) = resolve_witness_slot(submission, edge)
        else {
            continue;
        };
        let table = submission.witness_tables.iter().find(|table| table.type_name == edge.type_name && table.trait_name == edge.trait_name);
        // 入口 demo 调用直接走真实 Valkyrie 方法符号；若真实方法不在模块，则保留
        // `impl_symbol`——由于本模块不再生成 `witness_*` 桩，`reject_unresolved_local_calls`
        // 会将其作为未解析本地调用报编译错误，而不是用 Rust 写一份假实现。
        let table = table.ok_or_else(|| {
            miette!(code = "nyar::clr::unresolved_witness", "CLR witness table metadata is missing for {}::{}", edge.type_name, edge.trait_name)
        })?;
        if !real_method_exists(table, slot, module) {
            return Err(miette!(
                code = "nyar::clr::unresolved_witness",
                "CLR witness target `{}` is not present in the emitted MIR method set",
                real_method_msil_name(table, slot)
            ));
        }
        let call_target = real_method_msil_name(table, slot);
        call_targets.push((call_target, witness_call_signature(submission, edge), edge.print_result));
    }

    let Some(entry) = module.global_methods.iter_mut().find(|method| method.is_entry_point)
    else {
        return Err(miette!(code = "nyar::clr::missing_entry", "CLR witness augmentation requires an entry method"));
    };

    let mut instructions = Vec::new();
    for (call_target, signature, print_result) in &call_targets {
        instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ldnull, operand: None });
        instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Call,
            operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                owner: None,
                name: call_target.clone(),
                signature: signature.clone(),
            })),
        });
        if *print_result {
            instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Call,
                operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                    owner: Some("[System.Console]System.Console".to_string()),
                    name: "WriteLine".to_string(),
                    signature: MsilMethodSignature::new(MsilType::Void, vec![MsilType::String]),
                })),
            });
        }
    }

    if let Some(entry_operation) = submission.entry_operation.as_ref() {
        let entry_symbol = super::sanitize_operation_symbol(entry_operation);
        instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Call,
            operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                owner: None,
                name: entry_symbol,
                signature: MsilMethodSignature::new(MsilType::Int32 { signed: true }, Vec::new()),
            })),
        });
        entry.instructions = instructions;
        entry.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None });
        entry.max_stack = 2;
        return Ok(());
    }

    instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None });
    instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None });
    entry.instructions = instructions;
    entry.max_stack = 2;
    Ok(())
}

fn witness_call_signature(submission: &FragmentSubmission, edge: &WitnessCallEdge) -> MsilMethodSignature {
    submission
        .witness_tables
        .iter()
        .find(|table| table.trait_name == edge.trait_name && table.type_name == edge.type_name)
        .and_then(|table| {
            table.methods.iter().find(|slot| slot.method_index == edge.method_index).map(|slot| {
                let (return_type, params) = witness_slot_msil_signature(table, slot);
                MsilMethodSignature::new(return_type, params)
            })
        })
        .unwrap_or_else(|| MsilMethodSignature::new(MsilType::Object, vec![MsilType::Object]))
}

fn resolve_witness_slot<'a>(submission: &'a FragmentSubmission, edge: &WitnessCallEdge) -> Option<&'a WitnessMethodSlotSubmission> {
    submission
        .witness_tables
        .iter()
        .find(|table| table.trait_name == edge.trait_name && table.type_name == edge.type_name)
        .and_then(|table| table.methods.iter().find(|slot| slot.method_index == edge.method_index))
}

fn ensure_console_extern(module: &mut MsilModule) {
    for segment in ["mscorlib", "System.Console"] {
        if !module.assembly.externs.iter().any(|existing| existing == segment) {
            module.assembly.externs.push(segment.to_string());
        }
    }
}
