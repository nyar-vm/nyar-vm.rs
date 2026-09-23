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
}
