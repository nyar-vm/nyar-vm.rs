//! Migrated from `src/cmds/report/codegen.rs` (`tests`).

use super::*;

#[test]
fn test_report_emits_col_series_items() {
    let results = vec![TestResultEntry { name: "a".into(), status: "pass".into(), error: None, target: "legion".into() }];
    let charts = charts_for_test_report(&results);
    assert_eq!(charts.len(), 1);
    assert_eq!(charts[0].route, "chart-status");
    assert_eq!(charts[0].title, "Test Status");
    assert_eq!(charts[0].series.len(), 3);
    assert!(charts[0].series.iter().any(|s| s.key == "pass" && s.fill == "#22c55e"));
    assert!(charts[0].series.iter().all(|s| s.height_pct >= 4.0));
    assert!(!format!("{:?}", charts[0].series).contains("test_status_series"));
}
