use std::collections::HashMap;

use std_data::binary::nyar_ir::{NyarHeadCode, NyarInstruction};

use crate::{
    error::NyarRuntimeError,
    frame::Frame,
    heap::ObjectHeap,
    module::LoadedModule,
    stack::ValueStack,
    value::{CoroutineState, ObjectId, Value},
};

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
    ///
    /// Carries the heap id of the resumed coroutine so the executor can stamp it onto
    /// the freshly pushed frame's `coroutine_origin`. When that frame later reaches
    /// `Return`, the executor writes `done = true` back into the heap entry — visible
    /// to every stack/local copy that holds the same `Value::Coroutine(id)`.
    ResumeCoroutine {
        /// Heap id of the coroutine being resumed.
        coroutine_id: ObjectId,
        /// The captured frame snapshot to restore.
        state: CoroutineState,
        /// The value injected into the resumed coroutine.
        resume_value: Value,
    },
    /// Invoke an effect handler found via `witness_entries`.
    ///
    /// The executor pops the current frame and captures it as a continuation
    /// (heap-backed coroutine), then pushes a fresh handler frame with the effect
    /// payload and `Value::Coroutine(continuation_id)` on its operand stack. The
    /// handler can `Resume` the continuation (if the effect's `Resume` type allows)
    /// or simply `Return` to unwind past it.
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
    /// Registered native handlers keyed by name.
    pub natives: &'a HashMap<String, NativeHandler>,
}

/// Native function signature.
pub type NativeHandler = for<'a> fn(&'a [Value]) -> Result<Value, NyarRuntimeError>;

/// Dispatches one decoded instruction.
pub fn dispatch(instruction: NyarInstruction, frame: &mut Frame, ctx: &mut ExecutionContext<'_>) -> Result<StepResult, NyarRuntimeError> {
    match instruction.code {
        NyarHeadCode::Nop => {
            frame.ip += instruction.size as usize;
            Ok(StepResult::Continue)
        }
        NyarHeadCode::Const => {
            let index = instruction.operand1;
            let constant = ctx.module.constant_at(index).ok_or(NyarRuntimeError::ConstantIndexOutOfRange(index))?;
            ctx.stack.push(Value::from_constant(constant));
            frame.ip += instruction.size as usize;
            Ok(StepResult::Continue)
        }
        NyarHeadCode::Pop => {
            ctx.stack.pop()?;
            frame.ip += instruction.size as usize;
            Ok(StepResult::Continue)
        }
        NyarHeadCode::Dup => {
            ctx.stack.dup()?;
            frame.ip += instruction.size as usize;
            Ok(StepResult::Continue)
        }
        NyarHeadCode::Return => Ok(StepResult::Return),
        NyarHeadCode::Yield => {
            let yielded_value = ctx.stack.pop()?;
            frame.ip += instruction.size as usize;
            Ok(StepResult::Suspend { yielded_value })
        }
        NyarHeadCode::Resume => {
            let resume_value = ctx.stack.pop()?;
            let coroutine = ctx.stack.pop()?;
            // Advance the caller's ip past the Resume opcode *before* the executor swaps frames.
            // Without this, control returns to the Resume instruction and re-executes it,
            // popping an already-empty stack (`StackUnderflow`).
            frame.ip += instruction.size as usize;
            match coroutine {
                Value::Coroutine(id) => {
                    let state = ctx.heap.get_coroutine(id).ok_or(NyarRuntimeError::ModuleLoad(format!("coroutine heap id {id} not found")))?;
                    if state.done {
                        return Err(NyarRuntimeError::TypeMismatch { expected: "active coroutine", actual: "completed coroutine".to_string() });
                    }
                    // Clone the snapshot for frame restoration; the heap entry stays in place so
                    // subsequent `done` / `yielded_value` mutations are visible to all copies.
                    Ok(StepResult::ResumeCoroutine { coroutine_id: id, state: state.clone(), resume_value })
                }
                other => Err(NyarRuntimeError::TypeMismatch { expected: "coroutine", actual: other.type_name().to_string() }),
            }
        }
        NyarHeadCode::PerformEffect => {
            // `raise expr`：payload 已在栈顶。
            // operand1 = 常量池索引，指向 effect 的 method_name 字符串。
            // VM 用 method_name 在 witness_entries 中查找 handler：
            //   找到 -> InvokeHandler（捕获续延 + 调用 handler）
            //   没找到 -> 降级为 Suspend（向后兼容无 witness_entries 的模块）
            let effect_value = ctx.stack.pop()?;
            frame.ip += instruction.size as usize;

            let effect_name_index = instruction.operand1;
            let effect_name = match ctx.module.constant_at(effect_name_index) {
                Some(std_data::binary::nyar_ir::NyarConstant::String(name)) => name.as_str(),
                _ => "raise",
            };

            // 在 witness_entries 中搜索匹配的 handler
            let handler_entry = ctx.module.witness_entries.iter().find(|entry| entry.method_name == effect_name);

            match handler_entry {
                Some(entry) if entry.function_index >= 0 => {
                    // 找到 handler：由 executor 捕获当前帧为 continuation 并 push handler frame。
                    Ok(StepResult::InvokeHandler { handler_function_index: entry.function_index as usize, effect_value })
                }
                _ => {
                    // 没找到 handler：降级为 Suspend（向后兼容）
                    Ok(StepResult::Suspend { yielded_value: effect_value })
                }
            }
        }
        NyarHeadCode::Call | NyarHeadCode::CallStatic => execute_call(instruction, frame, ctx.module),
        NyarHeadCode::Jump
        | NyarHeadCode::JumpIfTrue
        | NyarHeadCode::JumpIfFalse
        | NyarHeadCode::LoadLocal
        | NyarHeadCode::StoreLocal
        | NyarHeadCode::LoadArg
        | NyarHeadCode::LoadGlobal
        | NyarHeadCode::StoreGlobal
        | NyarHeadCode::CallNative => execute_control(instruction, frame, ctx),
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
        | NyarHeadCode::I32GeS => execute_arithmetic(instruction, frame, ctx.stack),
        _ => execute_object(instruction, frame, ctx),
    }
}

/// Resolves a native function name from the constant pool.
pub(crate) fn native_name_from_constant(module: &LoadedModule, index: i32) -> Result<String, NyarRuntimeError> {
    let constant = module.constant_at(index).ok_or(NyarRuntimeError::ConstantIndexOutOfRange(index))?;
    match constant {
        std_data::binary::nyar_ir::NyarConstant::String(name) => Ok(name.clone()),
        other => Err(NyarRuntimeError::TypeMismatch { expected: "string", actual: format!("{other:?}") }),
    }
}
