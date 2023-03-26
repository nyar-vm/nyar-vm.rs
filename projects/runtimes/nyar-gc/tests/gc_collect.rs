use nyar_gc::{
    CoroutineState, GarbageCollector, GcRoots, Generation, ObjectHeap, ObjectPayload, Value,
};

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

#[test]
fn host_root_keeps_object_across_collect() {
    let mut heap = ObjectHeap::new();
    let live = heap.alloc(empty_layout(0));
    let orphan = heap.alloc(empty_layout(0));
    let handle = heap.pin_root(Value::Object(live));

    let mut gc = GarbageCollector::new();
    gc.collect(roots(&[], &[], &[], &[]), &mut heap);

    assert!(heap.get(live).is_some());
    assert!(heap.get(orphan).is_none());
    assert_eq!(heap.get_root(handle), Some(&Value::Object(live)));

    heap.unpin_root(handle);
    gc.collect(roots(&[], &[], &[], &[]), &mut heap);
    assert!(heap.get(live).is_none());
}

#[test]
fn nursery_collect_reclaims_young_keeps_tenured_and_promotes_survivors() {
    let mut heap = ObjectHeap::new();
    let tenured = heap.alloc_tenured(empty_layout(0));
    let young_live = heap.alloc(empty_layout(1));
    let young_dead = heap.alloc(empty_layout(2));
    assert_eq!(heap.generation(tenured), Some(Generation::Tenured));
    assert_eq!(heap.generation(young_live), Some(Generation::Nursery));

    let stack = [Value::Object(young_live)];
    let mut gc = GarbageCollector::new();
    gc.collect_nursery(roots(&stack, &[], &[], &[]), &mut heap);

    assert!(heap.get(tenured).is_some());
    assert!(heap.get(young_live).is_some());
    assert!(heap.get(young_dead).is_none());
    // 存活年轻代晋升
    assert_eq!(heap.generation(young_live), Some(Generation::Tenured));
    assert_eq!(heap.nursery_live_count(), 0);
}

#[test]
fn write_barrier_remembers_old_to_young_for_nursery_collect() {
    let mut heap = ObjectHeap::new();
    let old = heap.alloc_tenured(ObjectPayload::LayoutObject {
        layout_id: 0,
        slots: vec![Value::Null],
    });
    let young = heap.alloc(empty_layout(1));
    let orphan_young = heap.alloc(empty_layout(2));

    heap.set_field(old, 0, Value::Object(young)).unwrap();
    assert!(!heap.barrier().remembered_set().is_empty());

    // 无解释器根：仅靠记忆集保住 young；orphan 应回收；old 保留
    let mut gc = GarbageCollector::new();
    gc.collect_nursery(roots(&[], &[], &[], &[]), &mut heap);

    assert!(heap.get(old).is_some());
    assert!(heap.get(young).is_some());
    assert!(heap.get(orphan_young).is_none());
    assert_eq!(heap.generation(young), Some(Generation::Tenured));
    assert!(heap.barrier().remembered_set().is_empty());
}
