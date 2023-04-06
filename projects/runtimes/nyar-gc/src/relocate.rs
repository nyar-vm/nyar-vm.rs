//! 年轻代晋升时的转发图：旧 [`ObjectId`] → 新老年代槽。

use crate::value::{ObjectId, Value};

/// 回收后用于改写根与堆内引用的转发表。
#[derive(Debug, Clone, Default)]
pub struct RelocateMap {
    /// `forward[old] = Some(new)` 表示对象已搬迁；`None` 表示未搬或已死亡。
    forward: Vec<Option<ObjectId>>,
}

impl RelocateMap {
    /// 空表。
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_capacity(slot_count: usize) -> Self {
        Self { forward: vec![None; slot_count] }
    }

    pub(crate) fn record(&mut self, old: ObjectId, new: ObjectId) {
        if old >= self.forward.len() {
            self.forward.resize(old + 1, None);
        }
        self.forward[old] = Some(new);
    }

    /// 是否发生过任何搬迁。
    pub fn has_moves(&self) -> bool {
        self.forward.iter().any(|slot| slot.is_some())
    }

    /// 将旧 id 映射为新 id；无记录则原样返回。
    pub fn map(&self, id: ObjectId) -> ObjectId {
        self.forward.get(id).copied().flatten().unwrap_or(id)
    }

    /// 改写单个值中的堆引用。
    pub fn rewrite_value(&self, value: &mut Value) {
        match value {
            Value::Object(id) | Value::Coroutine(id) => {
                *id = self.map(*id);
            }
            _ => {}
        }
    }

    /// 改写值切片。
    pub fn rewrite_slice(&self, values: &mut [Value]) {
        for value in values {
            self.rewrite_value(value);
        }
    }
}
