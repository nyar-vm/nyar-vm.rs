use crate::barrier::WriteBarrier;
use crate::generation::Generation;
use crate::layout::{LayoutDescriptor, LayoutId};
use crate::policy::GcPolicy;
use crate::roots::{HostRoots, RootHandle};
use crate::value::{CoroutineState, ObjectId, Value};

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

    /// Replace GC policy (does not migrate live objects between algorithms).
    pub fn set_policy(&mut self, policy: GcPolicy) {
        self.policy = policy;
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

    /// 将所有已标记的 nursery 对象晋升为老年代。
    pub(crate) fn promote_marked_nursery(&mut self, marked: &[bool]) {
        for (index, generation) in self.generations.iter_mut().enumerate() {
            if *generation == Generation::Nursery
                && self.objects.get(index).and_then(|s| s.as_ref()).is_some()
                && index < marked.len()
                && marked[index]
            {
                *generation = Generation::Tenured;
            }
        }
    }

    pub(crate) fn clear_remembered(&mut self) {
        self.barrier.clear_remembered();
    }

    pub(crate) fn remembered_set(&self) -> &[ObjectId] {
        self.barrier.remembered_set()
    }
}
