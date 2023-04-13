use nyar_bytecode::{NyarHeadCode, NyarInstruction};

use crate::{
    array_runtime::{array_get, array_len, array_set},
    error::NyarRuntimeError,
    frame::Frame,
    host::{ResolvedImport, execute_host_op},
    module::LoadedModule,
    ops::{ExecutionContext, StepResult},
    value::Value,
};

/// Dispatches control-flow and variable instructions.
pub fn execute_control(
    instruction: NyarInstruction,
    frame: &mut Frame,
    ctx: &mut ExecutionContext<'_>,
) -> Result<StepResult, NyarRuntimeError> {
    match instruction.code {
        NyarHeadCode::Jump => {
            frame.ip = frame.ip.wrapping_add(instruction.operand1 as usize);
            Ok(StepResult::Continue)
        }
        NyarHeadCode::JumpIfTrue | NyarHeadCode::JumpIfFalse => {
            let condition = ctx.stack.pop()?.to_bool();
            let should_jump = match instruction.code {
                NyarHeadCode::JumpIfTrue => condition,
                NyarHeadCode::JumpIfFalse => !condition,
                _ => false,
            };
            if should_jump {
                frame.ip = frame.ip.wrapping_add(instruction.operand1 as usize);
            }
            else {
                frame.ip += instruction.size as usize;
            }
            Ok(StepResult::Continue)
        }
        NyarHeadCode::LoadLocal | NyarHeadCode::LoadArg => {
            let index = instruction.operand1 as usize;
            let value = frame.locals.get(index).cloned().ok_or(NyarRuntimeError::LocalIndexOutOfRange(instruction.operand1))?;
            ctx.stack.push(value);
            frame.ip += instruction.size as usize;
            Ok(StepResult::Continue)
        }
        NyarHeadCode::StoreLocal => {
            let index = instruction.operand1 as usize;
            let value = ctx.stack.pop()?;
            let slot = frame.locals.get_mut(index).ok_or(NyarRuntimeError::LocalIndexOutOfRange(instruction.operand1))?;
            *slot = value;
            frame.ip += instruction.size as usize;
            Ok(StepResult::Continue)
        }
        NyarHeadCode::LoadGlobal => {
            let index = instruction.operand1 as usize;
            let value = ctx.globals.get(index).cloned().ok_or(NyarRuntimeError::GlobalIndexOutOfRange(instruction.operand1))?;
            ctx.stack.push(value);
            frame.ip += instruction.size as usize;
            Ok(StepResult::Continue)
        }
        NyarHeadCode::StoreGlobal => {
            let index = instruction.operand1 as usize;
            let value = ctx.stack.pop()?;
            let slot = ctx.globals.get_mut(index).ok_or(NyarRuntimeError::GlobalIndexOutOfRange(instruction.operand1))?;
            let satb = ctx.heap.concurrent_mark().requires_satb();
            nyar_gc::write_value_slot(ctx.heap.barrier_mut(), slot, value, satb);
            frame.ip += instruction.size as usize;
            Ok(StepResult::Continue)
        }
        NyarHeadCode::CallImport => {
            let import_index = instruction.operand1;
            if import_index < 0 || (import_index as usize) >= ctx.module.resolved_imports.len() {
                return Err(NyarRuntimeError::ImportIndexOutOfRange(import_index));
            }
            let resolved = ctx.module.resolved_imports[import_index as usize];
            let arg_count = instruction.operand2.max(0) as usize;
            let mut args = Vec::with_capacity(arg_count);
            for _ in 0..arg_count {
                args.push(ctx.stack.pop()?);
            }
            args.reverse();

            let result = match resolved {
                ResolvedImport::Host(op) => execute_host_op(op, &args, ctx.heap)?,
                ResolvedImport::External => {
                    return Err(NyarRuntimeError::UnsupportedFeature(
                        "non-host CallImport requires import-index binding; string host dispatch removed",
                    ));
                }
            };
            ctx.stack.push(result);
            frame.ip += instruction.size as usize;
            Ok(StepResult::Continue)
        }
        NyarHeadCode::CallIntrinsic => {
            // 稠密下标必须与 `nyar_types::IntrinsicId::bytecode_index` 对齐（VM 不依赖 nyar-types）。
            let intrinsic_index = instruction.operand1;
            let arg_count = instruction.operand2.max(0) as usize;
            let mut args = Vec::with_capacity(arg_count);
            for _ in 0..arg_count {
                args.push(ctx.stack.pop()?);
            }
            args.reverse();

            let result = match intrinsic_index {
                0 => {
                    return Err(NyarRuntimeError::UnsupportedFeature("CallIntrinsic ArrayPush"));
                }
                1 => array_len(ctx.heap, args.first().unwrap_or(&Value::Null))?,
                2 => array_get(ctx.heap, args.first().unwrap_or(&Value::Null), args.get(1).unwrap_or(&Value::Null))?,
                3 => array_set(
                    ctx.heap,
                    args.first().unwrap_or(&Value::Null),
                    args.get(1).unwrap_or(&Value::Null),
                    args.get(2).unwrap_or(&Value::Null),
                )?,
                4 => {
                    return Err(NyarRuntimeError::UnsupportedFeature("CallIntrinsic RefDeref"));
                }
                5 => {
                    let value = args.first().unwrap_or(&Value::Null);
                    Value::Bool(matches!(value, Value::Null))
                }
                6 => match args.first().unwrap_or(&Value::Null) {
                    Value::Null => {
                        return Err(NyarRuntimeError::TypeMismatch {
                            expected: "non-null value",
                            actual: "null".to_string(),
                        });
                    }
                    other => other.clone(),
                },
                other => return Err(NyarRuntimeError::UnknownIntrinsic(other)),
            };
            ctx.stack.push(result);
            frame.ip += instruction.size as usize;
            Ok(StepResult::Continue)
        }
        _ => Err(NyarRuntimeError::UnknownOpcode(instruction.code as u8)),
    }
}

/// Enters a nested function call.
pub fn execute_call(instruction: NyarInstruction, frame: &mut Frame, module: &LoadedModule) -> Result<StepResult, NyarRuntimeError> {
    let function_index = instruction.operand1;
    if function_index < 0 {
        return Err(NyarRuntimeError::FunctionIndexOutOfRange(function_index));
    }
    let function_index = function_index as usize;
    if function_index >= module.functions.len() {
        return Err(NyarRuntimeError::FunctionIndexOutOfRange(function_index as i32));
    }
    frame.ip += instruction.size as usize;
    Ok(StepResult::Call { function_index })
}
