use crate::{
    error::NyarRuntimeError,
    executor::Executor,
    jit::{JitCompiledArtifact, JitCompiler, JitError},
    module::{LoadedModule, ModuleGlobals},
    value::Value,
};
use nyar_gc::ObjectHeap;

/// Nyar virtual machine entry point.
#[derive(Debug, Default)]
pub struct NyarVm {
    executor: Executor,
}

impl NyarVm {
    /// Creates a VM. Host imports dispatch via [`crate::host::HostOp`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads a `.nyar` module from bytes.
    pub fn load(&self, bytes: &[u8]) -> Result<LoadedModule, NyarRuntimeError> {
        LoadedModule::from_bytes(bytes)
    }

    /// Runs an exported function with arguments.
    pub fn run(&mut self, module: &LoadedModule, entry: &str, args: Vec<Value>) -> Result<Value, NyarRuntimeError> {
        let mut globals = ModuleGlobals::new(module);
        self.run_with_globals(module, &mut globals, entry, args)
    }

    /// Runs an exported function while preserving module global slots (singleton instances).
    pub fn run_with_globals(
        &mut self,
        module: &LoadedModule,
        globals: &mut ModuleGlobals,
        entry: &str,
        args: Vec<Value>,
    ) -> Result<Value, NyarRuntimeError> {
        if !globals.init_done() {
            for &init_index in &module.init_function_indices {
                let init_index = init_index as usize;
                self.executor.run_function_frame(module, init_index, Vec::new(), globals.slots_mut())?;
                self.executor.reset_after_nested_run();
            }
            globals.mark_init_done();
        }

        let function_index = module.export_index(entry).ok_or_else(|| NyarRuntimeError::EntryNotFound(entry.to_string()))?;
        self.executor.run_function_frame(module, function_index, args, globals.slots_mut())
    }

    /// Whether the installed JIT backend is enabled.
    pub fn jit_enabled(&self) -> bool {
        self.executor.jit_enabled()
    }

    /// Attempts JIT compilation for one module function.
    pub fn try_jit_compile(&mut self, module: &LoadedModule, function_index: usize) -> Result<JitCompiledArtifact, JitError> {
        self.executor.try_jit_compile(module, function_index)
    }

    /// Replaces the JIT backend on the underlying executor.
    pub fn set_jit(&mut self, jit: Box<dyn JitCompiler>) {
        self.executor.set_jit(jit);
    }

    /// Borrows the executor's object heap for inspection after a run.
    ///
    /// Tests use this to read the internal state of a coroutine returned by `run`, since
    /// `Value::Coroutine` only carries a heap `ObjectId` — the `done` flag and embedded
    /// `yielded_value` live in the heap entry that the id references.
    pub fn heap(&self) -> &ObjectHeap {
        self.executor.heap()
    }

    /// Mutably borrows the object heap（宿主根 / 策略调整）。
    pub fn heap_mut(&mut self) -> &mut ObjectHeap {
        self.executor.heap_mut()
    }

    /// 设置堆级 GC 策略（立即生效于后续 safepoint）。
    pub fn set_gc_policy(&mut self, policy: nyar_gc::GcPolicy) {
        self.executor.heap_mut().set_policy(policy);
    }

    /// 更新工作负载提示（软上限、暂停预算等），不改 GC 模式。
    pub fn apply_workload_hints(&mut self, hints: nyar_gc::WorkloadHints) {
        self.executor.heap_mut().policy_mut().apply_hints(hints);
    }

    /// 应用进程级工作负载意图并刷新 GC 策略。
    pub fn apply_workload_intent(
        &mut self,
        intent: nyar_gc::WorkloadIntent,
    ) -> Result<nyar_gc::StrategyDecision, nyar_gc::IntentError> {
        self.executor.heap_mut().apply_intent(intent)
    }

    /// 进入业务阶段（嵌套）并刷新策略。
    pub fn begin_workload_phase(
        &mut self,
        intent: nyar_gc::WorkloadIntent,
    ) -> Result<nyar_gc::StrategyDecision, nyar_gc::IntentError> {
        self.executor.heap_mut().begin_phase(intent)
    }

    /// 结束业务阶段并刷新策略。
    pub fn end_workload_phase(
        &mut self,
        phase: Option<&str>,
    ) -> Result<nyar_gc::StrategyDecision, nyar_gc::IntentError> {
        self.executor.heap_mut().end_phase(phase)
    }

    /// 最近一次策略决策（若有）。
    pub fn last_strategy_decision(&self) -> Option<&nyar_gc::StrategyDecision> {
        self.executor.heap().last_strategy_decision()
    }

    /// 将值固定为宿主根，跨 `run` / `collect` 保持可达。
    pub fn pin_root(&mut self, value: Value) -> nyar_gc::RootHandle {
        self.executor.heap_mut().pin_root(value)
    }

    /// 释放宿主根。
    pub fn unpin_root(&mut self, handle: nyar_gc::RootHandle) {
        self.executor.heap_mut().unpin_root(handle);
    }

    /// 读取宿主根当前值。
    pub fn get_root(&self, handle: nyar_gc::RootHandle) -> Option<&Value> {
        self.executor.heap().get_root(handle)
    }
}
