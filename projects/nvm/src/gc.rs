use crate::{heap::ObjectHeap, stack::ValueStack, value::Value};

/// Mark-sweep GC stub. Currently a no-op collector hook.
#[derive(Debug, Default)]
pub struct GarbageCollector;

impl GarbageCollector {
    /// Creates a GC stub.
    pub fn new() -> Self {
        Self
    }

    /// Marks stack and locals as roots, then sweeps unreachable heap objects.
    pub fn collect(&mut self, _stack: &ValueStack, _locals: &[Value], _heap: &mut ObjectHeap) {}
}
