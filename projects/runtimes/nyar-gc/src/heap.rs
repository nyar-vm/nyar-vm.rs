use crate::barrier::WriteBarrier;
use crate::controller::{StrategyController, StrategyDecision};
use crate::generation::Generation;
use crate::intent::{IntentError, WorkloadIntent};
use crate::layout::{LayoutDescriptor, LayoutId};
use crate::policy::GcPolicy;
use crate::promotion::PromotionFailure;
use crate::relocate::RelocateMap;
use crate::roots::{HostRoots, RootHandle};
use crate::value::{CoroutineState, ObjectId, Value};

/// 默认 nursery 存活对象上限（超过则形成分配压力，促使 minor GC）。
const DEFAULT_NURSERY_CAPACITY: usize = 1024;

/// 单对象头部近似开销（诊断用，非物理分配器精确值）。
const OBJECT_HEADER_BYTES: u64 = 16;
/// 每个值槽近似宽度。
const VALUE_SLOT_BYTES: u64 = 24;

/// Object payload stored in the heap.
#[derive(Debug, Clone, PartialEq)]
pub enum ObjectPayload {
    /// 稠密布局对象：槽位由 layout_id + field_slot 寻址，不含字段名。
    LayoutObject {
        /// 模块 layouts 表下标（或进程内映射后的布局 id）。
        layout_id: u32,
        /// 与布局 `field_count` 等长的值槽。
        slots: Vec<Value>,
    },
    /// Suspended coroutine state.
    Coroutine(CoroutineState),
}

impl ObjectPayload {
    /// 近似托管字节数（记账用）。
    pub fn accounting_bytes(&self) -> u64 {
        match self {
            Self::LayoutObject { slots, .. } => {
                OBJECT_HEADER_BYTES + (slots.len() as u64).saturating_mul(VALUE_SLOT_BYTES)
            }
            Self::Coroutine(state) => {
                let locals = (state.locals.len() as u64).saturating_mul(VALUE_SLOT_BYTES);
                let ops = (state.operand_stack.len() as u64).saturating_mul(VALUE_SLOT_BYTES);
                OBJECT_HEADER_BYTES + locals + ops + VALUE_SLOT_BYTES // yielded_value
            }
        }
    }
}

/// Managed object heap：自由列表分配 + 布局表 + 写屏障 + 宿主根 + 分代标签 + 字节记账。
#[derive(Debug)]
pub struct ObjectHeap {
    objects: Vec<Option<ObjectPayload>>,
    /// 与 `objects` 等长的代标签；空槽位的代无意义。
    generations: Vec<Generation>,
    /// 可复用的空槽下标（LIFO）。
    free_list: Vec<ObjectId>,
    layouts: Vec<Option<LayoutDescriptor>>,
    barrier: WriteBarrier,
    policy: GcPolicy,
    host_roots: HostRoots,
    /// 工作负载意图控制器。
    strategy: StrategyController,
    /// nursery 存活对象软容量（超限视为分配压力）。
    nursery_capacity: usize,
    /// 老年代存活对象软容量；`None` 表示不限制（仍可物理增长）。
    tenured_soft_capacity: Option<usize>,
    /// 最近一次晋升失败（若有）。
    last_promotion_failure: Option<PromotionFailure>,
    /// 当前存活对象近似字节合计。
    live_bytes: u64,
    /// 进程内累计分配字节（含已回收）。
    total_allocated_bytes: u64,
}

impl Default for ObjectHeap {
    fn default() -> Self {
        Self::new()
    }
}

impl ObjectHeap {
    /// Creates an empty heap with mark-sweep baseline policy.
    pub fn new() -> Self {
        Self {
            objects: Vec::new(),
            generations: Vec::new(),
            free_list: Vec::new(),
            layouts: Vec::new(),
            barrier: WriteBarrier::new(),
            policy: GcPolicy::mark_sweep_baseline(),
            host_roots: HostRoots::new(),
            strategy: StrategyController::new(),
            nursery_capacity: DEFAULT_NURSERY_CAPACITY,
            tenured_soft_capacity: None,
            last_promotion_failure: None,
            live_bytes: 0,
            total_allocated_bytes: 0,
        }
    }

    /// Creates a heap with an explicit GC policy.
    pub fn with_policy(policy: GcPolicy) -> Self {
        let mut heap = Self::new();
        heap.policy = policy;
        heap
    }

    /// Current GC policy.
    pub fn policy(&self) -> &GcPolicy {
        &self.policy
    }

    /// Mutably borrow GC policy（更新工作负载提示等）。
    pub fn policy_mut(&mut self) -> &mut GcPolicy {
        &mut self.policy
    }

    /// Replace GC policy (does not migrate live objects between algorithms).
    pub fn set_policy(&mut self, policy: GcPolicy) {
        self.policy = policy;
    }

    /// 设置进程级工作负载意图并刷新 [`GcPolicy`]。
    pub fn apply_intent(&mut self, intent: WorkloadIntent) -> Result<StrategyDecision, IntentError> {
        self.strategy.set_process_intent(intent)?;
        Ok(self.refresh_policy_from_strategy())
    }

    /// 进入业务阶段并刷新策略。
    pub fn begin_phase(&mut self, intent: WorkloadIntent) -> Result<StrategyDecision, IntentError> {
        self.strategy.begin_phase(intent)?;
        Ok(self.refresh_policy_from_strategy())
    }

    /// 结束业务阶段并刷新策略。
    pub fn end_phase(&mut self, phase: Option<&str>) -> Result<StrategyDecision, IntentError> {
        self.strategy.end_phase(phase)?;
        Ok(self.refresh_policy_from_strategy())
    }

    fn refresh_policy_from_strategy(&mut self) -> StrategyDecision {
        let decision = self.strategy.decide();
        self.policy.mode = decision.mode;
        self.policy.hints = decision.hints.clone();
        decision
    }

    /// 最近一次策略决策。
    pub fn last_strategy_decision(&self) -> Option<&StrategyDecision> {
        self.strategy.last_decision()
    }

    /// 策略控制器（诊断）。
    pub fn strategy(&self) -> &StrategyController {
        &self.strategy
    }

    /// Borrow the write barrier (for interpreter field / global stores).
    pub fn barrier_mut(&mut self) -> &mut WriteBarrier {
        &mut self.barrier
    }

    /// Borrow the write barrier immutably.
    pub fn barrier(&self) -> &WriteBarrier {
        &self.barrier
    }

    /// 宿主根表。
    pub fn host_roots(&self) -> &HostRoots {
        &self.host_roots
    }

    /// 宿主根表（可变）。
    pub fn host_roots_mut(&mut self) -> &mut HostRoots {
        &mut self.host_roots
    }

    /// 固定宿主根。
    pub fn pin_root(&mut self, value: Value) -> RootHandle {
        self.host_roots.pin(value)
    }

    /// 释放宿主根。
    pub fn unpin_root(&mut self, handle: RootHandle) {
        self.host_roots.unpin(handle);
    }

    /// 读取宿主根值。
    pub fn get_root(&self, handle: RootHandle) -> Option<&Value> {
        self.host_roots.get(handle)
    }

    /// Register or replace a layout descriptor.
    pub fn register_layout(&mut self, descriptor: LayoutDescriptor) {
        let id = descriptor.layout_id as usize;
        if id >= self.layouts.len() {
            self.layouts.resize(id + 1, None);
        }
        self.layouts[id] = Some(descriptor);
    }

    /// Look up a registered layout.
    pub fn layout(&self, layout_id: LayoutId) -> Option<&LayoutDescriptor> {
        self.layouts.get(layout_id as usize).and_then(|slot| slot.as_ref())
    }

    /// 对象当前代；空槽返回 `None`。
    pub fn generation(&self, id: ObjectId) -> Option<Generation> {
        if self.get(id).is_none() {
            return None;
        }
        self.generations.get(id).copied()
    }

    /// 将存活对象晋升到老年代。
    pub fn promote(&mut self, id: ObjectId) {
        if id < self.generations.len() && self.objects.get(id).and_then(|s| s.as_ref()).is_some() {
            self.generations[id] = Generation::Tenured;
        }
    }

    /// Nursery 中存活对象数量。
    pub fn nursery_live_count(&self) -> usize {
        self.objects
            .iter()
            .enumerate()
            .filter(|(id, slot)| slot.is_some() && self.generations.get(*id) == Some(&Generation::Nursery))
            .count()
    }

    /// nursery 软容量。
    pub fn nursery_capacity(&self) -> usize {
        self.nursery_capacity
    }

    /// 设置 nursery 软容量（至少 1）。
    pub fn set_nursery_capacity(&mut self, capacity: usize) {
        self.nursery_capacity = capacity.max(1);
    }

    /// 年轻代是否达到软容量（应优先做 nursery 回收）。
    pub fn nursery_pressure(&self) -> bool {
        self.nursery_live_count() >= self.nursery_capacity
    }

    /// 设置老年代存活对象软容量（`None` 清除限制）。
    pub fn set_tenured_soft_capacity(&mut self, capacity: Option<usize>) {
        self.tenured_soft_capacity = capacity.map(|n| n.max(1));
    }

    /// 老年代软容量。
    pub fn tenured_soft_capacity(&self) -> Option<usize> {
        self.tenured_soft_capacity
    }

    /// 老年代存活对象数。
    pub fn tenured_live_count(&self) -> usize {
        self.objects
            .iter()
            .enumerate()
            .filter(|(id, slot)| slot.is_some() && self.generations.get(*id) == Some(&Generation::Tenured))
            .count()
    }

    /// 最近一次晋升失败。
    pub fn last_promotion_failure(&self) -> Option<&PromotionFailure> {
        self.last_promotion_failure.as_ref()
    }

    /// 清除晋升失败记录（诊断消费后）。
    pub fn clear_promotion_failure(&mut self) {
        self.last_promotion_failure = None;
    }

    pub(crate) fn record_promotion_failure(&mut self, failure: PromotionFailure) {
        self.last_promotion_failure = Some(failure);
    }

    pub(crate) fn count_marked_nursery(&self, marked: &[bool]) -> usize {
        (0..self.objects.len())
            .filter(|&id| {
                marked.get(id) == Some(&true)
                    && self.generations.get(id) == Some(&Generation::Nursery)
                    && self.objects[id].is_some()
            })
            .count()
    }

    /// 若晋升 `survivor_count` 个对象会突破老年代软容量则返回失败描述。
    pub fn promotion_would_fail(&self, survivor_count: usize) -> Option<PromotionFailure> {
        let capacity = self.tenured_soft_capacity?;
        let tenured_live_before = self.tenured_live_count();
        if tenured_live_before.saturating_add(survivor_count) > capacity {
            Some(PromotionFailure {
                survivor_count,
                tenured_live_before,
                tenured_soft_capacity: capacity,
                reason: "tenured soft capacity exceeded",
            })
        } else {
            None
        }
    }

    /// Allocates a new object in the nursery and returns its id.
    pub fn alloc(&mut self, payload: ObjectPayload) -> ObjectId {
        self.alloc_in(payload, Generation::Nursery)
    }

    /// Allocates directly into the tenured generation.
    pub fn alloc_tenured(&mut self, payload: ObjectPayload) -> ObjectId {
        self.alloc_in(payload, Generation::Tenured)
    }

    fn alloc_in(&mut self, payload: ObjectPayload, generation: Generation) -> ObjectId {
        let bytes = payload.accounting_bytes();
        self.live_bytes = self.live_bytes.saturating_add(bytes);
        self.total_allocated_bytes = self.total_allocated_bytes.saturating_add(bytes);

        if let Some(id) = self.free_list.pop() {
            self.objects[id] = Some(payload);
            if id >= self.generations.len() {
                self.generations.resize(id + 1, Generation::Nursery);
            }
            self.generations[id] = generation;
            return id;
        }
        let id = self.objects.len();
        self.objects.push(Some(payload));
        self.generations.push(generation);
        id
    }

    /// 当前存活对象近似字节数。
    pub fn live_bytes(&self) -> u64 {
        self.live_bytes
    }

    /// 累计分配字节（含已回收对象）。
    pub fn total_allocated_bytes(&self) -> u64 {
        self.total_allocated_bytes
    }

    /// 是否超过工作负载声明的堆软上限。
    pub fn over_soft_limit(&self) -> bool {
        match self.policy.hints.heap_soft_limit_bytes {
            Some(limit) if limit > 0 => self.live_bytes >= limit,
            _ => false,
        }
    }

    /// Allocate a layout object with `field_count` null slots.
    pub fn alloc_layout_object(&mut self, layout_id: u32, field_count: usize) -> ObjectId {
        if self.layout(layout_id).is_none() {
            self.register_layout(LayoutDescriptor::all_references(layout_id, field_count as u32));
        }
        self.alloc(ObjectPayload::LayoutObject {
            layout_id,
            slots: vec![Value::Null; field_count],
        })
    }

    /// Borrows an object payload by id.
    pub fn get(&self, id: ObjectId) -> Option<&ObjectPayload> {
        self.objects.get(id).and_then(|slot| slot.as_ref())
    }

    /// Mutably borrows an object payload by id.
    pub fn get_mut(&mut self, id: ObjectId) -> Option<&mut ObjectPayload> {
        self.objects.get_mut(id).and_then(|slot| slot.as_mut())
    }

    /// Write a field slot through the write barrier（含老→年轻记忆集）。
    pub fn set_field(&mut self, id: ObjectId, field_slot: usize, value: Value) -> Result<(), &'static str> {
        let container_gen = self.generation(id).ok_or("object not found")?;
        let is_old_to_young = container_gen == Generation::Tenured
            && value.heap_ids().any(|child| self.generation(child) == Some(Generation::Nursery));

        match self.objects.get_mut(id).and_then(|slot| slot.as_mut()) {
            Some(ObjectPayload::LayoutObject { slots, .. }) => {
                let slot = slots.get_mut(field_slot).ok_or("field slot out of range")?;
                if value.heap_ids().next().is_some() {
                    self.barrier.note_ref_write();
                }
                if is_old_to_young {
                    self.barrier.record_old_to_young(id);
                }
                *slot = value;
                Ok(())
            }
            _ => Err("not a layout object"),
        }
    }

    /// Allocates a coroutine payload and returns its id.
    pub fn alloc_coroutine(&mut self, state: CoroutineState) -> ObjectId {
        self.alloc(ObjectPayload::Coroutine(state))
    }

    /// Borrows a coroutine state by id.
    pub fn get_coroutine(&self, id: ObjectId) -> Option<&CoroutineState> {
        match self.get(id) {
            Some(ObjectPayload::Coroutine(state)) => Some(state),
            _ => None,
        }
    }

    /// Mutably borrows a coroutine state by id.
    pub fn get_coroutine_mut(&mut self, id: ObjectId) -> Option<&mut CoroutineState> {
        match self.get_mut(id) {
            Some(ObjectPayload::Coroutine(state)) => Some(state),
            _ => None,
        }
    }

    /// Number of heap slots (live + free).
    pub fn slot_count(&self) -> usize {
        self.objects.len()
    }

    /// Number of live (allocated) objects.
    pub fn live_count(&self) -> usize {
        self.objects.iter().filter(|slot| slot.is_some()).count()
    }

    /// Free-list depth（诊断）。
    pub fn free_list_len(&self) -> usize {
        self.free_list.len()
    }

    /// Legacy alias for slot count.
    pub fn len(&self) -> usize {
        self.slot_count()
    }

    /// Drops unmarked objects after a collection pass and returns ids to the free list.
    pub(crate) fn sweep(&mut self, marked: &[bool]) {
        for (index, slot) in self.objects.iter_mut().enumerate() {
            if slot.is_some() && (index >= marked.len() || !marked[index]) {
                if let Some(payload) = slot.take() {
                    self.live_bytes = self.live_bytes.saturating_sub(payload.accounting_bytes());
                }
                self.free_list.push(index);
            }
        }
    }

    /// Nursery-only sweep：只回收未标记的年轻代；老年代即使未标记也保留
    /// （应由全堆回收处理；minor 路径假定老年代经根或记忆集可达）。
    pub(crate) fn sweep_nursery(&mut self, marked: &[bool]) {
        for (index, slot) in self.objects.iter_mut().enumerate() {
            if slot.is_none() {
                continue;
            }
            if self.generations.get(index) != Some(&Generation::Nursery) {
                continue;
            }
            if index >= marked.len() || !marked[index] {
                if let Some(payload) = slot.take() {
                    self.live_bytes = self.live_bytes.saturating_sub(payload.accounting_bytes());
                }
                self.free_list.push(index);
            }
        }
    }

    /// 将标记的 nursery 对象**物理搬迁**到新的老年代槽，返回转发图。
    ///
    /// 旧槽进入自由列表；堆内引用与宿主根就地改写。解释器栈/帧/全局须由调用方
    /// 用同一张 [`RelocateMap`] 改写。
    pub(crate) fn promote_marked_nursery_moving(&mut self, marked: &[bool]) -> RelocateMap {
        let mut map = RelocateMap::with_capacity(self.objects.len());
        let survivors: Vec<ObjectId> = (0..self.objects.len())
            .filter(|&id| {
                marked.get(id) == Some(&true)
                    && self.generations.get(id) == Some(&Generation::Nursery)
                    && self.objects[id].is_some()
            })
            .collect();

        let mut vacated = Vec::with_capacity(survivors.len());
        for old in survivors {
            let payload = self.objects[old].take().expect("survivor present");
            self.live_bytes = self.live_bytes.saturating_sub(payload.accounting_bytes());
            vacated.push(old);
            let new_id = self.alloc_tenured_fresh(payload);
            map.record(old, new_id);
        }
        for id in vacated {
            self.free_list.push(id);
        }
        self.rewrite_heap_references(&map);
        map
    }

    /// 仅追加老年代槽（不复用 free_list），供物理晋升使用。
    fn alloc_tenured_fresh(&mut self, payload: ObjectPayload) -> ObjectId {
        let bytes = payload.accounting_bytes();
        self.live_bytes = self.live_bytes.saturating_add(bytes);
        self.total_allocated_bytes = self.total_allocated_bytes.saturating_add(bytes);
        let id = self.objects.len();
        self.objects.push(Some(payload));
        self.generations.push(Generation::Tenured);
        id
    }

    fn rewrite_heap_references(&mut self, map: &RelocateMap) {
        if !map.has_moves() {
            return;
        }
        for slot in self.objects.iter_mut().flatten() {
            match slot {
                ObjectPayload::LayoutObject { slots, .. } => map.rewrite_slice(slots),
                ObjectPayload::Coroutine(state) => {
                    map.rewrite_value(&mut state.yielded_value);
                    map.rewrite_slice(&mut state.locals);
                    map.rewrite_slice(&mut state.operand_stack);
                }
            }
        }
        for value in self.host_roots.iter_mut() {
            map.rewrite_value(value);
        }
    }

    pub(crate) fn clear_remembered(&mut self) {
        self.barrier.clear_remembered();
    }

    pub(crate) fn remembered_set(&self) -> &[ObjectId] {
        self.barrier.remembered_set()
    }
}
