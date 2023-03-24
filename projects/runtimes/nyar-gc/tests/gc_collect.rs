use nyar_gc::{CoroutineState, GcRoots, GarbageCollector, ObjectHeap, ObjectPayload, Value};

fn empty_layout(layout_id: u32) -> ObjectPayload {
    ObjectPayload::LayoutObject { layout_id, slots: vec![] }
}

fn roots<'a>(
    stack: &'a [Value],
    frame_locals: &'a [&'a [Value]],
    globals: &'a [Value],
    frame_coroutines: &'a [usize],
) -> GcRoots<'a> {
    GcRoots { stack, frame_locals, globals, frame_coroutines }
}

#[test]
fn collect_reclaims_unreachable_objects() {
    let mut heap = ObjectHeap::new();
    let live = heap.alloc(empty_layout(0));
    let dead = heap.alloc(empty_layout(0));
    assert_eq!(heap.live_count(), 2);

    let stack = [Value::Object(live)];
    let mut gc = GarbageCollector::new();
    gc.collect(roots(&stack, &[], &[], &[]), &mut heap);

    assert!(heap.get(live).is_some());
    assert!(heap.get(dead).is_none());
    assert_eq!(heap.live_count(), 1);
}

#[test]
fn collect_traces_nested_layout_references() {
    let mut heap = ObjectHeap::new();
    let inner = heap.alloc(empty_layout(0));
    let outer = heap.alloc(ObjectPayload::LayoutObject {
        layout_id: 1,
        slots: vec![Value::Object(inner)],
    });
    let orphan = heap.alloc(empty_layout(0));

    let stack = [Value::Object(outer)];
    let mut gc = GarbageCollector::new();
    gc.collect(roots(&stack, &[], &[], &[]), &mut heap);

    assert!(heap.get(outer).is_some());
    assert!(heap.get(inner).is_some());
    assert!(heap.get(orphan).is_none());
}

#[test]
fn collect_reuses_freed_slots_via_free_list() {
    let mut heap = ObjectHeap::new();
    let dead = heap.alloc(empty_layout(0));
    let mut gc = GarbageCollector::new();
    gc.collect(roots(&[], &[], &[], &[]), &mut heap);
    assert!(heap.get(dead).is_none());
    assert_eq!(heap.free_list_len(), 1);

    let reused = heap.alloc(empty_layout(0));
    assert_eq!(reused, dead);
    assert_eq!(heap.free_list_len(), 0);
}

#[test]
fn collect_keeps_coroutine_reachable_only_via_frame_origin() {
    // 模拟 Resume 之后：操作数栈上已无 Coroutine 值，唯一存活引用在 frame_coroutines。
    let mut heap = ObjectHeap::new();
    let kept = heap.alloc(empty_layout(0));
    let coroutine_id = heap.alloc_coroutine(CoroutineState {
        function_index: 0,
        ip: 0,
        locals: vec![Value::Object(kept)],
        stack_base: 0,
        operand_stack: Vec::new(),
        done: false,
        yielded_value: Value::Null,
    });
    let orphan = heap.alloc(empty_layout(0));

    let frame_coroutines = [coroutine_id];
    let mut gc = GarbageCollector::new();
    gc.collect(roots(&[], &[], &[], &frame_coroutines), &mut heap);

    assert!(heap.get(coroutine_id).is_some());
    assert!(heap.get(kept).is_some());
    assert!(heap.get(orphan).is_none());
}

#[test]
fn collect_traces_suspended_operand_stack_values() {
    // 挂起帧上仍存活的操作数必须成为根，否则仅被该片段引用的对象会被误回收。
    let mut heap = ObjectHeap::new();
    let live = heap.alloc(empty_layout(0));
    let coroutine_id = heap.alloc_coroutine(CoroutineState {
        function_index: 0,
        ip: 0,
        locals: Vec::new(),
        stack_base: 0,
        operand_stack: vec![Value::Object(live)],
        done: false,
        yielded_value: Value::Null,
    });
    let orphan = heap.alloc(empty_layout(0));

    let stack = [Value::Coroutine(coroutine_id)];
    let mut gc = GarbageCollector::new();
    gc.collect(roots(&stack, &[], &[], &[]), &mut heap);

    assert!(heap.get(coroutine_id).is_some());
    assert!(heap.get(live).is_some());
    assert!(heap.get(orphan).is_none());
}
