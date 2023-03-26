//! 写屏障合同：所有可能产生老→年轻或跨对象引用的写入必须经此入口。
//!
//! 分代启用时，老年代槽写入年轻引用会记入记忆集，供 nursery 回收扫描。

use crate::value::{ObjectId, Value};

/// 可变对象槽的写屏障上下文（含记忆集）。
#[derive(Debug, Default)]
pub struct WriteBarrier {
    /// 引用写入总次数（诊断）。
    recorded: u64,
    /// 老→年轻写入的容器对象 id（记忆集；可含重复，collect 时去重扫描）。
    remembered: Vec<ObjectId>,
}

impl WriteBarrier {
    /// 创建屏障。
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一次托管引用写入（无分代信息时仅计数）。
    pub fn note_ref_write(&mut self) {
        self.recorded = self.recorded.saturating_add(1);
    }

    /// 明确记录一条老→年轻边的容器（记忆集）。
    pub fn record_old_to_young(&mut self, container: ObjectId) {
        self.remembered.push(container);
    }

    /// 已记录的引用写入次数。
    pub fn recorded_writes(&self) -> u64 {
        self.recorded
    }

    /// 记忆集（老年代容器 id）。
    pub fn remembered_set(&self) -> &[ObjectId] {
        &self.remembered
    }

    /// 清空记忆集（nursery 回收后或全堆回收后）。
    pub fn clear_remembered(&mut self) {
        self.remembered.clear();
    }
}

/// 将 `value` 写入 `slot`，并通知屏障（无容器代信息时仅计数）。
pub fn write_value_slot(barrier: &mut WriteBarrier, slot: &mut Value, value: Value) {
    if value.heap_ids().next().is_some() {
        barrier.note_ref_write();
    }
    *slot = value;
}
