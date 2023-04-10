//! 晋升失败事件：年轻代存活量超过老年代软容量时的可诊断记录。
//!
//! 失败不得静默丢掉可达对象；调用方应改为全堆回收或背压，并把本事件写入证据。

/// 一次晋升失败的诊断快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionFailure {
    /// 本轮 nursery 中标记为存活、拟晋升的对象数。
    pub survivor_count: usize,
    /// 失败前老年代存活对象数。
    pub tenured_live_before: usize,
    /// 配置的老年代软容量（对象个数）。
    pub tenured_soft_capacity: usize,
    /// 人类可读原因。
    pub reason: &'static str,
}

impl std::fmt::Display for PromotionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "promotion failure: {} survivors + {} tenured exceed soft capacity {} ({})",
            self.survivor_count, self.tenured_live_before, self.tenured_soft_capacity, self.reason
        )
    }
}
