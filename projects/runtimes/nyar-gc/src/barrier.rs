//! 写屏障合同：所有可能产生老→年轻或跨对象引用的写入必须经此入口。
//!
//! 分代启用时，老年代槽写入年轻引用会记入记忆集，供 nursery 回收扫描。
//! 并发标记启用且处于 mutator 可见标记阶段时，覆盖旧引用会记入 SATB 缓冲。

use crate::value::{ObjectId, Value};

/// 可变对象槽的写屏障上下文（含记忆集与 SATB 缓冲）。
#[derive(Debug, Default)]
pub struct WriteBarrier {
    /// 引用写入总次数（诊断）。
    recorded: u64,
    /// 老→年轻写入的容器对象 id（记忆集；可含重复，collect 时去重扫描）。
    remembered: Vec<ObjectId>,
    /// SATB（snapshot-at-the-beginning）删除屏障缓冲：被覆盖的旧托管引用。
    satb: Vec<ObjectId>,
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

    /// SATB：记录即将被覆盖的旧引用（浅层 object / coroutine id）。
    pub fn record_satb_pre_write(&mut self, old: &Value) {
        for id in old.heap_ids() {
            self.satb.push(id);
        }
    }

    /// 已记录的引用写入次数。
    pub fn recorded_writes(&self) -> u64 {
        self.recorded
    }

    /// 记忆集（老年代容器 id）。
    pub fn remembered_set(&self) -> &[ObjectId] {
        &self.remembered
    }

    /// SATB 缓冲（尚未排空的旧引用）。
    pub fn satb_buffer(&self) -> &[ObjectId] {
        &self.satb
    }

    /// 清空记忆集（nursery 回收后或全堆回收后）。
    pub fn clear_remembered(&mut self) {
        self.remembered.clear();
    }

    /// 排空 SATB 缓冲（终止检测 / Remark 前由 collector 消费）。
    pub fn drain_satb(&mut self) -> Vec<ObjectId> {
        std::mem::take(&mut self.satb)
    }

    /// 清空 SATB（周期中止或不消费时）。
    pub fn clear_satb(&mut self) {
        self.satb.clear();
    }
}

/// 将 `value` 写入 `slot`，并通知屏障。
///
/// `satb_active` 为真时，在覆盖前把旧托管引用记入 SATB 缓冲（单线程协议骨架）。
pub fn write_value_slot(barrier: &mut WriteBarrier, slot: &mut Value, value: Value, satb_active: bool) {
    if satb_active {
        barrier.record_satb_pre_write(slot);
    }
    if value.heap_ids().next().is_some() {
        barrier.note_ref_write();
    }
    *slot = value;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn satb_records_overwritten_object_id() {
        let mut barrier = WriteBarrier::new();
        let mut slot = Value::Object(7);
        write_value_slot(&mut barrier, &mut slot, Value::Null, true);
        assert_eq!(barrier.satb_buffer(), &[7]);
        assert_eq!(slot, Value::Null);
    }

    #[test]
    fn satb_inactive_skips_buffer() {
        let mut barrier = WriteBarrier::new();
        let mut slot = Value::Object(7);
        write_value_slot(&mut barrier, &mut slot, Value::Null, false);
        assert!(barrier.satb_buffer().is_empty());
    }

    #[test]
    fn satb_records_overwrites_in_order_on_same_slot() {
        // 单线程协议：同槽连续覆盖时，SATB 按覆盖前序追加旧引用。
        let mut barrier = WriteBarrier::new();
        let mut slot = Value::Object(1);
        write_value_slot(&mut barrier, &mut slot, Value::Object(2), true);
        write_value_slot(&mut barrier, &mut slot, Value::Null, true);
        assert_eq!(barrier.satb_buffer(), &[1, 2]);
        assert_eq!(slot, Value::Null);
    }

    #[test]
    fn satb_interleaved_two_slots_preserves_fifo() {
        let mut barrier = WriteBarrier::new();
        let mut a = Value::Object(10);
        let mut b = Value::Object(20);
        write_value_slot(&mut barrier, &mut a, Value::Object(11), true);
        write_value_slot(&mut barrier, &mut b, Value::Object(21), true);
        write_value_slot(&mut barrier, &mut a, Value::Null, true);
        assert_eq!(barrier.satb_buffer(), &[10, 20, 11]);
    }

    #[test]
    fn remembered_set_records_containers_in_call_order() {
        // 单线程协议：老→年轻边按 record 调用序进入记忆集（可含重复）。
        let mut barrier = WriteBarrier::new();
        barrier.record_old_to_young(1);
        barrier.record_old_to_young(2);
        barrier.record_old_to_young(1);
        assert_eq!(barrier.remembered_set(), &[1, 2, 1]);
    }

    #[test]
    fn remembered_set_interleaved_with_satb_keeps_independent_fifos() {
        let mut barrier = WriteBarrier::new();
        let mut slot = Value::Object(7);
        barrier.record_old_to_young(100);
        write_value_slot(&mut barrier, &mut slot, Value::Object(8), true);
        barrier.record_old_to_young(200);
        write_value_slot(&mut barrier, &mut slot, Value::Null, true);
        assert_eq!(barrier.remembered_set(), &[100, 200]);
        assert_eq!(barrier.satb_buffer(), &[7, 8]);
    }

    #[test]
    fn satb_records_overwritten_coroutine_id() {
        let mut barrier = WriteBarrier::new();
        let mut slot = Value::Coroutine(42);
        write_value_slot(&mut barrier, &mut slot, Value::Null, true);
        assert_eq!(barrier.satb_buffer(), &[42]);
        assert_eq!(slot, Value::Null);
    }

    #[test]
    fn drain_satb_returns_fifo_and_clears_buffer() {
        let mut barrier = WriteBarrier::new();
        let mut a = Value::Object(1);
        let mut b = Value::Coroutine(2);
        write_value_slot(&mut barrier, &mut a, Value::Object(3), true);
        write_value_slot(&mut barrier, &mut b, Value::Null, true);
        assert_eq!(barrier.drain_satb(), vec![1, 2]);
        assert!(barrier.satb_buffer().is_empty());
        assert!(barrier.drain_satb().is_empty());
    }
}
