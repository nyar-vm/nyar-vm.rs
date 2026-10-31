//! Asgard 交付宿主策略与 chunk 分割规划。
//!
//! 浏览器 HTTP 交付与桌面 WebView 交付共享同一套 DOM 执行语义、减枝规则和
//! JS/Wasm ABI，但产物组织不同：
//! - 浏览器按语义可达性减枝后，还需要计算 chunk 分割方案并生成 loader/manifest；
//! - 桌面 WebView 同样减枝，但直接在 target 策略中跳过 chunk partitioning，
//!   生成未分割的执行闭包，不计算浏览器 chunk 拓扑，也不生成动态 loader/manifest。
//!
//! 本模块只拥有这条策略事实，不生成文件名、hash 或 import 路径；具体产物由
//! 后续构建阶段消费 [`AsgardBuildPlan`]。

use crate::asgard::config_model::{ChunkBoundary, ChunkBudgets, ChunkPlan, ResolvedAsgardConfig};

/// 交付宿主策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryHost {
    /// 浏览器 HTTP 交付：按语义减枝后计算 chunk 分割方案。
    BrowserHttp,
    /// 桌面 WebView 交付：按语义减枝后直接生成未分割的执行闭包。
    DesktopWebView,
}

/// 语义减枝策略；所有宿主都必须按可达性减枝，不能携带整棵 std 或未使用组件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PruningStrategy {
    /// 按语义可达性减枝（函数、类型、数据段、组件、glue 与宿主绑定）。
    SemanticReachability,
}

/// chunk 分割规划结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkPartitioning {
    /// 浏览器交付：保留作者配置的边界与预算，供 loader/manifest 生成。
    Partitioned(ChunkPlan),
    /// 桌面交付：跳过分割规划；不产生边界、chunk 拓扑或动态 loader。
    Skipped,
}

impl ChunkPartitioning {
    /// 是否进行了 chunk 分割规划。
    pub fn is_partitioned(&self) -> bool {
        matches!(self, Self::Partitioned(_))
    }

    /// 浏览器分割计划；桌面未分割交付返回 `None`。
    pub fn plan(&self) -> Option<&ChunkPlan> {
        match self {
            Self::Partitioned(plan) => Some(plan),
            Self::Skipped => None,
        }
    }

    /// 规划出的切分边界；桌面未分割交付为空。
    pub fn boundaries(&self) -> &[ChunkBoundary] {
        self.plan().map_or(&[], |plan| plan.boundaries.as_slice())
    }

    /// 分包预算；桌面未分割交付不承担 chunk 预算门禁，返回 `None`。
    pub fn budgets(&self) -> Option<&ChunkBudgets> {
        self.plan().map(|plan| &plan.budgets)
    }

    /// 是否需要生成动态 chunk loader/manifest；桌面未分割交付不需要。
    pub fn requires_chunk_loader(&self) -> bool {
        self.is_partitioned()
    }
}

/// 一次 Asgard 构建的目标策略快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsgardBuildPlan {
    /// 交付宿主。
    pub host: DeliveryHost,
    /// 减枝策略；所有宿主一致。
    pub pruning: PruningStrategy,
    /// chunk 分割规划结果。
    pub chunk_partitioning: ChunkPartitioning,
}

/// 按交付宿主决定 chunk 分割规划。
///
/// 桌面 WebView 无论配置里写了多少 `artifact.chunks.boundaries`，都跳过分割规划；
/// 忽略这些边界不是丢事实，而是该宿主的产品合同。浏览器交付保留配置计划。
pub fn plan_asgard_build(host: DeliveryHost, chunks: &ChunkPlan) -> AsgardBuildPlan {
    let chunk_partitioning = match host {
        DeliveryHost::BrowserHttp => ChunkPartitioning::Partitioned(chunks.clone()),
        DeliveryHost::DesktopWebView => ChunkPartitioning::Skipped,
    };
    AsgardBuildPlan { host, pruning: PruningStrategy::SemanticReachability, chunk_partitioning }
}

impl ResolvedAsgardConfig {
    /// 以已校验配置生成目标策略快照；不读取配置以外的工程事实。
    pub fn build_plan(&self, host: DeliveryHost) -> AsgardBuildPlan {
        plan_asgard_build(host, &self.chunks)
    }
}
