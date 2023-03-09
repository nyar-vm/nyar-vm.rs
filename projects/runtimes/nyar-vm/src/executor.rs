use std::fmt::{self, Debug, Formatter};

use std_data::binary::nyar_ir::decode_at;

use crate::{
    error::NyarRuntimeError,
    frame::Frame,
    jit::{DisabledJit, JitCompiledArtifact, JitCompiler, JitError, compile_request},
    module::LoadedModule,
    ops::{ExecutionContext, StepResult, dispatch},
    stack::ValueStack,
    value::{CoroutineState, Value},
};
use nyar_gc::{GarbageCollector, GcRoots, ObjectHeap};

/// Bytecode interpreter loop.
pub struct Executor {
    stack: ValueStack,
    heap: ObjectHeap,
    gc: GarbageCollector,
    frames: Vec<Frame>,
    jit: Box<dyn JitCompiler>,
}

impl Executor {
    /// Creates a new executor. Host imports dispatch via [`crate::host::HostOp`].
    pub fn new() -> Self {
        Self {
            stack: ValueStack::new(),
            heap: ObjectHeap::new(),
            gc: GarbageCollector::new(),
            frames: Vec::new(),
            jit: Box::new(DisabledJit),
        }
    }

    /// Creates an executor with a custom JIT backend.
    pub fn with_jit(jit: Box<dyn JitCompiler>) -> Self {
        let mut executor = Self::new();
        executor.jit = jit;
        executor
    }

    /// Whether the installed JIT backend is enabled.
    pub fn jit_enabled(&self) -> bool {
        self.jit.enabled()
    }

    /// Attempts JIT compilation for one module function.
    ///
    /// Returns `JitError::Unsupported` when the installed backend is disabled.
    pub fn try_jit_compile(&mut self, module: &LoadedModule, function_index: usize) -> Result<JitCompiledArtifact, JitError> {
        let request = compile_request(module, function_index)?;
        self.jit.compile_function(&request)
    }

    /// Replaces the JIT backend.
    pub fn set_jit(&mut self, jit: Box<dyn JitCompiler>) {
        self.jit = jit;
    }

    /// Borrows the object heap for inspection by callers (e.g. `NyarVm::heap`).
    pub fn heap(&self) -> &ObjectHeap {
        &self.heap
    }

    /// Executes a function in `module` and returns its result value.
    pub fn run(&mut self, module: &LoadedModule, function_index: usize, args: Vec<Value>) -> Result<Value, NyarRuntimeError> {
        self.run_with_globals(module, function_index, args, &mut vec![Value::Null; module.globals.len()])
    }

    /// Executes a function using caller-provided global storage (for repeated runs on the same module).
    pub fn run_with_globals(
        &mut self,
        module: &LoadedModule,
        function_index: usize,
        args: Vec<Value>,
        globals: &mut [Value],
    ) -> Result<Value, NyarRuntimeError> {
        if globals.len() != module.globals.len() {
            return Err(NyarRuntimeError::ModuleLoad(format!(
                "global slot count mismatch: expected {}, got {}",
                module.globals.len(),
                globals.len()
            )));
        }

        self.run_function_frame(module, function_index, args, globals)
    }

    pub(crate) fn reset_after_nested_run(&mut self) {
        self.frames.clear();
        self.stack = ValueStack::new();
    }

    pub(crate) fn run_function_frame(
        &mut self,
        module: &LoadedModule,
        function_index: usize,
        args: Vec<Value>,
        globals: &mut [Value],
    ) -> Result<Value, NyarRuntimeError> {
        if function_index >= module.functions.len() {
            return Err(NyarRuntimeError::FunctionIndexOutOfRange(function_index as i32));
        }

        let function = &module.functions[function_index];
        let mut frame = Frame::new(function_index, function.local_count.max(function.arity) as usize, 0);
        frame.ip = function.code_offset as usize;
        frame.set_arguments(args);
        self.frames = vec![frame];

        while let Some(current) = self.frames.last_mut() {
            let end = {
                let function = &module.functions[current.function_index];
                function.code_offset as usize + function.code_length as usize
            };

            if current.ip >= end {
                self.frames.pop();
                continue;
            }

            let instruction = decode_at(&module.code_bytes, current.ip);
            if !instruction.is_valid() {
                return Err(NyarRuntimeError::UnknownOpcode(module.code_bytes.get(current.ip).copied().unwrap_or(0)));
            }

            let mut ctx = ExecutionContext { module, globals, stack: &mut self.stack, heap: &mut self.heap };

            match dispatch(instruction, current, &mut ctx)? {
                StepResult::Continue => {}
                StepResult::Return => {
                    let finished = self.frames.pop().expect("return without frame");
                    // If this frame was resuming a coroutine, mark the heap entry as done and
                    // embed the final return value as the coroutine's `yielded_value`. This is
                    // the one place `done` flips to `true` — visible to all stack/local copies
                    // holding `Value::Coroutine(id)` because coroutines are heap-backed.
                    if let Some(coroutine_id) = finished.coroutine_origin {
                        if let Some(state) = self.heap.get_coroutine_mut(coroutine_id) {
                            state.done = true;
                            // The return value is whatever the function pushed before `Return`;
                            // peek (don't pop) so the normal Return path below still sees it.
                            state.yielded_value = self.stack.peek().cloned().unwrap_or(Value::Null);
                        }
                    }
                    let mut frame_locals: Vec<&[Value]> = self.frames.iter().map(|frame| frame.locals.as_slice()).collect();
                    frame_locals.push(finished.locals.as_slice());
                    self.gc.collect(
                        GcRoots { stack: self.stack.values(), frame_locals: &frame_locals, globals },
                        &mut self.heap,
                    );
                    if self.frames.is_empty() {
                        return self.stack.pop().or(Ok(Value::Null));
                    }
                }
                StepResult::Call { function_index } => {
                    let target = &module.functions[function_index];
                    let arity = target.arity.max(0) as usize;
                    let mut call_args = Vec::with_capacity(arity);
                    for _ in 0..arity {
                        call_args.push(self.stack.pop()?);
                    }
                    call_args.reverse();

                    let mut child = Frame::new(function_index, target.local_count.max(target.arity) as usize, self.stack.len());
                    child.ip = target.code_offset as usize;
                    child.set_arguments(call_args);
                    self.frames.push(child);
                }
                StepResult::Suspend { yielded_value } => {
                    // `Yield` 已由 dispatch 弹出 yielded 值并推进 ip。
                    // 此处将当前帧捕获为 `CoroutineState`（嵌入 yielded 值），存入 heap,
                    // 把 `Coroutine(ObjectId)` 作为 `Call` 的"返回值"压入父帧栈，
                    // 父帧据此感知一次挂起。若没有父帧（顶层调用），直接返回该 coroutine。
                    let suspended = self.frames.pop().expect("suspend without frame");
                    let state = CoroutineState {
                        function_index: suspended.function_index,
                        ip: suspended.ip,
                        locals: suspended.locals,
                        stack_base: suspended.stack_base,
                        done: false,
                        yielded_value,
                    };
                    let coroutine_id = self.heap.alloc_coroutine(state);
                    self.stack.push(Value::Coroutine(coroutine_id));
                    if self.frames.is_empty() {
                        return self.stack.pop().or(Ok(Value::Null));
                    }
                }
                StepResult::ResumeCoroutine { coroutine_id, state, resume_value } => {
                    // `Resume` 已由 dispatch 弹出 [resume_value, coroutine(ObjectId)]。
                    // 此处将 coroutine 的帧快照恢复为新的 Frame 压回 frames 栈，并记录
                    // `coroutine_origin` 以便该 frame Return 时回写 `done=true` 与最终返回值;
                    // 再把 resume_value 推入操作数栈，coroutine 从挂起点之后继续执行。
                    let mut frame = Frame::new(state.function_index, 0, self.stack.len());
                    frame.ip = state.ip;
                    frame.locals = state.locals;
                    frame.coroutine_origin = Some(coroutine_id);
                    self.frames.push(frame);
                    self.stack.push(resume_value);
                }
                StepResult::InvokeHandler { handler_function_index, effect_value } => {
                    // `PerformEffect` 已由 dispatch 弹出 effect_value 并推进 ip。
                    // 此处将当前帧捕获为 continuation（heap-backed coroutine），
                    // 然后压入 handler 的 fresh frame，并把 [continuation, effect_value]
                    // 推入操作数栈供 handler 使用。
                    //
                    // 栈布局（handler 视角）：
                    //   栈顶 = effect_value（raise 的 payload）
                    //   次顶 = Value::Coroutine(continuation_id)（被捕获的续延）
                    //
                    // handler 可以：
                    //   1. Resume continuation（用 Resume opcode 恢复续延，注入 resume 值）
                    //   2. Return 不 resume（unwind，续延被丢弃，其 done 保持 false 但无人引用）
                    let suspended = self.frames.pop().expect("invoke_handler without frame");
                    let continuation_state = CoroutineState {
                        function_index: suspended.function_index,
                        ip: suspended.ip,
                        locals: suspended.locals,
                        stack_base: suspended.stack_base,
                        done: false,
                        yielded_value: effect_value.clone(),
                    };
                    let continuation_id = self.heap.alloc_coroutine(continuation_state);

                    let target = &module.functions[handler_function_index];
                    let mut handler_frame = Frame::new(handler_function_index, target.local_count.max(target.arity) as usize, self.stack.len());
                    handler_frame.ip = target.code_offset as usize;
                    self.frames.push(handler_frame);

                    // 推入 handler 参数：先 continuation，再 effect_value（effect_value 在栈顶）
                    self.stack.push(Value::Coroutine(continuation_id));
                    self.stack.push(effect_value);
                }
            }
        }

        Ok(Value::Null)
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

impl Debug for Executor {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Executor")
            .field("stack", &self.stack)
            .field("heap", &self.heap)
            .field("gc", &self.gc)
            .field("frames", &self.frames)
            .field("jit_enabled", &self.jit.enabled())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std_data::binary::nyar_ir::{
        NyarConstant, NyarExport, NyarExportKind, NyarFunction, NyarHeadCode, NyarModuleData, NYAR_VERSION, encode_module,
    };

    use super::*;
    use crate::module::LoadedModule;

    #[test]
    fn runs_const_add_return_bytecode() {
        let code = vec![
            NyarHeadCode::Const as u8,
            0,
            0,
            0,
            0,
            NyarHeadCode::Const as u8,
            1,
            0,
            0,
            0,
            NyarHeadCode::I32Add as u8,
            NyarHeadCode::Return as u8,
        ];

        let data = NyarModuleData {
            version: NYAR_VERSION,
            name: "test".to_string(),
            constants: vec![NyarConstant::Integer32(0), NyarConstant::Integer32(1)],
            functions: vec![NyarFunction {
                name: "main".to_string(),
                arity: 0,
                local_count: 0,
                code_offset: 0,
                code_length: code.len() as i32,
            }],
            imports: Vec::new(),
            exports: vec![NyarExport { kind: NyarExportKind::Function, symbol_name: "main".to_string(), function_index: 0 }],
            witness_entries: Vec::new(),
            code_bytes: code,
            globals: Vec::new(),
            init_function_indices: Vec::new(),
        };

        let bytes = encode_module(&data);
        let module = LoadedModule::from_bytes(&bytes).expect("load module");
        let mut executor = Executor::new();
        let result = executor.run(&module, 0, Vec::new()).expect("execute");
        assert_eq!(result, Value::I32(1));
    }
}
