//! 结构指令：`ObjectNew` / `FieldGet` / `FieldSet`（按 layout_id + field_slot）。

use std_data::binary::nyar_ir::{NyarHeadCode, NyarInstruction};

use nyar_gc::ObjectPayload;

use crate::{
    error::NyarRuntimeError,
    frame::Frame,
    ops::{ExecutionContext, StepResult},
    value::Value,
};

/// 执行结构指令族。
pub fn execute_object(
    instruction: NyarInstruction,
    frame: &mut Frame,
    ctx: &mut ExecutionContext<'_>,
) -> Result<StepResult, NyarRuntimeError> {
    match instruction.code {
        NyarHeadCode::ObjectNew => {
            let layout_id = instruction.operand1;
            if layout_id < 0 || (layout_id as usize) >= ctx.module.layouts.len() {
                return Err(NyarRuntimeError::LayoutIndexOutOfRange(layout_id));
            }
            let field_count = ctx.module.layouts[layout_id as usize].field_count;
            if field_count < 0 {
                return Err(NyarRuntimeError::ModuleLoad(format!("layout[{layout_id}] has negative field_count")));
            }
            let slots = vec![Value::Null; field_count as usize];
            let object_id = ctx.heap.alloc(ObjectPayload::LayoutObject {
                layout_id: layout_id as u32,
                slots,
            });
            ctx.stack.push(Value::Object(object_id));
            frame.ip += instruction.size as usize;
            Ok(StepResult::Continue)
        }
        NyarHeadCode::FieldGet => {
            let field_slot = instruction.operand1;
            let object = ctx.stack.pop()?;
            let object_id = match object {
                Value::Object(id) => id,
                other => {
                    return Err(NyarRuntimeError::TypeMismatch {
                        expected: "object",
                        actual: other.type_name().to_string(),
                    });
                }
            };
            let value = match ctx.heap.get(object_id) {
                Some(ObjectPayload::LayoutObject { slots, .. }) => {
                    if field_slot < 0 || (field_slot as usize) >= slots.len() {
                        return Err(NyarRuntimeError::FieldSlotOutOfRange(field_slot));
                    }
                    slots[field_slot as usize].clone()
                }
                Some(_) => {
                    return Err(NyarRuntimeError::TypeMismatch {
                        expected: "layout object",
                        actual: "non-layout object".to_string(),
                    });
                }
                None => {
                    return Err(NyarRuntimeError::ModuleLoad(format!("object heap id {object_id} not found")));
                }
            };
            ctx.stack.push(value);
            frame.ip += instruction.size as usize;
            Ok(StepResult::Continue)
        }
        NyarHeadCode::FieldSet => {
            let field_slot = instruction.operand1;
            let value = ctx.stack.pop()?;
            let object = ctx.stack.pop()?;
            let object_id = match object {
                Value::Object(id) => id,
                other => {
                    return Err(NyarRuntimeError::TypeMismatch {
                        expected: "object",
                        actual: other.type_name().to_string(),
                    });
                }
            };
            match ctx.heap.get_mut(object_id) {
                Some(ObjectPayload::LayoutObject { slots, .. }) => {
                    if field_slot < 0 || (field_slot as usize) >= slots.len() {
                        return Err(NyarRuntimeError::FieldSlotOutOfRange(field_slot));
                    }
                    slots[field_slot as usize] = value;
                }
                Some(_) => {
                    return Err(NyarRuntimeError::TypeMismatch {
                        expected: "layout object",
                        actual: "non-layout object".to_string(),
                    });
                }
                None => {
                    return Err(NyarRuntimeError::ModuleLoad(format!("object heap id {object_id} not found")));
                }
            }
            ctx.stack.push(Value::Object(object_id));
            frame.ip += instruction.size as usize;
            Ok(StepResult::Continue)
        }
        _ => Err(NyarRuntimeError::UnknownOpcode(instruction.code as u8)),
    }
}
