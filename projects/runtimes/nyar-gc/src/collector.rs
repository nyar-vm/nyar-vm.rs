use crate::heap::ObjectHeap;
use crate::trace::trace_value;
use crate::value::{ObjectId, Value};

/// Root set for a mark-sweep collection.
#[derive(Debug, Clone, Copy)]
pub struct GcRoots<'a> {
    /// Operand stack values.
    pub stack: &'a [Value],
    /// Active call-frame local slots.
    pub frame_locals: &'a [&'a [Value]],
    /// Module global slots.
    pub globals: &'a [Value],
    /// Heap ids of coroutines currently being resumed by active frames.
    ///
    /// A resume frame may hold the only strong reference to its coroutine after the
    /// `Resume` opcode has already popped the coroutine value from the operand stack.
    /// Omitting these ids would allow a mid-resume collection to reclaim a still-running
    /// coroutine object.
    pub frame_coroutines: &'a [ObjectId],
}

/// Mark-sweep garbage collector.
#[derive(Debug, Default)]
pub struct GarbageCollector {
    marked: Vec<bool>,
}

impl GarbageCollector {
    /// Creates a collector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks roots and sweeps unreachable heap objects.
    pub fn collect(&mut self, roots: GcRoots<'_>, heap: &mut ObjectHeap) {
        let slot_count = heap.slot_count();
        if slot_count == 0 {
            return;
        }

        self.marked.resize(slot_count, false);
        self.marked.fill(false);

        for value in roots.stack {
            trace_value(value, heap, &mut self.marked);
        }
        for locals in roots.frame_locals {
            for value in *locals {
                trace_value(value, heap, &mut self.marked);
            }
        }
        for value in roots.globals {
            trace_value(value, heap, &mut self.marked);
        }
        for &coroutine_id in roots.frame_coroutines {
            trace_value(&Value::Coroutine(coroutine_id), heap, &mut self.marked);
        }

        heap.sweep(&self.marked);
    }
}
