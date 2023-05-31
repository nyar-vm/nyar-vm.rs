use std::collections::HashMap;
use std::fmt::{self, Debug, Formatter};

use crate::{
    error::NyarRuntimeError,
    frame::Frame,
    jit::{
        DisabledJit, JitAssumption, JitCompiledArtifact, JitCompiler, JitError, RestoredInterpreterFrame, StackMapEntry,
        compile_request,
    },
    module::LoadedModule,
    ops::{ExecutionContext, StepResult, dispatch_exec},
    stack::ValueStack,
    value::{CoroutineState, Value},
};
use nyar_gc::{GarbageCollector, GcRoots, LayoutDescriptor, ObjectHeap};

/// NJ1 编译缓存键：`(module.version, module.name, function_index)`。
type Nj1CacheKey = (u32, String, usize);

#[derive(Clone)]
struct Nj1CacheEntry {
    blob: Vec<u8>,
    assumptions: Vec<JitAssumption>,
}

/// 按 stack map 条目抽取可能含引用的 local 槽（拷贝，供 `GcRoots` 借用）。
fn select_local_roots(locals: &[Value], entry: &StackMapEntry) -> Vec<Value> {
    entry
        .local_root_slots
        .iter()
        .filter_map(|&slot| locals.get(slot as usize).cloned())
        .collect()
}

/// Bytecode interpreter loop.
pub struct Executor {
    stack: ValueStack,
    heap: ObjectHeap,
    gc: GarbageCollector,
    frames: Vec<Frame>,
    jit: Box<dyn JitCompiler>,
    /// 已成功编译的 NJ1 blob 缓存（避免每次 `run` 重编译）。
    nj1_cache: HashMap<Nj1CacheKey, Nj1CacheEntry>,
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
            nj1_cache: HashMap::new(),
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

    /// NJ1 缓存条目数（测试 / 诊断）。
    pub fn nj1_cache_len(&self) -> usize {
        self.nj1_cache.len()
    }

    /// 清空全部 NJ1 编译缓存（代码失效 / deopt 假设变化后调用）。
    pub fn invalidate_nj1_cache(&mut self) {
        self.nj1_cache.clear();
    }

    /// 按模块版本与名称失效该模块下全部函数的 NJ1 缓存。
    pub fn invalidate_nj1_module(&mut self, module_version: u32, module_name: &str) {
        self.nj1_cache
            .retain(|(version, name, _), _| *version != module_version || name != module_name);
    }

    /// 失效依赖给定假设的全部 NJ1 缓存条目。
    pub fn invalidate_assumption(&mut self, assumption: JitAssumption) {
        self.nj1_cache
            .retain(|_, entry| !entry.assumptions.iter().any(|item| *item == assumption));
    }

    /// Attempts JIT compilation for one module function.
    ///
    /// Returns `JitError::Unsupported` when the installed backend is disabled.
    pub fn try_jit_compile(&mut self, module: &LoadedModule, function_index: usize) -> Result<JitCompiledArtifact, JitError> {
        let request = compile_request(module, function_index)?;
        self.jit.compile_function(&request)
    }

    /// Replaces the JIT backend（并清空 NJ1 缓存）。
    pub fn set_jit(&mut self, jit: Box<dyn JitCompiler>) {
        self.jit = jit;
        self.invalidate_nj1_cache();
    }

    /// 安装 deopt 物化帧链并失效 NJ1 缓存。
    ///
    /// `restored[0]` 为最内层（当前执行函数）；写入解释器栈时外层在前、内层在顶。
    pub fn install_deopt_frames(
        &mut self,
        restored: &[RestoredInterpreterFrame],
        stack_base: usize,
        roots: Option<&nyar_gc::HostRoots>,
    ) -> Result<(), NyarRuntimeError> {
        self.invalidate_nj1_cache();
        let mut frames = Vec::with_capacity(restored.len());
        for frame in restored.iter().rev() {
            frames.push(Frame::from_deopt_restore(frame, stack_base, roots)?);
        }
        self.frames = frames;
        Ok(())
    }

    /// 当前解释器帧数量（测试 / 诊断）。
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Borrows the object heap for inspection by callers (e.g. `NyarVm::heap`).
    pub fn heap(&self) -> &ObjectHeap {
        &self.heap
    }

    /// Mutably borrows the object heap（宿主根 / 策略调整）。
    pub fn heap_mut(&mut self) -> &mut ObjectHeap {
        &mut self.heap
    }

    /// 按工作负载提示同步堆策略与 ConcurrentTrace 有界预算。
    pub fn apply_workload_hints(&mut self, hints: nyar_gc::WorkloadHints) {
        self.gc.apply_workload_hints(&hints);
        self.heap.policy_mut().apply_hints(hints);
    }

    /// ConcurrentTrace 当前灰预算（测试 / 诊断）。
    pub fn gray_budget_per_slice(&self) -> usize {
        self.gc.gray_budget_per_slice()
    }

    /// 最近一次 ConcurrentTrace poll 的工作量记账。
    pub fn last_trace_poll(&self) -> nyar_gc::TracePollReport {
        self.gc.last_trace_poll()
    }

    /// 最近一次 ConcurrentTrace 周期的根握手证据。
    pub fn last_root_handshake(&self) -> nyar_gc::RootHandshakeReport {
        self.gc.last_root_handshake()
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

        // 将外码 layouts 登记为 GC 可见的 LayoutDescriptor（保守：全部槽可能含引用）。
        for (index, layout) in module.layouts.iter().enumerate() {
            let field_count = layout.field_count.max(0) as u32;
            self.heap.register_layout(LayoutDescriptor::all_references(index as u32, field_count));
        }

        let function = &module.functions[function_index];
        let mut frame = Frame::new(function_index, function.local_count.max(function.arity) as usize, 0);
        // 内码从指令下标 0 开始。
        frame.ip = 0;
        frame.set_arguments(args);

        // JIT 快路径：缓存命中或新编译出 NJ1 blob 时跳过解释循环（仅叶标量形态）。
        if self.jit.enabled() {
            let cache_key = (module.version, module.name.clone(), function_index);
            let blob = if let Some(cached) = self.nj1_cache.get(&cache_key) {
                Some(cached.blob.clone())
            } else if let Ok(artifact) = self.try_jit_compile(module, function_index) {
                let assumptions = artifact.assumptions.clone();
                artifact.machine_code.map(|blob| {
                    self.nj1_cache.insert(
                        cache_key,
                        Nj1CacheEntry {
                            blob: blob.clone(),
                            assumptions,
                        },
                    );
                    blob
                })
            } else {
                None
            };
            if let Some(blob) = blob {
                // 机器码叶路径仍遵守堆压力：进入前强制策略回收，并经统一转发图改写帧 / 栈 / 全局。
                if self.heap.over_soft_limit() || self.heap.nursery_pressure() {
                    self.frames = vec![frame];
                    let owned_locals = self.frames[0].locals.clone();
                    let frame_locals = [owned_locals.as_slice()];
                    let frame_coroutines: Vec<_> =
                        self.frames.iter().filter_map(|frame| frame.coroutine_origin).collect();
                    let relocate = self.gc.collect_for_policy(
                        GcRoots {
                            stack: self.stack.values(),
                            frame_locals: &frame_locals,
                            globals,
                            frame_coroutines: &frame_coroutines,
                        },
                        &mut self.heap,
                    );
                    self.apply_relocate_map(&relocate, globals);
                    frame = self.frames.pop().expect("nj1 pressure frame");
                }
                return crate::nj1_runtime::execute_nj1_blob(&blob, &frame.locals);
            }
        }

        self.frames = vec![frame];

        while let Some(current) = self.frames.last_mut() {
            let ops_len = module
                .executable
                .get(current.function_index)
                .map(|exec| exec.ops.len())
                .unwrap_or(0);

            if current.ip >= ops_len {
                self.frames.pop();
                continue;
            }

            let function_index = current.function_index;
            let ip_at_op = current.ip as u32;
            let pressure_safepoint = (self.heap.over_soft_limit() || self.heap.nursery_pressure())
                && module
                    .executable
                    .get(function_index)
                    .map(|exec| exec.safepoints.binary_search(&ip_at_op).is_ok())
                    .unwrap_or(false);

            let op = module.executable[function_index].ops[current.ip];
            let step = {
                let mut ctx = ExecutionContext { module, globals, stack: &mut self.stack, heap: &mut self.heap };
                dispatch_exec(op, current, &mut ctx)?
            };

            match step {
                StepResult::Continue => {
                    // 软上限 / nursery 压力下，在分配/调用等 safepoint 触发策略回收。
                    // 顶帧 local 根按保守 stack map 收窄；其余帧仍全量扫描。
                    if pressure_safepoint {
                        self.collect_active_roots_at_safepoint(module, function_index, ip_at_op, globals);
                    }
                }
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
                    let mut frame_coroutines: Vec<_> =
                        self.frames.iter().filter_map(|frame| frame.coroutine_origin).collect();
                    if let Some(coroutine_id) = finished.coroutine_origin {
                        frame_coroutines.push(coroutine_id);
                    }
                    let relocate = self.gc.collect_for_policy(
                        GcRoots {
                            stack: self.stack.values(),
                            frame_locals: &frame_locals,
                            globals,
                            frame_coroutines: &frame_coroutines,
                        },
                        &mut self.heap,
                    );
                    self.apply_relocate_map(&relocate, globals);
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
                    child.ip = 0;
                    child.set_arguments(call_args);
                    self.frames.push(child);
                }
                StepResult::Suspend { yielded_value } => {
                    // `Yield` 已由 dispatch 弹出 yielded 值并推进 ip。
                    // 此处将当前帧及其挂起时仍存活的操作数栈片段捕获为 `CoroutineState`，
                    // 存入 heap，把 `Coroutine(ObjectId)` 作为调用方的“返回值”压栈。
                    let suspended = self.frames.pop().expect("suspend without frame");
                    let operand_stack = self.stack.split_off_above(suspended.stack_base)?;
                    let state = CoroutineState {
                        function_index: suspended.function_index,
                        ip: suspended.ip,
                        locals: suspended.locals,
                        stack_base: suspended.stack_base,
                        operand_stack,
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
                    // 恢复帧快照与挂起时的操作数栈片段，记录 `coroutine_origin`，
                    // 再注入 resume_value，从挂起点之后继续执行。
                    let mut frame = Frame::new(state.function_index, 0, self.stack.len());
                    frame.ip = state.ip; // 内码指令下标
                    frame.locals = state.locals;
                    frame.coroutine_origin = Some(coroutine_id);
                    self.frames.push(frame);
                    self.stack.extend(state.operand_stack);
                    self.stack.push(resume_value);
                }
                StepResult::InvokeHandler { handler_function_index, effect_value } => {
                    // `PerformEffect` 已由 dispatch 弹出 effect_value 并推进 ip。
                    // 捕获当前帧与操作数栈片段为 continuation，再压入 handler 帧。
                    let suspended = self.frames.pop().expect("invoke_handler without frame");
                    let operand_stack = self.stack.split_off_above(suspended.stack_base)?;
                    let continuation_state = CoroutineState {
                        function_index: suspended.function_index,
                        ip: suspended.ip,
                        locals: suspended.locals,
                        stack_base: suspended.stack_base,
                        operand_stack,
                        done: false,
                        yielded_value: effect_value.clone(),
                    };
                    let continuation_id = self.heap.alloc_coroutine(continuation_state);

                    let target = &module.functions[handler_function_index];
                    let mut handler_frame =
                        Frame::new(handler_function_index, target.local_count.max(target.arity) as usize, self.stack.len());
                    handler_frame.ip = 0;
                    self.frames.push(handler_frame);

                    // 推入 handler 参数：先 continuation，再 effect_value（effect_value 在栈顶）
                    self.stack.push(Value::Coroutine(continuation_id));
                    self.stack.push(effect_value);
                }
            }
        }

        Ok(Value::Null)
    }

    /// 压力 safepoint：顶帧按 stack map 收窄 local 根后回收。
    fn collect_active_roots_at_safepoint(
        &mut self,
        module: &LoadedModule,
        function_index: usize,
        ip: u32,
        globals: &mut [Value],
    ) {
        let maps = crate::jit::stack_maps_for(module, function_index).ok();
        let entry = maps.as_ref().and_then(|m| m.entry_at(ip));

        let mut owned_precise: Vec<Vec<Value>> = Vec::new();
        let frame_count = self.frames.len();
        for (index, frame) in self.frames.iter().enumerate() {
            let is_top = index + 1 == frame_count;
            if is_top {
                if let Some(entry) = entry {
                    owned_precise.push(select_local_roots(&frame.locals, entry));
                    continue;
                }
            }
            owned_precise.push(frame.locals.clone());
        }
        let frame_locals: Vec<&[Value]> = owned_precise.iter().map(|locals| locals.as_slice()).collect();
        let frame_coroutines: Vec<_> = self.frames.iter().filter_map(|frame| frame.coroutine_origin).collect();
        let relocate = self.gc.collect_for_policy(
            GcRoots {
                stack: self.stack.values(),
                frame_locals: &frame_locals,
                globals,
                frame_coroutines: &frame_coroutines,
            },
            &mut self.heap,
        );
        self.apply_relocate_map(&relocate, globals);
    }

    /// 将 nursery 物理晋升后的转发图应用到解释器根。
    fn apply_relocate_map(&mut self, map: &nyar_gc::RelocateMap, globals: &mut [Value]) {
        if !map.has_moves() {
            return;
        }
        map.rewrite_slice(self.stack.values_mut());
        for frame in &mut self.frames {
            map.rewrite_slice(&mut frame.locals);
            if let Some(coroutine_id) = &mut frame.coroutine_origin {
                *coroutine_id = map.map(*coroutine_id);
            }
        }
        map.rewrite_slice(globals);
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
            .field("nj1_cache_len", &self.nj1_cache.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use nyar_bytecode::{
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
            layouts: Vec::new(),
        };

        let bytes = encode_module(&data);
        let module = LoadedModule::from_bytes(&bytes).expect("load module");
        let mut executor = Executor::new();
        let result = executor.run(&module, 0, Vec::new()).expect("execute");
        assert_eq!(result, Value::I32(1));
    }

    #[test]
    fn select_local_roots_keeps_only_mapped_slots() {
        let locals = vec![Value::I32(1), Value::I32(2), Value::I32(3)];
        let entry = StackMapEntry {
            instruction_index: 0,
            local_root_slots: vec![0, 2],
            operand_root_depth: None,
        };
        let selected = select_local_roots(&locals, &entry);
        assert_eq!(selected, vec![Value::I32(1), Value::I32(3)]);
    }

    #[test]
    fn concurrent_collect_exposes_root_handshake_on_executor() {
        use nyar_gc::{GcPolicy, ObjectPayload};

        let mut executor = Executor::new();
        *executor.heap_mut().policy_mut() = GcPolicy::concurrent_mark_reserved();
        let live = executor.heap_mut().alloc(ObjectPayload::LayoutObject {
            layout_id: 0,
            slots: vec![],
        });
        let stack = [Value::Object(live)];
        let _ = executor.gc.collect_for_policy(
            GcRoots {
                stack: &stack,
                frame_locals: &[],
                globals: &[],
                frame_coroutines: &[],
            },
            &mut executor.heap,
        );
        let hs = executor.last_root_handshake();
        assert_eq!(hs.stack_slots, 1);
        assert!(hs.gray_after_roots >= 1);
        let _ = executor.last_trace_poll();
    }
}
