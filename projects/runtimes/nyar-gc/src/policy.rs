//! GC 策略与上游工作负载提示的运行时入口。
//!
//! 策略选择属于 `nyar-gc`；业务意图由 Valkyrie 侧提供。本模块先固定类型边界，
//! 不得把部署参数烘焙进 `.nyar` 外码。

/// GC 运行模式（进程 / 堆创建时选定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GcMode {
    /// 同步全堆标记清扫（默认）。
    #[default]
    MarkSweep,
    /// 分代低延迟：safepoint 优先 nursery 回收，周期性全堆。
    GenerationalLowLatency,
    /// 吞吐优先：当前仍走全堆回收（大批次语义占位）。
    ThroughputBatch,
    /// 并发标记低延迟（单线程 `poll_concurrent_mark` + SATB；无后台线程，直至写屏障/别名合同闭合）。
    ConcurrentMarkReserved,
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
    /// 是否允许在维护窗口做较重整理（分代模式下强制全堆）。
    pub allow_heavy_collection: bool,
    /// ConcurrentTrace 每拍灰对象扫描上限；`None` 表示沿用收集器当前值。
    pub concurrent_gray_budget: Option<u32>,
    /// ConcurrentTrace 单次 poll 最多灰切片数（含 ticker boost）；`None` 表示默认上限。
    pub concurrent_trace_slices_per_poll: Option<u32>,
}

impl WorkloadHints {
    /// 由暂停预算推导 ConcurrentTrace 灰扫描默认值（未显式声明预算时使用）。
    pub fn derived_gray_budget(&self) -> Option<usize> {
        if let Some(budget) = self.concurrent_gray_budget {
            return Some(budget.max(1) as usize);
        }
        match self.pause_budget_ms {
            Some(ms) if ms <= 1 => Some(8),
            Some(ms) if ms <= 5 => Some(32),
            Some(ms) if ms <= 20 => Some(64),
            Some(_) => Some(128),
            None => None,
        }
    }

    /// 由暂停预算推导单次 poll 灰切片上限。
    pub fn derived_trace_slices_per_poll(&self) -> Option<usize> {
        if let Some(slices) = self.concurrent_trace_slices_per_poll {
            return Some(slices.max(1) as usize);
        }
        match self.pause_budget_ms {
            Some(ms) if ms <= 1 => Some(2),
            Some(ms) if ms <= 5 => Some(4),
            Some(_) => Some(8),
            None => None,
        }
    }
}

/// 堆级策略配置。
#[derive(Debug, Clone, PartialEq)]
pub struct GcPolicy {
    /// 选中的回收模式。
    pub mode: GcMode,
    /// 当前工作负载提示。
    pub hints: WorkloadHints,
    /// 连续 nursery 回收次数达到该值后强制一次全堆（分代模式）。
    pub full_collect_every_n_nursery: u32,
}

impl Default for GcPolicy {
    fn default() -> Self {
        Self::mark_sweep_baseline()
    }
}

impl GcPolicy {
    /// 默认低延迟意图下的标记清扫基线。
    pub fn mark_sweep_baseline() -> Self {
        Self {
            mode: GcMode::MarkSweep,
            hints: WorkloadHints::default(),
            full_collect_every_n_nursery: 8,
        }
    }

    /// 分代低延迟策略（nursery + 周期性全堆）。
    pub fn generational_low_latency() -> Self {
        Self {
            mode: GcMode::GenerationalLowLatency,
            hints: WorkloadHints {
                pause_budget_ms: Some(5),
                ..WorkloadHints::default()
            },
            full_collect_every_n_nursery: 8,
        }
    }

    /// 并发标记预留模式（单线程状态机 + SATB；无后台线程）。
    pub fn concurrent_mark_reserved() -> Self {
        Self {
            mode: GcMode::ConcurrentMarkReserved,
            hints: WorkloadHints {
                pause_budget_ms: Some(5),
                ..WorkloadHints::default()
            },
            full_collect_every_n_nursery: 8,
        }
    }

    /// 用上游提示覆盖软约束。
    pub fn with_hints(mut self, hints: WorkloadHints) -> Self {
        self.hints = hints;
        self
    }

    /// 仅更新工作负载提示，保留模式与周期配置。
    pub fn apply_hints(&mut self, hints: WorkloadHints) {
        self.hints = hints;
    }

    /// 调整 nursery→全堆周期。
    pub fn with_full_collect_every(mut self, n: u32) -> Self {
        self.full_collect_every_n_nursery = n.max(1);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_budgets_tighten_with_pause_hint() {
        let tight = WorkloadHints {
            pause_budget_ms: Some(1),
            ..WorkloadHints::default()
        };
        assert_eq!(tight.derived_gray_budget(), Some(8));
        assert_eq!(tight.derived_trace_slices_per_poll(), Some(2));

        let explicit = WorkloadHints {
            concurrent_gray_budget: Some(3),
            concurrent_trace_slices_per_poll: Some(1),
            pause_budget_ms: Some(1),
            ..WorkloadHints::default()
        };
        assert_eq!(explicit.derived_gray_budget(), Some(3));
        assert_eq!(explicit.derived_trace_slices_per_poll(), Some(1));
    }
}
