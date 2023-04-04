//! 策略控制器：合并进程意图与阶段栈，产出可解释的 [`StrategyDecision`]。

use crate::intent::{IntentError, IntentSource, ObjectLifetimeHint, WorkloadIntent};
use crate::policy::{GcMode, GcPolicy, WorkloadHints};

/// 一次策略决策记录（供诊断与证据）。
#[derive(Debug, Clone, PartialEq)]
pub struct StrategyDecision {
    /// 选中的模式。
    pub mode: GcMode,
    /// 合并后的软提示。
    pub hints: WorkloadHints,
    /// 人类可读选中原因。
    pub reason: String,
    /// 参与合并的来源。
    pub sources_considered: Vec<IntentSource>,
    /// 仍未知的业务维度（缺省基线时列出）。
    pub unknown_dimensions: Vec<&'static str>,
}

/// 合并进程级意图与嵌套阶段，驱动 [`GcPolicy`]。
#[derive(Debug, Default)]
pub struct StrategyController {
    process: Option<WorkloadIntent>,
    phases: Vec<WorkloadIntent>,
    last_decision: Option<StrategyDecision>,
}

impl StrategyController {
    /// 空控制器（保守 mark-sweep 基线，直至有意图）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置 / 替换进程级意图（工程或部署）。
    pub fn set_process_intent(&mut self, intent: WorkloadIntent) -> Result<(), IntentError> {
        validate_single(&intent)?;
        self.process = Some(intent);
        Ok(())
    }

    /// 压入阶段意图。
    pub fn begin_phase(&mut self, intent: WorkloadIntent) -> Result<(), IntentError> {
        validate_single(&intent)?;
        if intent.phase.is_none() {
            return Err(IntentError::PhaseMismatch {
                message: "phase intent requires a phase name".into(),
            });
        }
        self.phases.push(intent);
        Ok(())
    }

    /// 弹出阶段；若提供名字则必须与栈顶匹配（不匹配时不弹出）。
    pub fn end_phase(&mut self, phase: Option<&str>) -> Result<WorkloadIntent, IntentError> {
        let top = self.phases.last().ok_or_else(|| IntentError::PhaseMismatch {
            message: "phase stack is empty".into(),
        })?;
        if let Some(expected) = phase {
            match &top.phase {
                Some(name) if name == expected => {}
                Some(name) => {
                    return Err(IntentError::PhaseMismatch {
                        message: format!("expected to end phase `{expected}`, stack top is `{name}`"),
                    });
                }
                None => {
                    return Err(IntentError::PhaseMismatch {
                        message: format!("expected to end phase `{expected}`, stack top has no name"),
                    });
                }
            }
        }
        Ok(self.phases.pop().expect("phase stack checked non-empty"))
    }

    /// 当前阶段深度。
    pub fn phase_depth(&self) -> usize {
        self.phases.len()
    }

    /// 最近一次决策。
    pub fn last_decision(&self) -> Option<&StrategyDecision> {
        self.last_decision.as_ref()
    }

    /// 根据当前意图栈计算决策（不写 policy）。
    pub fn decide(&mut self) -> StrategyDecision {
        let layers: Vec<&WorkloadIntent> = self.process.iter().chain(self.phases.iter()).collect();
        let decision = merge_layers(&layers);
        self.last_decision = Some(decision.clone());
        decision
    }

    /// 计算决策并写入 [`GcPolicy`]。
    pub fn apply_to_policy(&mut self, policy: &mut GcPolicy) -> StrategyDecision {
        let decision = self.decide();
        policy.mode = decision.mode;
        policy.hints = decision.hints.clone();
        decision
    }
}

fn validate_single(intent: &WorkloadIntent) -> Result<(), IntentError> {
    if let (Some(GcMode::ThroughputBatch), Some(pause)) = (intent.preferred_mode, intent.pause_budget_ms) {
        if pause <= 10 {
            return Err(IntentError::HardConflict {
                message: format!("preferred ThroughputBatch conflicts with tight pause_budget_ms={pause}"),
            });
        }
    }
    Ok(())
}

fn merge_layers(layers: &[&WorkloadIntent]) -> StrategyDecision {
    if layers.is_empty() {
        return StrategyDecision {
            mode: GcMode::MarkSweep,
            hints: WorkloadHints::default(),
            reason: "no workload intent; conservative mark-sweep baseline".into(),
            sources_considered: Vec::new(),
            unknown_dimensions: vec!["scenario", "pause_budget_ms", "heap_soft_limit_bytes", "phase"],
        };
    }

    let mut sources: Vec<IntentSource> = layers.iter().map(|l| l.source).collect();
    sources.sort_by_key(|s| std::cmp::Reverse(s.soft_priority()));
    sources.dedup();

    let mut pause_budget_ms: Option<u32> = None;
    for layer in layers {
        if let Some(p) = layer.pause_budget_ms {
            pause_budget_ms = Some(match pause_budget_ms {
                Some(cur) => cur.min(p),
                None => p,
            });
        }
    }

    let mut heap_soft_limit_bytes: Option<u64> = None;
    for layer in layers {
        if let Some(limit) = layer.heap_soft_limit_bytes {
            heap_soft_limit_bytes = Some(match heap_soft_limit_bytes {
                Some(cur) => cur.min(limit),
                None => limit,
            });
        }
    }

    let heavy_votes: Vec<bool> = layers.iter().filter_map(|l| l.allow_heavy_collection).collect();
    let allow_heavy_collection = !heavy_votes.is_empty() && heavy_votes.iter().all(|v| *v);

    let phase = layers.iter().rev().find_map(|l| l.phase.clone());

    let mut preferred: Option<(u8, GcMode)> = None;
    for layer in layers {
        if let Some(mode) = layer.preferred_mode {
            let pri = layer.source.soft_priority();
            match preferred {
                Some((p, _)) if p >= pri => {}
                _ => preferred = Some((pri, mode)),
            }
        }
    }

    let lifetime = layers
        .iter()
        .rev()
        .map(|l| l.lifetime_hint)
        .find(|h| *h != ObjectLifetimeHint::Unspecified)
        .unwrap_or(ObjectLifetimeHint::Unspecified);

    let mut mode = preferred.map(|(_, m)| m).unwrap_or_else(|| {
        if matches!(lifetime, ObjectLifetimeHint::ShortLivedBatch) || pause_budget_ms.is_some_and(|p| p <= 20) {
            GcMode::GenerationalLowLatency
        } else if allow_heavy_collection {
            GcMode::ThroughputBatch
        } else {
            GcMode::MarkSweep
        }
    });

    if let Some(pause) = pause_budget_ms {
        if pause <= 10 && mode == GcMode::ThroughputBatch {
            mode = GcMode::GenerationalLowLatency;
        }
    }

    let mut unknown = Vec::new();
    if phase.is_none() {
        unknown.push("phase");
    }
    if pause_budget_ms.is_none() {
        unknown.push("pause_budget_ms");
    }
    if heap_soft_limit_bytes.is_none() {
        unknown.push("heap_soft_limit_bytes");
    }

    let reason = format!(
        "merged {} layer(s); mode={mode:?}; pause_budget_ms={pause_budget_ms:?}; heavy={allow_heavy_collection}",
        layers.len()
    );

    StrategyDecision {
        mode,
        hints: WorkloadHints {
            phase,
            pause_budget_ms,
            heap_soft_limit_bytes,
            allow_heavy_collection,
        },
        reason,
        sources_considered: sources,
        unknown_dimensions: unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_controller_uses_conservative_baseline() {
        let mut ctrl = StrategyController::new();
        let decision = ctrl.decide();
        assert_eq!(decision.mode, GcMode::MarkSweep);
        assert!(decision.unknown_dimensions.contains(&"pause_budget_ms"));
    }

    #[test]
    fn online_sample_selects_generational() {
        let mut ctrl = StrategyController::new();
        ctrl.set_process_intent(WorkloadIntent::sample_online_request()).unwrap();
        let decision = ctrl.decide();
        assert_eq!(decision.mode, GcMode::GenerationalLowLatency);
        assert_eq!(decision.hints.pause_budget_ms, Some(5));
        assert!(!decision.hints.allow_heavy_collection);
    }

    #[test]
    fn nested_phase_takes_stricter_pause() {
        let mut ctrl = StrategyController::new();
        ctrl.set_process_intent(WorkloadIntent::sample_offline_batch()).unwrap();
        let mut request = WorkloadIntent::sample_online_request();
        request.source = IntentSource::PhaseEvent;
        ctrl.begin_phase(request).unwrap();
        let decision = ctrl.decide();
        assert_eq!(decision.hints.pause_budget_ms, Some(5));
        assert_eq!(decision.mode, GcMode::GenerationalLowLatency);
        assert!(!decision.hints.allow_heavy_collection);
    }

    #[test]
    fn hard_conflict_rejects_throughput_with_tight_pause() {
        let intent = WorkloadIntent {
            preferred_mode: Some(GcMode::ThroughputBatch),
            pause_budget_ms: Some(5),
            ..WorkloadIntent::empty(IntentSource::ProjectConfig)
        };
        let err = StrategyController::new().set_process_intent(intent).unwrap_err();
        assert!(matches!(err, IntentError::HardConflict { .. }));
    }

    #[test]
    fn end_phase_checks_name() {
        let mut ctrl = StrategyController::new();
        ctrl.begin_phase(WorkloadIntent::sample_online_request()).unwrap();
        let err = ctrl.end_phase(Some("batch")).unwrap_err();
        assert!(matches!(err, IntentError::PhaseMismatch { .. }));
        ctrl.end_phase(Some("request")).unwrap();
        assert_eq!(ctrl.phase_depth(), 0);
    }

    #[test]
    fn apply_to_policy_updates_mode_and_hints() {
        let mut ctrl = StrategyController::new();
        ctrl.set_process_intent(WorkloadIntent::sample_resident_service()).unwrap();
        let mut policy = GcPolicy::mark_sweep_baseline();
        let decision = ctrl.apply_to_policy(&mut policy);
        assert_eq!(policy.mode, GcMode::GenerationalLowLatency);
        assert_eq!(policy.hints.heap_soft_limit_bytes, Some(256 * 1024 * 1024));
        assert_eq!(decision.mode, policy.mode);
    }
}
