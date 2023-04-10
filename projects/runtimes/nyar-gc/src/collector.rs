use crate::generation::Generation;
use crate::heap::ObjectHeap;
use crate::relocate::RelocateMap;
use crate::trace::trace_value;
use crate::value::{ObjectId, Value};

/// Root set for a mark-sweep collection（不含宿主根；宿主根始终从堆内读取）。
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

/// Mark-sweep garbage collector（全堆 + nursery + 策略入口）。
#[derive(Debug, Default)]
pub struct GarbageCollector {
    marked: Vec<bool>,
    /// 自上次全堆回收以来已完成的 nursery 次数（分代模式）。
    nursery_collects_since_full: u32,
    /// 最近一次 nursery 晋升转发图（供解释器改写根）。
    last_relocate: RelocateMap,
}

impl GarbageCollector {
    /// Creates a collector.
    pub fn new() -> Self {
        Self::default()
    }

    /// 最近一次 nursery 晋升转发图。
    pub fn last_relocate_map(&self) -> &RelocateMap {
        &self.last_relocate
    }

    /// 按堆上 [`crate::GcPolicy`] 选择 nursery 或全堆回收。
    ///
    /// 若发生 nursery 物理晋升，返回转发图；全堆回收返回空图。
    pub fn collect_for_policy(&mut self, roots: GcRoots<'_>, heap: &mut ObjectHeap) -> RelocateMap {
        use crate::policy::GcMode;

        let mode = heap.policy().mode;
        let force_full = heap.policy().hints.allow_heavy_collection
            || heap.over_soft_limit()
            || self.nursery_collects_since_full >= heap.policy().full_collect_every_n_nursery;
        // nursery 软容量压力优先走年轻代回收（仍受 force_full 约束）。
        let prefer_nursery = matches!(mode, GcMode::GenerationalLowLatency) && !force_full;

        if prefer_nursery {
            let map = self.collect_nursery(roots, heap);
            self.nursery_collects_since_full = self.nursery_collects_since_full.saturating_add(1);
            map
        } else {
            self.collect(roots, heap);
            self.nursery_collects_since_full = 0;
            RelocateMap::new()
        }
    }

    /// 全堆标记清扫：根 + 宿主根；回收后清空记忆集。
    pub fn collect(&mut self, roots: GcRoots<'_>, heap: &mut ObjectHeap) {
        let slot_count = heap.slot_count();
        if slot_count == 0 {
            return;
        }

        self.marked.resize(slot_count, false);
        self.marked.fill(false);
        self.mark_interpreter_roots(roots, heap);
        self.mark_host_roots(heap);

        heap.sweep(&self.marked);
        heap.clear_remembered();
        self.last_relocate = RelocateMap::new();
    }

    /// Nursery 回收：根 + 宿主根 + 记忆集；清扫死亡年轻代；存活者物理晋升到老年代。
    pub fn collect_nursery(&mut self, roots: GcRoots<'_>, heap: &mut ObjectHeap) -> RelocateMap {
        let slot_count = heap.slot_count();
        if slot_count == 0 {
            return RelocateMap::new();
        }

        self.marked.resize(slot_count, false);
        self.marked.fill(false);
        self.mark_interpreter_roots(roots, heap);
        self.mark_host_roots(heap);

        let remembered: Vec<ObjectId> = heap.remembered_set().to_vec();
        for container in remembered {
            if heap.generation(container) == Some(Generation::Tenured) {
                trace_value(&Value::Object(container), heap, &mut self.marked);
            }
        }

        let survivor_count = heap.count_marked_nursery(&self.marked);
        if let Some(failure) = heap.promotion_would_fail(survivor_count) {
            // 晋升会突破老年代软容量：记录事件并回退到全堆回收，不得丢弃可达对象。
            heap.record_promotion_failure(failure);
            heap.sweep(&self.marked);
            heap.clear_remembered();
            self.last_relocate = RelocateMap::new();
            self.nursery_collects_since_full = 0;
            return RelocateMap::new();
        }

        heap.sweep_nursery(&self.marked);
        let map = heap.promote_marked_nursery_moving(&self.marked);
        heap.clear_remembered();
        self.last_relocate = map.clone();
        map
    }

    fn mark_interpreter_roots(&mut self, roots: GcRoots<'_>, heap: &ObjectHeap) {
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
    }

    fn mark_host_roots(&mut self, heap: &ObjectHeap) {
        let pinned: Vec<Value> = heap.host_roots().iter().cloned().collect();
        for value in &pinned {
            trace_value(value, heap, &mut self.marked);
        }
    }
}
