#![doc = include_str!("readme.md")]

use crate::planning::PartitionBackendRequirement;

/// 候选后端元信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendCandidate {
    /// 后端名。
    pub name: String,
    /// 该候选后端所满足的需求。
    pub requirement: PartitionBackendRequirement,
    /// 选择优先级，越大越优先。
    pub priority: u16,
}

/// 简单后端选择器。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BackendSelector {
    /// 注册的候选后端。
    pub candidates: Vec<BackendCandidate>,
}

impl BackendSelector {
    /// 注册一个候选后端。
    pub fn register(&mut self, candidate: BackendCandidate) {
        self.candidates.push(candidate);
    }

    /// 为已经完成规划的后端需求选择优先级最高的候选。
    pub fn select(&self, requirement: &PartitionBackendRequirement) -> Option<&BackendCandidate> {
        self.candidates.iter().filter(|candidate| candidate.requirement == *requirement).max_by_key(|candidate| candidate.priority)
    }
}
