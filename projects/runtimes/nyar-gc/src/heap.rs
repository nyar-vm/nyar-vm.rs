use crate::value::{CoroutineState, ObjectId, Value};

/// Object payload stored in the heap.
#[derive(Debug, Clone, PartialEq)]
pub enum ObjectPayload {
    /// Generic key/value object（宿主遗留桥；结构指令热路径不用字段名）。
    Record(Vec<(String, Value)>),
    /// 稠密布局对象：槽位由 layout_id + field_slot 寻址，不含字段名。
    LayoutObject {
        /// 模块 layouts 表下标。
        layout_id: u32,
        /// 与 `NyarLayout.field_count` 等长的值槽。
        slots: Vec<Value>,
    },
    /// Suspended coroutine state.
    Coroutine(CoroutineState),
}

/// Managed object heap with mark-sweep reclamation.
#[derive(Debug, Default)]
pub struct ObjectHeap {
    objects: Vec<Option<ObjectPayload>>,
}

impl ObjectHeap {
    /// Creates an empty heap.
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocates a new object and returns its id.
    pub fn alloc(&mut self, payload: ObjectPayload) -> ObjectId {
        if let Some(index) = self.objects.iter().position(|slot| slot.is_none()) {
            self.objects[index] = Some(payload);
            return index;
        }
        let id = self.objects.len();
        self.objects.push(Some(payload));
        id
    }

    /// Borrows an object payload by id.
    pub fn get(&self, id: ObjectId) -> Option<&ObjectPayload> {
        self.objects.get(id).and_then(|slot| slot.as_ref())
    }

    /// Mutably borrows an object payload by id.
    pub fn get_mut(&mut self, id: ObjectId) -> Option<&mut ObjectPayload> {
        self.objects.get_mut(id).and_then(|slot| slot.as_mut())
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

    /// Number of heap slots (live + reclaimed).
    pub fn slot_count(&self) -> usize {
        self.objects.len()
    }

    /// Number of live (allocated) objects.
    pub fn live_count(&self) -> usize {
        self.objects.iter().filter(|slot| slot.is_some()).count()
    }

    /// Legacy alias for slot count.
    pub fn len(&self) -> usize {
        self.slot_count()
    }

    /// Drops unmarked objects after a collection pass.
    pub(crate) fn sweep(&mut self, marked: &[bool]) {
        for (index, slot) in self.objects.iter_mut().enumerate() {
            if slot.is_some() && (index >= marked.len() || !marked[index]) {
                *slot = None;
            }
        }
    }
}
