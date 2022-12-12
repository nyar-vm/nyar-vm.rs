use crate::{
    error::NyarRuntimeError,
    executor::Executor,
    heap::ObjectHeap,
    module::{LoadedModule, ModuleGlobals},
    value::Value,
};

/// Nyar virtual machine entry point.
#[derive(Debug, Default)]
pub struct NyarVm {
    executor: Executor,
}

impl NyarVm {
    /// Creates a VM with built-in natives.
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

    /// Registers a native handler on the underlying executor.
    pub fn register_native(&mut self, name: impl Into<String>, handler: crate::ops::NativeHandler) {
        self.executor.register_native(name, handler);
    }

    /// Borrows the executor's object heap for inspection after a run.
    ///
    /// Tests use this to read the internal state of a coroutine returned by `run`, since
    /// `Value::Coroutine` only carries a heap `ObjectId` — the `done` flag and embedded
    /// `yielded_value` live in the heap entry that the id references.
    pub fn heap(&self) -> &ObjectHeap {
        self.executor.heap()
    }
}
