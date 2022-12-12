use crate::value::{ObjectId, Value};

/// Activation frame for one function invocation.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Local variable slots.
    pub locals: Vec<Value>,
    /// Instruction pointer into the module code section.
    pub ip: usize,
    /// Index into the module function table.
    pub function_index: usize,
    /// Stack depth when this frame was entered.
    pub stack_base: usize,
    /// Heap id of the coroutine this frame is resuming, if any.
    ///
    /// Set by the executor when `StepResult::ResumeCoroutine` pushes a fresh frame to
    /// restore a suspended coroutine. When this frame later reaches `Return`, the
    /// executor inspects this field: if `Some(id)`, the coroutine's heap entry is
    /// marked `done = true` and its `yielded_value` is overwritten with the final
    /// return value, so any remaining stack/local copies of the coroutine observe
    /// completion and reject further `Resume` attempts. `None` for ordinary call
    /// frames that did not originate from a coroutine resume.
    pub coroutine_origin: Option<ObjectId>,
}

impl Frame {
    /// Creates a new frame for `function_index` with `local_count` slots.
    pub fn new(function_index: usize, local_count: usize, stack_base: usize) -> Self {
        Self { locals: vec![Value::Null; local_count], ip: 0, function_index, stack_base, coroutine_origin: None }
    }

    /// Stores call arguments into the first `args.len()` locals.
    pub fn set_arguments(&mut self, args: Vec<Value>) {
        for (index, value) in args.into_iter().enumerate() {
            if index < self.locals.len() {
                self.locals[index] = value;
            }
        }
    }
}
