use crate::generation::Generation;
use crate::heap::ObjectHeap;
use crate::relocate::RelocateMap;
use crate::trace::{enqueue_object_gray, enqueue_value_gray, scan_gray_object, trace_value};
use crate::value::{ObjectId, Value};

/// ConcurrentTrace 每拍默认扫描的灰对象上限（mutator 侧有界切片）。
const DEFAULT_GRAY_BUDGET_PER_SLICE: usize = 64;

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
#[derive(Debug)]
pub struct GarbageCollector {
    marked: Vec<bool>,
    /// 并发标记灰对象工作队列（已标记、待扫描子引用）。
    gray: Vec<ObjectId>,
    /// ConcurrentTrace 每拍灰扫描预算。
    gray_budget_per_slice: usize,
    /// 自上次全堆回收以来已完成的 nursery 次数（分代模式）。
    nursery_collects_since_full: u32,
    /// 最近一次 nursery 晋升转发图（供解释器改写根）。
    last_relocate: RelocateMap,
    /// 进入 / 停留 ConcurrentTrace 时观察到的 ticker 计数。
    last_concurrent_ticks: u64,
}

impl Default for GarbageCollector {
    fn default() -> Self {
        Self {
            marked: Vec::new(),
            gray: Vec::new(),
            gray_budget_per_slice: DEFAULT_GRAY_BUDGET_PER_SLICE,
            nursery_collects_since_full: 0,
            last_relocate: RelocateMap::new(),
            last_concurrent_ticks: 0,
        }
    }
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

    /// 设置 ConcurrentTrace 每拍灰对象扫描上限（至少为 1）。
    pub fn set_gray_budget_per_slice(&mut self, budget: usize) {
        self.gray_budget_per_slice = budget.max(1);
    }

    /// 当前灰队列长度（测试 / 诊断）。
    pub fn gray_queue_len(&self) -> usize {
        self.gray.len()
    }

    /// 按堆上 [`crate::GcPolicy`] 选择 nursery、全堆或并发标记（单线程模拟）回收。
    ///
    /// 若发生 nursery 物理晋升，返回转发图；全堆 / 并发标记路径返回空图。
    pub fn collect_for_policy(&mut self, roots: GcRoots<'_>, heap: &mut ObjectHeap) -> RelocateMap {
        use crate::policy::GcMode;

        let mode = heap.policy().mode;
        if matches!(mode, GcMode::ConcurrentMarkReserved) {
            // 完整周期：忽略 ticker 延长，避免后台节拍使 while 永续。
            while !self.poll_concurrent_mark_ex(roots, heap, true) {}
            self.nursery_collects_since_full = 0;
            return RelocateMap::new();
        }

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

    /// 在 safepoint 推进一步并发标记（可响应 ticker 延长 ConcurrentTrace）。
    ///
    /// 返回 `true` 表示本步已完成清扫并回到 Idle。
    pub fn poll_concurrent_mark(&mut self, roots: GcRoots<'_>, heap: &mut ObjectHeap) -> bool {
        self.poll_concurrent_mark_ex(roots, heap, false)
    }

    /// `force_complete` 为真时，`ConcurrentTrace` 忽略 ticker，直接进入终止（供整周期回收）。
    pub fn poll_concurrent_mark_ex(
        &mut self,
        roots: GcRoots<'_>,
        heap: &mut ObjectHeap,
        force_complete: bool,
    ) -> bool {
        use crate::concurrent::{ConcurrentMarkEvent, ConcurrentMarkState};

        heap.concurrent_mark_mut().set_enabled(true);
        let state = heap.concurrent_mark().state();

        match state {
            ConcurrentMarkState::Idle => {
                let slot_count = heap.slot_count();
                if slot_count == 0 {
                    return true;
                }
                heap.concurrent_mark_mut()
                    .transition(ConcurrentMarkEvent::BeginCycle)
                    .expect("enabled BeginCycle");
                self.marked.resize(slot_count, false);
                self.marked.fill(false);
                self.gray.clear();
                // 根快照只入灰，不递归扫闭包；闭包由 ConcurrentTrace 有界切片完成。
                self.enqueue_interpreter_roots(roots);
                self.enqueue_host_roots(heap);
                heap.concurrent_mark_mut()
                    .transition(ConcurrentMarkEvent::RootsReady)
                    .expect("RootsReady");
                heap.concurrent_mark_mut()
                    .transition(ConcurrentMarkEvent::TraceSliceDone)
                    .expect("enter ConcurrentTrace");
                self.last_concurrent_ticks = heap.concurrent_mark_ticks();
                false
            }
            ConcurrentMarkState::StartMark | ConcurrentMarkState::RootSnapshot => {
                // 异常残留：中止后下一拍从 Idle 重开。
                let _ = heap.concurrent_mark_mut().transition(ConcurrentMarkEvent::Abort);
                false
            }
            ConcurrentMarkState::ConcurrentTrace => {
                self.ensure_marked_capacity(heap);
                self.drain_satb_to_gray(heap);
                let gray_done = self.process_gray_slice(heap);
                let ticks = heap.concurrent_mark_ticks();
                if !gray_done {
                    // 灰队列未空：本拍有界切片结束，停留 ConcurrentTrace。
                    self.last_concurrent_ticks = ticks;
                    heap.concurrent_mark_mut()
                        .transition(ConcurrentMarkEvent::TraceSliceDone)
                        .expect("stay ConcurrentTrace for gray work");
                } else if !force_complete && ticks > self.last_concurrent_ticks {
                    // 灰已空但后台节拍有进展：再留一拍（仍不在后台扫堆）。
                    self.last_concurrent_ticks = ticks;
                    heap.concurrent_mark_mut()
                        .transition(ConcurrentMarkEvent::TraceSliceDone)
                        .expect("stay ConcurrentTrace");
                } else {
                    // 灰空且（强制完成或无新节拍）：进入终止检测。
                    heap.concurrent_mark_mut()
                        .transition(ConcurrentMarkEvent::TerminationOk)
                        .expect("to TerminationCheck");
                }
                false
            }
            ConcurrentMarkState::TerminationCheck => {
                self.ensure_marked_capacity(heap);
                let pending_satb = !heap.barrier().satb_buffer().is_empty();
                self.drain_satb_to_gray(heap);
                let pending_gray = !self.gray.is_empty();
                if pending_satb || pending_gray {
                    heap.concurrent_mark_mut()
                        .transition(ConcurrentMarkEvent::TerminationRetry)
                        .expect("back to ConcurrentTrace");
                } else {
                    heap.concurrent_mark_mut()
                        .transition(ConcurrentMarkEvent::TerminationOk)
                        .expect("to Remark");
                }
                false
            }
            ConcurrentMarkState::Remark => {
                self.ensure_marked_capacity(heap);
                self.drain_satb_to_gray(heap);
                // Remark：从根做同步全闭包，闭合并发窗口遗漏。
                while let Some(id) = self.gray.pop() {
                    scan_gray_object(id, heap, &mut self.marked, &mut self.gray);
                }
                self.mark_interpreter_roots(roots, heap);
                self.mark_host_roots(heap);
                heap.concurrent_mark_mut()
                    .transition(ConcurrentMarkEvent::RemarkDone)
                    .expect("to Sweep");
                false
            }
            ConcurrentMarkState::Sweep => {
                heap.sweep(&self.marked);
                heap.clear_remembered();
                heap.barrier_mut().clear_satb();
                self.gray.clear();
                self.last_relocate = RelocateMap::new();
                heap.concurrent_mark_mut()
                    .transition(ConcurrentMarkEvent::SweepDone)
                    .expect("to Idle");
                true
            }
        }
    }

    fn ensure_marked_capacity(&mut self, heap: &ObjectHeap) {
        let slot_count = heap.slot_count();
        if self.marked.len() < slot_count {
            self.marked.resize(slot_count, false);
        }
    }

    fn drain_satb_to_gray(&mut self, heap: &mut ObjectHeap) {
        let drained = heap.drain_satb_buffer();
        for id in drained {
            enqueue_object_gray(id, &mut self.marked, &mut self.gray);
        }
    }

    /// 扫描最多 `gray_budget_per_slice` 个灰对象；返回灰队列是否已空。
    fn process_gray_slice(&mut self, heap: &ObjectHeap) -> bool {
        let budget = self.gray_budget_per_slice;
        for _ in 0..budget {
            let Some(id) = self.gray.pop() else {
                return true;
            };
            scan_gray_object(id, heap, &mut self.marked, &mut self.gray);
        }
        self.gray.is_empty()
    }

    fn enqueue_interpreter_roots(&mut self, roots: GcRoots<'_>) {
        for value in roots.stack {
            enqueue_value_gray(value, &mut self.marked, &mut self.gray);
        }
        for locals in roots.frame_locals {
            for value in *locals {
                enqueue_value_gray(value, &mut self.marked, &mut self.gray);
            }
        }
        for value in roots.globals {
            enqueue_value_gray(value, &mut self.marked, &mut self.gray);
        }
        for &coroutine_id in roots.frame_coroutines {
            enqueue_value_gray(&Value::Coroutine(coroutine_id), &mut self.marked, &mut self.gray);
        }
    }

    fn enqueue_host_roots(&mut self, heap: &ObjectHeap) {
        let pinned: Vec<Value> = heap.host_roots().iter().cloned().collect();
        for value in &pinned {
            enqueue_value_gray(value, &mut self.marked, &mut self.gray);
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
