use crate::{error::NyarRuntimeError, value::Value};

/// Operand stack for the interpreter.
#[derive(Debug, Default)]
pub struct ValueStack {
    slots: Vec<Value>,
}

impl ValueStack {
    /// Creates an empty stack.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of values on the stack.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether the stack is empty.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Pushes a value onto the stack.
    pub fn push(&mut self, value: Value) {
        self.slots.push(value);
    }

    /// Pops the top value.
    pub fn pop(&mut self) -> Result<Value, NyarRuntimeError> {
        self.slots.pop().ok_or(NyarRuntimeError::StackUnderflow)
    }

    /// Duplicates the top value.
    pub fn dup(&mut self) -> Result<(), NyarRuntimeError> {
        let top = self.slots.last().cloned().ok_or(NyarRuntimeError::StackUnderflow)?;
        self.slots.push(top);
        Ok(())
    }

    /// Returns the top value without popping.
    pub fn peek(&self) -> Result<&Value, NyarRuntimeError> {
        self.slots.last().ok_or(NyarRuntimeError::StackUnderflow)
    }

    /// Borrows all stack values (GC roots).
    pub fn values(&self) -> &[Value] {
        &self.slots
    }

    /// Mutably borrows stack values（晋升后改写引用）。
    pub fn values_mut(&mut self) -> &mut [Value] {
        &mut self.slots
    }

    /// Splits off every value above `base`, leaving the stack truncated to `base`.
    ///
    /// Used when capturing a suspended frame so parent frames do not observe the
    /// callee's remaining operand values, and so resume can restore them later.
    pub fn split_off_above(&mut self, base: usize) -> Result<Vec<Value>, NyarRuntimeError> {
        if base > self.slots.len() {
            return Err(NyarRuntimeError::StackUnderflow);
        }
        Ok(self.slots.split_off(base))
    }

    /// Appends previously captured operand values (for coroutine resume).
    pub fn extend(&mut self, values: impl IntoIterator<Item = Value>) {
        self.slots.extend(values);
    }
}
