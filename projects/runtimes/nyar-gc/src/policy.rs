//! GC 策略与上游工作负载提示的运行时入口。
//!
//! 策略选择属于 `nyar-gc`；业务意图由 Valkyrie 侧提供。本模块先固定类型边界，
//! 具体分代/并发算法在后续工作包装入，不得把部署参数烘焙进 `.nyar` 外码。

/// GC 运行模式（进程 / 堆创建时选定；第一版仅 `MarkSweep` 可用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GcMode {
    /// 同步标记清扫（当前默认实现）。
    #[default]
    MarkSweep,
    /// 预留：分代低延迟（nursery 回收与晋升已落地；并发/物理拷贝未实现）。
    GenerationalLowLatency,
    /// 预留：吞吐优先大批次（尚未实现）。
    ThroughputBatch,
}

/// 上游提供的工作负载提示（软约束；不得授权回收仍可达对象）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkloadHints {
    /// 人类可读的阶段名，例如 `request` / `batch`。
    pub phase: Option<String>,
    /// 期望的 GC 暂停上界（毫秒）；`None` 表示未声明。
    pub pause_budget_ms: Option<u32>,
    /// 堆软上限提示（字节）；`None` 表示未声明。
    pub heap_soft_limit_bytes: Option<u64>,
    /// 是否允许在维护窗口做较重整理。
    pub allow_heavy_collection: bool,
}

/// 堆级策略配置。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GcPolicy {
    /// 选中的回收模式。
    pub mode: GcMode,
    /// 当前工作负载提示。
    pub hints: WorkloadHints,
}

impl GcPolicy {
    /// 默认低延迟意图下的标记清扫基线（实现仍为同步 mark-sweep）。
    pub fn mark_sweep_baseline() -> Self {
        Self { mode: GcMode::MarkSweep, hints: WorkloadHints::default() }
    }

    /// 用上游提示覆盖软约束；不改变尚未实现的模式。
    pub fn with_hints(mut self, hints: WorkloadHints) -> Self {
        self.hints = hints;
        self
    }
}
