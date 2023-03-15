use crate::heap::{ObjectHeap, ObjectPayload};
use crate::value::{ObjectId, Value};

/// Marks `value` and all transitively referenced heap objects.
pub(crate) fn trace_value(value: &Value, heap: &ObjectHeap, marked: &mut [bool]) {
    for id in value.heap_ids() {
        trace_object(id, heap, marked);
    }
}

fn trace_object(id: ObjectId, heap: &ObjectHeap, marked: &mut [bool]) {
    if id >= marked.len() || marked[id] {
        return;
    }
    marked[id] = true;
    if let Some(payload) = heap.get(id) {
        trace_payload(payload, heap, marked);
    }
}

fn trace_payload(payload: &ObjectPayload, heap: &ObjectHeap, marked: &mut [bool]) {
    match payload {
        ObjectPayload::LayoutObject { slots, .. } => {
            for value in slots {
                trace_value(value, heap, marked);
            }
        }
        ObjectPayload::Coroutine(state) => {
            trace_value(&state.yielded_value, heap, marked);
            for value in &state.locals {
                trace_value(value, heap, marked);
            }
        }
    }
}
