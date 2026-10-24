use odin_vm::*;
use std::{fs, path::PathBuf};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/workload").join(name)
}

#[test]
fn parses_bundled_scenario_fixtures() {
    for name in ["online-request.json", "offline-batch.json", "resident-service.json", "concurrent-interactive.json"] {
        let text = fs::read_to_string(fixture(name)).unwrap_or_else(|e| panic!("read {name}: {e}"));
        let intent = parse_workload_intent_json(&text).unwrap_or_else(|e| panic!("parse {name}: {e}"));
        assert!(intent.scenario_id.is_some(), "{name} missing scenario_id");
        assert!(intent.pause_budget_ms.is_some(), "{name} missing pause_budget_ms");
    }
}

#[test]
fn rejects_bundled_hard_conflict_fixtures() {
    for (name, needle) in
        [("hard-conflict-throughput-tight-pause.json", "ThroughputBatch"), ("hard-conflict-concurrent-heavy.json", "ConcurrentMarkReserved")]
    {
        let text = fs::read_to_string(fixture(name)).unwrap_or_else(|e| panic!("read {name}: {e}"));
        let err = parse_workload_intent_json(&text).expect_err(name);
        assert!(err.to_string().contains(needle), "{name}: {err}");
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
