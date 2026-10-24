//! 将 JSON 工作负载意图解析为 [`nyar_gc::WorkloadIntent`]。
//!
//! 解析属于宿主边界；`.nyar` 外码不携带这些字段。

use nyar_gc::{GcMode, IntentError, IntentSource, ObjectLifetimeHint, WorkloadIntent};

/// JSON 解析失败。
#[derive(Debug)]
pub struct WorkloadJsonError(String);

impl std::fmt::Display for WorkloadJsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for WorkloadJsonError {}

/// 解析一条工作负载意图 JSON 对象。
pub fn parse_workload_intent_json(source: &str) -> Result<WorkloadIntent, WorkloadJsonError> {
    let value: serde_json::Value = serde_json::from_str(source).map_err(|error| WorkloadJsonError(format!("invalid JSON: {error}")))?;
    let obj = value.as_object().ok_or_else(|| WorkloadJsonError("workload intent must be a JSON object".into()))?;

    let source = match obj.get("source").and_then(|v| v.as_str()).unwrap_or("project_config") {
        "project_config" => IntentSource::ProjectConfig,
        "deployment" => IntentSource::Deployment,
        "phase_event" => IntentSource::PhaseEvent,
        "profile" => IntentSource::Profile,
        other => {
            return Err(WorkloadJsonError(format!("unknown intent source `{other}`")));
        }
    };

    let preferred_mode = match obj.get("preferred_mode").and_then(|v| v.as_str()) {
        None => None,
        Some("mark_sweep") => Some(GcMode::MarkSweep),
        Some("generational_low_latency") => Some(GcMode::GenerationalLowLatency),
        Some("throughput_batch") => Some(GcMode::ThroughputBatch),
        Some("concurrent_mark_reserved") => Some(GcMode::ConcurrentMarkReserved),
        Some(other) => {
            return Err(WorkloadJsonError(format!("unknown preferred_mode `{other}`")));
        }
    };

    let lifetime_hint = match obj.get("lifetime_hint").and_then(|v| v.as_str()) {
        None | Some("unspecified") => ObjectLifetimeHint::Unspecified,
        Some("short_lived_batch") => ObjectLifetimeHint::ShortLivedBatch,
        Some("cross_batch_cache") => ObjectLifetimeHint::CrossBatchCache,
        Some("long_lived_shared") => ObjectLifetimeHint::LongLivedShared,
        Some("large_readonly") => ObjectLifetimeHint::LargeReadonly,
        Some(other) => {
            return Err(WorkloadJsonError(format!("unknown lifetime_hint `{other}`")));
        }
    };

    let intent = WorkloadIntent {
        scenario_id: obj.get("scenario_id").and_then(|v| v.as_str()).map(str::to_string),
        source,
        phase: obj.get("phase").and_then(|v| v.as_str()).map(str::to_string),
        pause_budget_ms: obj.get("pause_budget_ms").and_then(|v| v.as_u64()).map(|v| v as u32),
        heap_soft_limit_bytes: obj.get("heap_soft_limit_bytes").and_then(|v| v.as_u64()),
        allow_heavy_collection: obj.get("allow_heavy_collection").and_then(|v| v.as_bool()),
        preferred_mode,
        lifetime_hint,
    };

    // 复用控制器校验（紧暂停 + 吞吐冲突）
    let mut probe = nyar_gc::StrategyController::new();
    probe.set_process_intent(intent.clone()).map_err(|error: IntentError| WorkloadJsonError(error.to_string()))?;

    Ok(intent)
}

fn gc_mode_label(mode: GcMode) -> &'static str {
    match mode {
        GcMode::MarkSweep => "mark_sweep",
        GcMode::GenerationalLowLatency => "generational_low_latency",
        GcMode::ThroughputBatch => "throughput_batch",
        GcMode::ConcurrentMarkReserved => "concurrent_mark_reserved",
    }
}

/// 将当前策略与 ConcurrentTrace / 晋升证据合成可序列化 JSON 包络。
pub fn snapshot_gc_evidence(vm: &crate::OdinVm) -> serde_json::Value {
    use serde_json::{Value, json};

    let decision = vm.last_strategy_decision().map(|d| {
        json!({
            "mode": gc_mode_label(d.mode),
            "scenario_id": d.scenario_id,
            "reason": d.reason,
            "unknown_dimensions": d.unknown_dimensions,
            "hints": {
                "phase": d.hints.phase,
                "pause_budget_ms": d.hints.pause_budget_ms,
                "heap_soft_limit_bytes": d.hints.heap_soft_limit_bytes,
                "allow_heavy_collection": d.hints.allow_heavy_collection,
            }
        })
    });

    let transitions: Vec<Value> = vm
        .strategy_transition_history()
        .iter()
        .map(|t| {
            json!({
                "sequence": t.sequence,
                "from_mode": t.from_mode.map(gc_mode_label),
                "to_mode": gc_mode_label(t.decision.mode),
                "scenario_id": t.decision.scenario_id,
            })
        })
        .collect();

    let poll = vm.last_trace_poll();
    let hs = vm.last_root_handshake();
    let promo = vm.last_promotion_failure().map(|f| {
        json!({
            "survivor_count": f.survivor_count,
            "tenured_live_before": f.tenured_live_before,
            "tenured_soft_capacity": f.tenured_soft_capacity,
            "reason": f.reason,
        })
    });

    json!({
        "decision": decision,
        "transitions": transitions,
        "gray_budget_per_slice": vm.gray_budget_per_slice(),
        "max_trace_slices_per_poll": vm.max_trace_slices_per_poll(),
        "trace_poll": {
            "slices_run": poll.slices_run,
            "gray_scanned": poll.gray_scanned,
            "budget_exhausted": poll.budget_exhausted,
            "gray_pending_before": poll.gray_pending_before,
        },
        "root_handshake": {
            "stack_slots": hs.stack_slots,
            "frame_local_slots": hs.frame_local_slots,
            "global_slots": hs.global_slots,
            "frame_coroutines": hs.frame_coroutines,
            "host_roots": hs.host_roots,
            "gray_after_roots": hs.gray_after_roots,
        },
        "relocate_has_moves": vm.last_relocate_map().has_moves(),
        "promotion_failure": promo,
    })
}
