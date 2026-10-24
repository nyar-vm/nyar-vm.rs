//! Algebraic control-flow and builtin helpers.

mod core_evaluator;

pub use core_evaluator::CoreEvaluator;

use std::sync::Arc;

use crate::value::LegacyValue;

/// Loop `break` sentinel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BreakValue;

/// Loop `continue` sentinel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContinueValue;

/// Function `return` sentinel.
#[derive(Debug, Clone, PartialEq)]
pub struct ReturnValue {
    /// Returned payload.
    pub value: LegacyValue,
}

impl ReturnValue {
    /// Wrap a runtime value.
    pub fn new(value: LegacyValue) -> Self {
        Self { value }
    }
}

/// Builtin callable wrapper.
#[derive(Clone)]
pub struct BuiltinFunction {
    /// Function name.
    pub name: String,
    /// Implementation.
    pub implementation: Arc<dyn Fn(&[LegacyValue]) -> LegacyValue + Send + Sync>,
}

impl BuiltinFunction {
    /// Create a builtin wrapper.
    pub fn new(name: impl Into<String>, implementation: impl Fn(&[LegacyValue]) -> LegacyValue + Send + Sync + 'static) -> Self {
        Self { name: name.into(), implementation: Arc::new(implementation) }
    }

    /// Invoke the builtin.
    pub fn invoke(&self, args: &[LegacyValue]) -> LegacyValue {
        (self.implementation)(args)
    }
}

impl std::fmt::Debug for BuiltinFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BuiltinFunction").field("name", &self.name).finish()
    }
}
