use nyar_gc::{GcRoots, GarbageCollector, ObjectHeap, ObjectPayload, Value};

#[test]
fn collect_reclaims_unreachable_objects() {
    let mut heap = ObjectHeap::new();
    let live = heap.alloc(ObjectPayload::Record(vec![]));
    let dead = heap.alloc(ObjectPayload::Record(vec![]));
    assert_eq!(heap.live_count(), 2);

    let roots = [Value::Object(live)];
    let mut gc = GarbageCollector::new();
    gc.collect(GcRoots { stack: &roots, frame_locals: &[], globals: &[] }, &mut heap);

    assert!(heap.get(live).is_some());
    assert!(heap.get(dead).is_none());
    assert_eq!(heap.live_count(), 1);
}

#[test]
fn collect_traces_nested_record_references() {
    let mut heap = ObjectHeap::new();
    let inner = heap.alloc(ObjectPayload::Record(vec![]));
    let outer = heap.alloc(ObjectPayload::Record(vec![("child".to_string(), Value::Object(inner))]));
    let orphan = heap.alloc(ObjectPayload::Record(vec![]));

    let roots = [Value::Object(outer)];
    let mut gc = GarbageCollector::new();
    gc.collect(GcRoots { stack: &roots, frame_locals: &[], globals: &[] }, &mut heap);

    assert!(heap.get(outer).is_some());
    assert!(heap.get(inner).is_some());
    assert!(heap.get(orphan).is_none());
}

#[test]
fn collect_reuses_freed_slots() {
    let mut heap = ObjectHeap::new();
    let dead = heap.alloc(ObjectPayload::Record(vec![]));
    let mut gc = GarbageCollector::new();
    gc.collect(GcRoots { stack: &[], frame_locals: &[], globals: &[] }, &mut heap);
    assert!(heap.get(dead).is_none());

    let reused = heap.alloc(ObjectPayload::Record(vec![]));
    assert_eq!(reused, dead);
}
