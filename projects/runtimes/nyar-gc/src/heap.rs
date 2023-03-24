use crate::barrier::WriteBarrier;
use crate::layout::{LayoutDescriptor, LayoutId};
use crate::policy::GcPolicy;
use crate::value::{CoroutineState, ObjectId, Value};

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

/// Managed object heap：自由列表分配 + 布局表 + 写屏障挂钩。
#[derive(Debug)]
pub struct ObjectHeap {
    objects: Vec<Option<ObjectPayload>>,
    /// 可复用的空槽下标（LIFO）。
    free_list: Vec<ObjectId>,
    layouts: Vec<Option<LayoutDescriptor>>,
    barrier: WriteBarrier,
    policy: GcPolicy,
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
            free_list: Vec::new(),
            layouts: Vec::new(),
            barrier: WriteBarrier::new(),
            policy: GcPolicy::mark_sweep_baseline(),
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

    /// Allocates a new object and returns its id（自由列表优先，避免线性扫描）。
    pub fn alloc(&mut self, payload: ObjectPayload) -> ObjectId {
        if let Some(id) = self.free_list.pop() {
            self.objects[id] = Some(payload);
            return id;
        }
        let id = self.objects.len();
        self.objects.push(Some(payload));
        id
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

    /// Write a field slot through the write barrier.
    pub fn set_field(&mut self, id: ObjectId, field_slot: usize, value: Value) -> Result<(), &'static str> {
        match self.objects.get_mut(id).and_then(|slot| slot.as_mut()) {
            Some(ObjectPayload::LayoutObject { slots, .. }) => {
                let slot = slots.get_mut(field_slot).ok_or("field slot out of range")?;
                crate::barrier::write_value_slot(&mut self.barrier, slot, value);
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
                *slot = None;
                self.free_list.push(index);
            }
        }
    }
}
