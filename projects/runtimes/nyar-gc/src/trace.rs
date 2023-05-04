use crate::heap::{ObjectHeap, ObjectPayload};
use crate::value::{ObjectId, Value};

/// Marks `value` and all transitively referenced heap objects（同步全闭包）。
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
            for value in &state.operand_stack {
                trace_value(value, heap, marked);
            }
        }
    }
}

/// 将值中的堆引用标灰并入队（已标记则跳过）。
pub(crate) fn enqueue_value_gray(value: &Value, marked: &mut [bool], gray: &mut Vec<ObjectId>) {
    for id in value.heap_ids() {
        enqueue_object_gray(id, marked, gray);
    }
}

/// 将对象标灰并入队。
pub(crate) fn enqueue_object_gray(id: ObjectId, marked: &mut [bool], gray: &mut Vec<ObjectId>) {
    if id >= marked.len() || marked[id] {
        return;
    }
    marked[id] = true;
    gray.push(id);
}

/// 扫描一个灰对象：子引用入灰队列（对象本身已在入队时标为已标记）。
pub(crate) fn scan_gray_object(id: ObjectId, heap: &ObjectHeap, marked: &mut [bool], gray: &mut Vec<ObjectId>) {
    let Some(payload) = heap.get(id) else {
        return;
    };
    match payload {
        ObjectPayload::LayoutObject { slots, .. } => {
            for value in slots {
                enqueue_value_gray(value, marked, gray);
            }
        }
        ObjectPayload::Coroutine(state) => {
            enqueue_value_gray(&state.yielded_value, marked, gray);
            for value in &state.locals {
                enqueue_value_gray(value, marked, gray);
            }
            for value in &state.operand_stack {
                enqueue_value_gray(value, marked, gray);
            }
        }
    }
}
