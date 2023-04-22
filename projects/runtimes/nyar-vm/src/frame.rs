use nyar_jit::{RestoredInterpreterFrame, RestoredLocal};

use crate::{error::NyarRuntimeError, value::{ObjectId, Value}};

/// Activation frame for one function invocation.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Local variable slots.
    pub locals: Vec<Value>,
    /// 内码指令下标（相对当前函数的 [`crate::executable::ExecutableFunction::ops`]）。
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

    /// 由 deopt 物化帧构造解释器帧（`Absent` → `Null`；`Provided` 经 [`crate::deopt_value`] 解码）。
    pub fn from_deopt_restore(restored: &RestoredInterpreterFrame, stack_base: usize) -> Result<Self, NyarRuntimeError> {
        let mut locals = Vec::with_capacity(restored.locals.len());
        for cell in &restored.locals {
            match cell {
                RestoredLocal::Absent => locals.push(Value::Null),
                RestoredLocal::Provided(bytes) => {
                    locals.push(crate::deopt_value::decode_value_from_deopt(bytes)?);
                }
            }
        }
        Ok(Self {
            locals,
            ip: restored.instruction_index as usize,
            function_index: restored.function_index,
            stack_base,
            coroutine_origin: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nyar_jit::{build_baseline_deopt_map, materialize_interpreter_frames};

    #[test]
    fn from_deopt_restore_maps_absent_to_null() {
        let map = build_baseline_deopt_map(4, 2, &[7]);
        let entry = map.entry_at(7).expect("entry");
        let frames = materialize_interpreter_frames(entry, &[vec![]]).expect("materialize");
        let frame = Frame::from_deopt_restore(&frames[0], 0).expect("restore");
        assert_eq!(frame.function_index, 4);
        assert_eq!(frame.ip, 8);
        assert_eq!(frame.locals, vec![Value::Null, Value::Null]);
    }

    #[test]
    fn from_deopt_restore_decodes_provided_i32() {
        let map = build_baseline_deopt_map(0, 1, &[0]);
        let entry = map.entry_at(0).expect("entry");
        let payload = crate::deopt_value::encode_value_for_deopt(&Value::I32(42)).expect("encode");
        let frames = materialize_interpreter_frames(entry, &[vec![Some(payload)]]).expect("materialize");
        let frame = Frame::from_deopt_restore(&frames[0], 0).expect("restore");
        assert_eq!(frame.locals, vec![Value::I32(42)]);
    }
}
