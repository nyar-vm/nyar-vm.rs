use crate::value::{CoroutineState, ObjectId, Value};

/// Simple object heap storing runtime object payloads.
#[derive(Debug, Default)]
pub struct ObjectHeap {
    objects: Vec<ObjectPayload>,
}

/// Object payload stored in the heap.
#[derive(Debug, Clone, PartialEq)]
pub enum ObjectPayload {
    /// Generic key/value object.
    Record(Vec<(String, Value)>),
    /// Suspended coroutine state shared between stack copies via `Value::Coroutine(ObjectId)`.
    Coroutine(CoroutineState),
}

impl ObjectHeap {
    /// Creates an empty heap.
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocates a new object and returns its id.
    pub fn alloc(&mut self, payload: ObjectPayload) -> ObjectId {
        let id = self.objects.len();
        self.objects.push(payload);
        id
    }

    /// Borrows an object payload by id.
    pub fn get(&self, id: ObjectId) -> Option<&ObjectPayload> {
        self.objects.get(id)
    }

    /// Mutably borrows an object payload by id.
    pub fn get_mut(&mut self, id: ObjectId) -> Option<&mut ObjectPayload> {
        self.objects.get_mut(id)
    }

    /// Allocates a coroutine payload and returns its id.
    pub fn alloc_coroutine(&mut self, state: CoroutineState) -> ObjectId {
        self.alloc(ObjectPayload::Coroutine(state))
    }

    /// Borrows a coroutine state by id.
    pub fn get_coroutine(&self, id: ObjectId) -> Option<&CoroutineState> {
        match self.objects.get(id) {
            Some(ObjectPayload::Coroutine(state)) => Some(state),
            _ => None,
        }
    }

    /// Mutably borrows a coroutine state by id.
    pub fn get_coroutine_mut(&mut self, id: ObjectId) -> Option<&mut CoroutineState> {
        match self.objects.get_mut(id) {
            Some(ObjectPayload::Coroutine(state)) => Some(state),
            _ => None,
        }
    }

    /// Number of allocated objects.
    pub fn len(&self) -> usize {
        self.objects.len()
    }
}
