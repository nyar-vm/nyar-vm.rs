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
    let value: serde_json::Value =
        serde_json::from_str(source).map_err(|error| WorkloadJsonError(format!("invalid JSON: {error}")))?;
    let obj = value
        .as_object()
        .ok_or_else(|| WorkloadJsonError("workload intent must be a JSON object".into()))?;

    let source = match obj.get("source").and_then(|v| v.as_str()).unwrap_or("project_config") {
        "project_config" => IntentSource::ProjectConfig,
        "deployment" => IntentSource::Deployment,
        "phase_event" => IntentSource::PhaseEvent,
        "profile" => IntentSource::Profile,
        other => return Err(WorkloadJsonError(format!("unknown intent source `{other}`"))),
    };

    let preferred_mode = match obj.get("preferred_mode").and_then(|v| v.as_str()) {
        None => None,
        Some("mark_sweep") => Some(GcMode::MarkSweep),
        Some("generational_low_latency") => Some(GcMode::GenerationalLowLatency),
        Some("throughput_batch") => Some(GcMode::ThroughputBatch),
        Some("concurrent_mark_reserved") => Some(GcMode::ConcurrentMarkReserved),
        Some(other) => return Err(WorkloadJsonError(format!("unknown preferred_mode `{other}`"))),
    };

    let lifetime_hint = match obj.get("lifetime_hint").and_then(|v| v.as_str()) {
        None | Some("unspecified") => ObjectLifetimeHint::Unspecified,
        Some("short_lived_batch") => ObjectLifetimeHint::ShortLivedBatch,
        Some("cross_batch_cache") => ObjectLifetimeHint::CrossBatchCache,
        Some("long_lived_shared") => ObjectLifetimeHint::LongLivedShared,
        Some("large_readonly") => ObjectLifetimeHint::LargeReadonly,
        Some(other) => return Err(WorkloadJsonError(format!("unknown lifetime_hint `{other}`"))),
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
    probe
        .set_process_intent(intent.clone())
        .map_err(|error: IntentError| WorkloadJsonError(error.to_string()))?;

    Ok(intent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/workload").join(name)
    }

    #[test]
    fn parses_bundled_scenario_fixtures() {
        for name in ["online-request.json", "offline-batch.json", "resident-service.json"] {
            let text = fs::read_to_string(fixture(name)).unwrap_or_else(|e| panic!("read {name}: {e}"));
            let intent = parse_workload_intent_json(&text).unwrap_or_else(|e| panic!("parse {name}: {e}"));
            assert!(intent.scenario_id.is_some(), "{name} missing scenario_id");
            assert!(intent.pause_budget_ms.is_some(), "{name} missing pause_budget_ms");
        }
    }

    #[test]
    fn parses_online_request_shape() {
        let intent = parse_workload_intent_json(
            r#"{
              "scenario_id": "online-request",
              "source": "project_config",
              "phase": "request",
              "pause_budget_ms": 5,
              "heap_soft_limit_bytes": 67108864,
              "allow_heavy_collection": false,
              "preferred_mode": "generational_low_latency",
              "lifetime_hint": "short_lived_batch"
            }"#,
        )
        .expect("parse");
        assert_eq!(intent.pause_budget_ms, Some(5));
        assert_eq!(intent.preferred_mode, Some(GcMode::GenerationalLowLatency));
    }

    #[test]
    fn rejects_hard_conflict_in_json() {
        let err = parse_workload_intent_json(
            r#"{
              "preferred_mode": "throughput_batch",
              "pause_budget_ms": 5
            }"#,
        )
        .expect_err("conflict");
        assert!(err.to_string().contains("ThroughputBatch"));
    }
}
