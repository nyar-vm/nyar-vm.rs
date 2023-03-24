//! 写屏障合同：所有可能产生老→年轻或跨对象引用的写入必须经此入口。
//!
//! 当前实现为标记清扫且无分代，屏障记录为空操作，但解释器与未来 JIT
//! 必须统一调用，避免日后接入分代时遗漏写入点。

use crate::value::Value;

/// 可变对象槽的写屏障上下文（预留记忆集挂钩）。
#[derive(Debug, Default)]
pub struct WriteBarrier {
    /// 预留：跨代写记录条数（诊断用）。
    recorded: u64,
}

impl WriteBarrier {
    /// 创建屏障。
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一次托管引用写入（当前仅计数）。
    pub fn note_ref_write(&mut self) {
        self.recorded = self.recorded.saturating_add(1);
    }

    /// 已记录的引用写入次数。
    pub fn recorded_writes(&self) -> u64 {
        self.recorded
    }
}

/// 将 `value` 写入 `slot`，并通知屏障。
pub fn write_value_slot(barrier: &mut WriteBarrier, slot: &mut Value, value: Value) {
    if value.heap_ids().next().is_some() {
        barrier.note_ref_write();
    }
    *slot = value;
}
