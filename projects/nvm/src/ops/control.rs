use std_data::binary::nyar_ir::{NyarHeadCode, NyarInstruction};

use crate::{
    error::NyarRuntimeError,
    frame::Frame,
    heap::ObjectPayload,
    module::LoadedModule,
    ops::{ExecutionContext, StepResult, native_name_from_constant},
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
            *slot = value;
            frame.ip += instruction.size as usize;
            Ok(StepResult::Continue)
        }
        NyarHeadCode::CallNative => {
            let name = native_name_from_constant(ctx.module, instruction.operand1)?;
            let arg_count = instruction.operand2.max(0) as usize;
            let mut args = Vec::with_capacity(arg_count);
            for _ in 0..arg_count {
                args.push(ctx.stack.pop()?);
            }
            args.reverse();

            let result = if name == "alloc_record" {
                let type_name = match args.first() {
                    Some(Value::String(name)) => name.clone(),
                    Some(other) => {
                        return Err(NyarRuntimeError::TypeMismatch { expected: "string", actual: other.type_name().to_string() });
                    }
                    None => {
                        return Err(NyarRuntimeError::TypeMismatch { expected: "string", actual: "empty".to_string() });
                    }
                };
                let object_id = ctx.heap.alloc(ObjectPayload::Record(vec![("__type__".to_string(), Value::String(type_name))]));
                Value::Object(object_id)
            }
            else if name == "record_get" {
                let field = match args.get(1) {
                    Some(Value::String(name)) => name.as_str(),
                    Some(other) => {
                        return Err(NyarRuntimeError::TypeMismatch { expected: "string", actual: other.type_name().to_string() });
                    }
                    None => return Ok(StepResult::Continue),
                };
                let object_id = match args.first() {
                    Some(Value::Object(id)) => *id,
                    Some(Value::Null) => {
                        ctx.stack.push(Value::Null);
                        frame.ip += instruction.size as usize;
                        return Ok(StepResult::Continue);
                    }
                    Some(other) => {
                        return Err(NyarRuntimeError::TypeMismatch { expected: "object", actual: other.type_name().to_string() });
                    }
                    None => {
                        ctx.stack.push(Value::Null);
                        frame.ip += instruction.size as usize;
                        return Ok(StepResult::Continue);
                    }
                };
                let payload = ctx.heap.get(object_id).ok_or_else(|| NyarRuntimeError::ModuleLoad(format!("invalid object id {object_id}")))?;
                match payload {
                    ObjectPayload::Record(fields) => {
                        fields.iter().find(|(key, _)| key == field).map(|(_, value)| value.clone()).unwrap_or(Value::Null)
                    }
                    // Coroutines expose no record fields; reads on them resolve to Null.
                    ObjectPayload::Coroutine(_) => Value::Null,
                }
            }
            else if name == "record_set" {
                let value = args.get(2).cloned().unwrap_or(Value::Null);
                let field = match args.get(1) {
                    Some(Value::String(name)) => name.clone(),
                    Some(other) => {
                        return Err(NyarRuntimeError::TypeMismatch { expected: "string", actual: other.type_name().to_string() });
                    }
                    None => {
                        ctx.stack.push(Value::Null);
                        frame.ip += instruction.size as usize;
                        return Ok(StepResult::Continue);
                    }
                };
                if let Some(Value::Object(object_id)) = args.first() {
                    if let Some(ObjectPayload::Record(fields)) = ctx.heap.get_mut(*object_id) {
                        if let Some(entry) = fields.iter_mut().find(|(key, _)| key == &field) {
                            entry.1 = value;
                        }
                        else {
                            fields.push((field, value));
                        }
                    }
                }
                Value::Null
            }
            else if name == "print" {
                let text = args.iter().map(value_display).collect::<Vec<_>>().join("\t");
                println!("{text}");
                args.last().cloned().unwrap_or(Value::Null)
            }
            else if name == "string_concat" {
                let left = args.first().map(value_display).unwrap_or_default();
                let right = args.get(1).map(value_display).unwrap_or_default();
                Value::String(format!("{left}{right}"))
            }
            else {
                let handler = ctx.natives.get(&name).copied().ok_or_else(|| NyarRuntimeError::NativeNotRegistered(name.clone()))?;
                handler(&args)?
            };
            ctx.stack.push(result);
            frame.ip += instruction.size as usize;
            Ok(StepResult::Continue)
        }
        _ => Err(NyarRuntimeError::UnknownOpcode(instruction.code as u8)),
    }
}

fn value_display(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(value) => value.to_string(),
        Value::I32(value) => value.to_string(),
        Value::I64(value) => value.to_string(),
        Value::F32(value) => value.to_string(),
        Value::F64(value) => {
            if value.fract() == 0.0 && value.is_finite() {
                format!("{:.0}", value)
            }
            else {
                value.to_string()
            }
        }
        Value::String(value) => value.clone(),
        Value::Object(_) => "object".to_string(),
        Value::Coroutine(_) => "coroutine".to_string(),
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
