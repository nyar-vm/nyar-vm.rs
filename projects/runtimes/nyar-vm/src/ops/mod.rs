use nyar_format::NyarHeadCode;

use crate::{
    error::NyarRuntimeError,
    executable::ExecOp,
    frame::Frame,
    module::LoadedModule,
    stack::ValueStack,
    value::{CoroutineState, ObjectId, Value, value_from_constant},
};
use nyar_gc::ObjectHeap;

mod arithmetic;
mod control;
mod object;

pub use arithmetic::execute_arithmetic;
pub use control::{execute_call, execute_control};
pub use object::execute_object;

/// Result of executing one instruction.
#[derive(Debug, Clone, PartialEq)]
pub enum StepResult {
    /// Continue with the current frame.
    Continue,
    /// Return from the current frame.
    Return,
    /// Call a nested frame.
    Call {
        /// Target function index.
        function_index: usize,
    },
    /// Suspend the current frame: pop a yielded value and capture the frame as a coroutine.
    Suspend {
        /// The value yielded to the caller.
        yielded_value: Value,
    },
    /// Resume a coroutine: restore a suspended frame and inject the resume value.
    ResumeCoroutine {
        /// Heap id of the coroutine being resumed.
        coroutine_id: ObjectId,
        /// The captured frame snapshot to restore.
        state: CoroutineState,
        /// The value injected into the resumed coroutine.
        resume_value: Value,
    },
    /// Invoke an effect handler found via `witness_entries`.
    InvokeHandler {
        /// Handler function index from `witness_entries`.
        handler_function_index: usize,
        /// The effect payload value popped from the stack.
        effect_value: Value,
    },
}

/// Execution context passed to opcode handlers.
pub struct ExecutionContext<'a> {
    /// Loaded module metadata.
    pub module: &'a LoadedModule,
    /// Module-level global slots.
    pub globals: &'a mut [Value],
    /// Operand stack.
    pub stack: &'a mut ValueStack,
    /// Object heap.
    pub heap: &'a mut ObjectHeap,
}

/// 执行一条预解码内码；`frame.ip` 为函数内指令下标。
pub fn dispatch_exec(op: ExecOp, frame: &mut Frame, ctx: &mut ExecutionContext<'_>) -> Result<StepResult, NyarRuntimeError> {
    match op.code {
        NyarHeadCode::Jump => {
            frame.ip = op.target as usize;
            Ok(StepResult::Continue)
        }
        NyarHeadCode::JumpIfTrue | NyarHeadCode::JumpIfFalse => {
            let condition = ctx.stack.pop()?.to_bool();
            let should_jump = match op.code {
                NyarHeadCode::JumpIfTrue => condition,
                NyarHeadCode::JumpIfFalse => !condition,
                _ => false,
            };
            if should_jump {
                frame.ip = op.target as usize;
            }
            else {
                frame.ip += 1;
            }
            Ok(StepResult::Continue)
        }
        NyarHeadCode::Nop => {
            frame.ip += 1;
            Ok(StepResult::Continue)
        }
        NyarHeadCode::Const => {
            let index = op.operand1;
            let constant = ctx.module.constant_at(index).ok_or(NyarRuntimeError::ConstantIndexOutOfRange(index))?;
            ctx.stack.push(value_from_constant(constant));
            frame.ip += 1;
            Ok(StepResult::Continue)
        }
        NyarHeadCode::Pop => {
            ctx.stack.pop()?;
            frame.ip += 1;
            Ok(StepResult::Continue)
        }
        NyarHeadCode::Dup => {
            ctx.stack.dup()?;
            frame.ip += 1;
            Ok(StepResult::Continue)
        }
        NyarHeadCode::Return => Ok(StepResult::Return),
        NyarHeadCode::Yield => {
            let yielded_value = ctx.stack.pop()?;
            frame.ip += 1;
            Ok(StepResult::Suspend { yielded_value })
        }
        NyarHeadCode::Resume => {
            let resume_value = ctx.stack.pop()?;
            let coroutine = ctx.stack.pop()?;
            frame.ip += 1;
            match coroutine {
                Value::Coroutine(id) => {
                    let state =
                        ctx.heap.get_coroutine(id).ok_or(NyarRuntimeError::ModuleLoad(format!("coroutine heap id {id} not found")))?;
                    if state.done {
                        return Err(NyarRuntimeError::TypeMismatch {
                            expected: "active coroutine",
                            actual: "completed coroutine".to_string(),
                        });
                    }
                    Ok(StepResult::ResumeCoroutine { coroutine_id: id, state: state.clone(), resume_value })
                }
                other => Err(NyarRuntimeError::TypeMismatch { expected: "coroutine", actual: other.type_name().to_string() }),
            }
        }
        NyarHeadCode::PerformEffect => {
            let effect_value = ctx.stack.pop()?;
            frame.ip += 1;

            let effect_name_index = op.operand1;
            let effect_name = match ctx.module.constant_at(effect_name_index) {
                Some(nyar_format::NyarConstant::String(name)) => name.as_str(),
                _ => "raise",
            };

            let handler_entry = ctx.module.witness_entries.iter().find(|entry| entry.method_name == effect_name);

            match handler_entry {
                Some(entry) if entry.function_index >= 0 => {
                    Ok(StepResult::InvokeHandler { handler_function_index: entry.function_index as usize, effect_value })
                }
                _ => Ok(StepResult::Suspend { yielded_value: effect_value }),
            }
        }
        NyarHeadCode::Call | NyarHeadCode::CallStatic => execute_call(op.as_instruction(), frame, ctx.module),
        NyarHeadCode::LoadLocal
        | NyarHeadCode::StoreLocal
        | NyarHeadCode::LoadArg
        | NyarHeadCode::LoadGlobal
        | NyarHeadCode::StoreGlobal
        | NyarHeadCode::CallImport
        | NyarHeadCode::CallIntrinsic => execute_control(op.as_instruction(), frame, ctx),
        NyarHeadCode::I32Add
        | NyarHeadCode::I32Sub
        | NyarHeadCode::I32Mul
        | NyarHeadCode::I32DivS
        | NyarHeadCode::I32RemS
        | NyarHeadCode::I32Eq
        | NyarHeadCode::I32Ne
        | NyarHeadCode::I32LtS
        | NyarHeadCode::I32LeS
        | NyarHeadCode::I32GtS
        | NyarHeadCode::I32GeS => execute_arithmetic(op.as_instruction(), frame, ctx.stack),
        NyarHeadCode::ObjectNew | NyarHeadCode::FieldGet | NyarHeadCode::FieldSet => {
            execute_object(op.as_instruction(), frame, ctx)
        }
    }
}
