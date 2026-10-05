//! 上游工作负载意图：来源分层、软/硬约束与合并规则。
//!
//! 业务意图不得授权回收仍可达对象；冲突硬目标返回诊断，软偏好按来源优先级合并。

use crate::policy::GcMode;

/// 意图来源（可信类别；不可互相冒充）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntentSource {
    /// 工程 / 清单声明的默认意图。
    ProjectConfig,
    /// 部署环境绑定的资源与服务等级。
    Deployment,
    /// 运行时显式阶段事件（请求、批次、维护窗口）。
    PhaseEvent,
    /// 历史 profile 经验提示（最低优先级）。
    Profile,
}

impl IntentSource {
    /// 合并软偏好时的优先级（越大越优先）。
    pub fn soft_priority(self) -> u8 {
        match self {
            Self::Deployment => 40,
            Self::PhaseEvent => 30,
            Self::ProjectConfig => 20,
            Self::Profile => 10,
        }
    }
}

/// 对象寿命预期（提示，非证明）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ObjectLifetimeHint {
    /// 未声明。
    #[default]
    Unspecified,
    /// 短命批次。
    ShortLivedBatch,
    /// 跨批次缓存。
    CrossBatchCache,
    /// 长期共享状态。
    LongLivedShared,
    /// 大型只读数据。
    LargeReadonly,
}

/// 一条上游工作负载意图（字段均可选；缺省不覆盖）。
#[derive(Debug, Clone, PartialEq)]
pub struct WorkloadIntent {
    /// 场景标识（人类可读，不作运行时分派键）。
    pub scenario_id: Option<String>,
    /// 来源类别。
    pub source: IntentSource,
    /// 阶段名（阶段事件时必填更清晰）。
    pub phase: Option<String>,
    /// GC / 交互暂停预算（毫秒）。
    pub pause_budget_ms: Option<u32>,
    /// 堆软上限（字节）。
    pub heap_soft_limit_bytes: Option<u64>,
    /// 是否允许较重整理；`None` 表示未声明。
    pub allow_heavy_collection: Option<bool>,
    /// 偏好的回收模式；与硬延迟冲突时拒绝。
    pub preferred_mode: Option<GcMode>,
    /// 寿命预期提示。
    pub lifetime_hint: ObjectLifetimeHint,
}

impl WorkloadIntent {
    /// 空意图（仅带来源标签）。
    pub fn empty(source: IntentSource) -> Self {
        Self {
            scenario_id: None,
            source,
            phase: None,
            pause_budget_ms: None,
            heap_soft_limit_bytes: None,
            allow_heavy_collection: None,
            preferred_mode: None,
            lifetime_hint: ObjectLifetimeHint::Unspecified,
        }
    }

    /// 在线请求样本：低暂停、分代。
    pub fn sample_online_request() -> Self {
        Self {
            scenario_id: Some("online-request".into()),
            source: IntentSource::ProjectConfig,
            phase: Some("request".into()),
            pause_budget_ms: Some(5),
            heap_soft_limit_bytes: Some(64 * 1024 * 1024),
            allow_heavy_collection: Some(false),
            preferred_mode: Some(GcMode::GenerationalLowLatency),
            lifetime_hint: ObjectLifetimeHint::ShortLivedBatch,
        }
    }

    /// 离线批处理样本：可在批次边界做重回收。
    pub fn sample_offline_batch() -> Self {
        Self {
            scenario_id: Some("offline-batch".into()),
            source: IntentSource::ProjectConfig,
            phase: Some("batch".into()),
            pause_budget_ms: Some(200),
            heap_soft_limit_bytes: Some(512 * 1024 * 1024),
            allow_heavy_collection: Some(true),
            preferred_mode: Some(GcMode::ThroughputBatch),
            lifetime_hint: ObjectLifetimeHint::ShortLivedBatch,
        }
    }

    /// 常驻服务样本：混合缓存与临时对象。
    pub fn sample_resident_service() -> Self {
        Self {
            scenario_id: Some("resident-service".into()),
            source: IntentSource::Deployment,
            phase: Some("steady".into()),
            pause_budget_ms: Some(10),
            heap_soft_limit_bytes: Some(256 * 1024 * 1024),
            allow_heavy_collection: Some(false),
            preferred_mode: Some(GcMode::GenerationalLowLatency),
            lifetime_hint: ObjectLifetimeHint::CrossBatchCache,
        }
    }

    /// 交互式并发标记样本：紧暂停、禁止重整理。
    pub fn sample_concurrent_interactive() -> Self {
        Self {
            scenario_id: Some("concurrent-interactive".into()),
            source: IntentSource::PhaseEvent,
            phase: Some("interactive".into()),
            pause_budget_ms: Some(5),
            heap_soft_limit_bytes: Some(128 * 1024 * 1024),
            allow_heavy_collection: Some(false),
            preferred_mode: Some(GcMode::ConcurrentMarkReserved),
            lifetime_hint: ObjectLifetimeHint::ShortLivedBatch,
        }
    }

    /// 故意硬冲突样本：吞吐模式 + 紧暂停（仅供合同测试，不得用于部署）。
    pub fn sample_hard_conflict_throughput_tight_pause() -> Self {
        Self {
            scenario_id: Some("hard-conflict-throughput-tight-pause".into()),
            source: IntentSource::ProjectConfig,
            phase: Some("invalid".into()),
            pause_budget_ms: Some(5),
            heap_soft_limit_bytes: None,
            allow_heavy_collection: Some(true),
            preferred_mode: Some(GcMode::ThroughputBatch),
            lifetime_hint: ObjectLifetimeHint::Unspecified,
        }
    }

    /// 故意硬冲突样本：并发标记 + 重整理。
    pub fn sample_hard_conflict_concurrent_heavy() -> Self {
        Self {
            scenario_id: Some("hard-conflict-concurrent-heavy".into()),
            source: IntentSource::Deployment,
            phase: Some("invalid".into()),
            pause_budget_ms: Some(5),
            heap_soft_limit_bytes: None,
            allow_heavy_collection: Some(true),
            preferred_mode: Some(GcMode::ConcurrentMarkReserved),
            lifetime_hint: ObjectLifetimeHint::Unspecified,
        }
    }
}

/// 意图冲突或非法阶段操作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntentError {
    /// 硬目标互相冲突。
    HardConflict {
        /// 人类可读说明。
        message: String,
    },
    /// 阶段栈为空或不匹配。
    PhaseMismatch {
        /// 人类可读说明。
        message: String,
    },
}

impl std::fmt::Display for IntentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HardConflict { message } | Self::PhaseMismatch { message } => {
                write!(f, "{message}")
            }
        }
    }
}

impl std::error::Error for IntentError {}
