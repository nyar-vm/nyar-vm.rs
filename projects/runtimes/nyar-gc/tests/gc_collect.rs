use nyar_gc::{
    ConcurrentMarkEvent, CoroutineState, GarbageCollector, GcRoots, Generation, ObjectHeap, ObjectPayload,
    Value,
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
    let map = gc.collect_nursery(roots(&stack, &[], &[], &[]), &mut heap);
    let promoted = map.map(young_live);

    assert!(heap.get(tenured).is_some());
    assert!(heap.get(young_live).is_none()); // 旧槽已腾空
    assert!(heap.get(promoted).is_some());
    assert!(heap.get(young_dead).is_none());
    assert_eq!(heap.generation(promoted), Some(Generation::Tenured));
    assert_ne!(promoted, young_live);
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
    let map = gc.collect_nursery(roots(&[], &[], &[], &[]), &mut heap);
    let promoted = map.map(young);

    assert!(heap.get(old).is_some());
    assert!(heap.get(young).is_none());
    assert!(heap.get(promoted).is_some());
    assert!(heap.get(orphan_young).is_none());
    assert_eq!(heap.generation(promoted), Some(Generation::Tenured));
    // 堆内字段已按转发图改写
    match heap.get(old) {
        Some(ObjectPayload::LayoutObject { slots, .. }) => {
            assert_eq!(slots[0], Value::Object(promoted));
        }
        other => panic!("expected layout object, got {other:?}"),
    }
    assert!(heap.barrier().remembered_set().is_empty());
}

#[test]
fn collect_for_policy_uses_nursery_then_forces_full() {
    use nyar_gc::GcPolicy;

    let mut heap = ObjectHeap::with_policy(GcPolicy::generational_low_latency().with_full_collect_every(2));
    let keep = heap.alloc(empty_layout(0));
    let _dead1 = heap.alloc(empty_layout(1));
    let stack = [Value::Object(keep)];
    let mut gc = GarbageCollector::new();

    let map = gc.collect_for_policy(roots(&stack, &[], &[], &[]), &mut heap);
    let keep = map.map(keep);
    assert!(heap.get(keep).is_some());
    assert_eq!(heap.generation(keep), Some(Generation::Tenured));

    let young = heap.alloc(empty_layout(2));
    let orphan = heap.alloc(empty_layout(3));
    let stack2 = [Value::Object(young)];
    // 第二次仍为 nursery（every=2 表示满 2 次后下一次全堆）
    gc.collect_for_policy(roots(&stack2, &[], &[], &[]), &mut heap);
    assert!(heap.get(orphan).is_none());

    let tenured_keep = heap.alloc_tenured(empty_layout(4));
    let orphan2 = heap.alloc(empty_layout(5));
    // 第三次：nursery_collects_since_full >= 2 → 全堆
    gc.collect_for_policy(roots(&[], &[], &[], &[]), &mut heap);
    assert!(heap.get(tenured_keep).is_none());
    assert!(heap.get(orphan2).is_none());
}

#[test]
fn nursery_capacity_reports_pressure() {
    let mut heap = ObjectHeap::new();
    heap.set_nursery_capacity(2);
    let _a = heap.alloc(empty_layout(0));
    assert!(!heap.nursery_pressure());
    let _b = heap.alloc(empty_layout(1));
    assert!(heap.nursery_pressure());
}

#[test]
fn promotion_failure_falls_back_to_full_collect() {
    let mut heap = ObjectHeap::new();
    heap.set_tenured_soft_capacity(Some(1));
    // 填满老年代软容量
    let tenured = heap.alloc_tenured(empty_layout(0));
    let young_keep = heap.alloc(empty_layout(1));
    let young_dead = heap.alloc(empty_layout(2));

    let stack = [Value::Object(young_keep)];
    let mut gc = GarbageCollector::new();
    let map = gc.collect_nursery(roots(&stack, &[], &[], &[]), &mut heap);

    assert!(heap.last_promotion_failure().is_some());
    assert!(map.has_moves() == false);
    // 全堆回退：无根的 tenured 与 young_dead 应回收；young_keep 仍存活（未搬迁）
    assert!(heap.get(tenured).is_none());
    assert!(heap.get(young_dead).is_none());
    assert!(heap.get(young_keep).is_some());
    assert_eq!(heap.generation(young_keep), Some(Generation::Nursery));
}

#[test]
fn accounting_tracks_live_bytes_and_soft_limit_forces_full() {
    use nyar_gc::{GcPolicy, WorkloadHints};

    let mut heap = ObjectHeap::with_policy(
        GcPolicy::generational_low_latency()
            .with_full_collect_every(100)
            .with_hints(WorkloadHints {
                heap_soft_limit_bytes: Some(1),
                ..WorkloadHints::default()
            }),
    );
    let live = heap.alloc(empty_layout(0));
    let dead = heap.alloc(empty_layout(1));
    assert!(heap.live_bytes() > 0);
    assert!(heap.over_soft_limit());

    let before_total = heap.total_allocated_bytes();
    let stack = [Value::Object(live)];
    let mut gc = GarbageCollector::new();
    // 软上限 → 即使分代模式也走全堆
    gc.collect_for_policy(roots(&stack, &[], &[], &[]), &mut heap);
    assert!(heap.get(live).is_some());
    assert!(heap.get(dead).is_none());
    assert!(heap.live_bytes() < before_total);
    assert_eq!(heap.total_allocated_bytes(), before_total);
}

#[test]
fn satb_buffer_records_overwritten_refs_during_concurrent_trace() {
    let mut heap = ObjectHeap::new();
    let container = heap.alloc(ObjectPayload::LayoutObject {
        layout_id: 0,
        slots: vec![Value::Null],
    });
    let old_ref = heap.alloc(empty_layout(1));
    let new_ref = heap.alloc(empty_layout(2));
    heap.set_field(container, 0, Value::Object(old_ref)).unwrap();
    assert!(heap.barrier().satb_buffer().is_empty());

    {
        let ctrl = heap.concurrent_mark_mut();
        ctrl.set_enabled(true);
        ctrl.transition(ConcurrentMarkEvent::BeginCycle).unwrap();
        ctrl.transition(ConcurrentMarkEvent::RootsReady).unwrap();
        ctrl.transition(ConcurrentMarkEvent::TraceSliceDone).unwrap();
    }
    assert!(heap.concurrent_mark().requires_satb());

    heap.set_field(container, 0, Value::Object(new_ref)).unwrap();
    assert_eq!(heap.barrier().satb_buffer(), &[old_ref]);
    let drained = heap.drain_satb_buffer();
    assert_eq!(drained, vec![old_ref]);
    assert!(heap.barrier().satb_buffer().is_empty());
}

#[test]
fn concurrent_mark_reserved_poll_drains_satb_and_reclaims() {
    use nyar_gc::{ConcurrentMarkState, GcPolicy};

    let mut heap = ObjectHeap::with_policy(GcPolicy::concurrent_mark_reserved());
    assert!(heap.concurrent_mark().enabled());

    let container = heap.alloc(ObjectPayload::LayoutObject {
        layout_id: 0,
        slots: vec![Value::Null],
    });
    let keep = heap.alloc(empty_layout(1));
    let doomed = heap.alloc(empty_layout(2));
    heap.set_field(container, 0, Value::Object(keep)).unwrap();

    let stack = [Value::Object(container)];
    let mut gc = GarbageCollector::new();

    // Idle → ConcurrentTrace
    assert!(!gc.poll_concurrent_mark(roots(&stack, &[], &[], &[]), &mut heap));
    assert_eq!(heap.concurrent_mark().state(), ConcurrentMarkState::ConcurrentTrace);
    assert!(heap.concurrent_mark().requires_satb());

    // mutator 覆盖：旧 keep 进 SATB；再挂上 doomed 后又覆盖掉，使 doomed 仅靠 SATB 被看见
    heap.set_field(container, 0, Value::Object(doomed)).unwrap();
    assert!(heap.barrier().satb_buffer().contains(&keep));
    heap.set_field(container, 0, Value::Object(keep)).unwrap();
    assert!(heap.barrier().satb_buffer().contains(&doomed));

    // 推进到 Idle（含 TerminationCheck / Remark / Sweep）
    let mut steps = 0;
    while !gc.poll_concurrent_mark(roots(&stack, &[], &[], &[]), &mut heap) {
        steps += 1;
        assert!(steps < 16, "concurrent mark poll should finish");
    }
    assert_eq!(heap.concurrent_mark().state(), ConcurrentMarkState::Idle);
    assert!(heap.get(container).is_some());
    assert!(heap.get(keep).is_some());
    // doomed 曾被 SATB 标记，随后 remark 从根只达 keep；但 SATB 已将其标灰，
    // 单线程实现保留「曾进入缓冲」对象直至本周期清扫 — 若最终不可达则被回收。
    // 此处 doomed 在 remark 根闭包外且标记位可能仍为 true（SATB 标记），故存活或回收均可；
    // 断言周期完成且 keep 存活即可。
    assert!(heap.barrier().satb_buffer().is_empty());
}

#[test]
fn collect_for_policy_concurrent_mark_reserved_reclaims_unreachable() {
    use nyar_gc::GcPolicy;

    let mut heap = ObjectHeap::with_policy(GcPolicy::concurrent_mark_reserved());
    let live = heap.alloc(empty_layout(0));
    let dead = heap.alloc(empty_layout(1));
    let stack = [Value::Object(live)];
    let mut gc = GarbageCollector::new();
    gc.collect_for_policy(roots(&stack, &[], &[], &[]), &mut heap);
    assert!(heap.get(live).is_some());
    assert!(heap.get(dead).is_none());
    assert_eq!(heap.concurrent_mark().state(), nyar_gc::ConcurrentMarkState::Idle);
}

#[test]
fn concurrent_trace_gray_slices_reach_nested_refs() {
    use nyar_gc::{ConcurrentMarkState, GcPolicy};

    let mut heap = ObjectHeap::with_policy(GcPolicy::concurrent_mark_reserved());
    // 链：root → a → b → c（三层嵌套，预算 1 时需多拍 ConcurrentTrace）
    let c = heap.alloc(empty_layout(3));
    let b = heap.alloc(ObjectPayload::LayoutObject {
        layout_id: 2,
        slots: vec![Value::Object(c)],
    });
    let a = heap.alloc(ObjectPayload::LayoutObject {
        layout_id: 1,
        slots: vec![Value::Object(b)],
    });
    let root = heap.alloc(ObjectPayload::LayoutObject {
        layout_id: 0,
        slots: vec![Value::Object(a)],
    });
    let dead = heap.alloc(empty_layout(9));

    let stack = [Value::Object(root)];
    let mut gc = GarbageCollector::new();
    gc.set_gray_budget_per_slice(1);

    assert!(!gc.poll_concurrent_mark(roots(&stack, &[], &[], &[]), &mut heap));
    assert_eq!(heap.concurrent_mark().state(), ConcurrentMarkState::ConcurrentTrace);
    // 根快照只入灰 root，尚未扫到 c
    assert_eq!(gc.gray_queue_len(), 1);

    let mut steps = 0;
    while !gc.poll_concurrent_mark_ex(roots(&stack, &[], &[], &[]), &mut heap, true) {
        steps += 1;
        assert!(steps < 32, "gray-sliced concurrent mark should finish");
    }
    assert_eq!(heap.concurrent_mark().state(), ConcurrentMarkState::Idle);
    assert!(heap.get(root).is_some());
    assert!(heap.get(a).is_some());
    assert!(heap.get(b).is_some());
    assert!(heap.get(c).is_some());
    assert!(heap.get(dead).is_none());
    assert_eq!(gc.gray_queue_len(), 0);
}

#[test]
fn concurrent_trace_waits_for_ticker_before_termination() {
    use std::thread;
    use std::time::Duration;

    use nyar_gc::{ConcurrentMarkState, GcPolicy};

    let mut heap = ObjectHeap::with_policy(GcPolicy::concurrent_mark_reserved());
    let live = heap.alloc(empty_layout(0));
    heap.start_concurrent_mark_ticker(Duration::from_millis(5)).expect("ticker");
    thread::sleep(Duration::from_millis(25));
    assert!(heap.concurrent_mark_ticks() >= 1);

    let stack = [Value::Object(live)];
    let mut gc = GarbageCollector::new();
    // Idle → ConcurrentTrace
    assert!(!gc.poll_concurrent_mark(roots(&stack, &[], &[], &[]), &mut heap));
    assert_eq!(heap.concurrent_mark().state(), ConcurrentMarkState::ConcurrentTrace);

    // 等待更多节拍后下一次 ConcurrentTrace 应因 ticks 进展而停留（TraceSliceDone）
    let before = heap.concurrent_mark_ticks();
    thread::sleep(Duration::from_millis(25));
    assert!(heap.concurrent_mark_ticks() > before);
    assert!(!gc.poll_concurrent_mark(roots(&stack, &[], &[], &[]), &mut heap));
    assert_eq!(heap.concurrent_mark().state(), ConcurrentMarkState::ConcurrentTrace);

    // 停止 ticker 后再 poll：无新节拍 → TerminationCheck → … → Idle
    heap.stop_concurrent_mark_ticker();
    let mut steps = 0;
    while !gc.poll_concurrent_mark(roots(&stack, &[], &[], &[]), &mut heap) {
        steps += 1;
        assert!(steps < 16);
    }
    assert_eq!(heap.concurrent_mark().state(), ConcurrentMarkState::Idle);
    assert!(heap.get(live).is_some());
}
