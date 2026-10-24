//! Report chart data → `ColSeriesItem`（Rust 数据面；UI 在 AWSL + asgard.plotter）。

use nyar_analyzer::report::{ColSeriesItem, HydratedChartSpec};

use super::models::{BenchResultRow, CoverageFeatureEntry, CoverageReport, TestResultEntry};

const BENCH_TOP_N: usize = 12;

const PALETTE: &[&str] = &["#3873ad", "#22c55e", "#ef4444", "#f97316", "#8b5cf6", "#14b8a6", "#eab308", "#64748b"];

/// One hydrate chart island.
pub type ChartIslandSpec = HydratedChartSpec;

/// Charts for a test report (empty when no results).
pub fn charts_for_test_report(results: &[TestResultEntry]) -> Vec<ChartIslandSpec> {
    if results.is_empty() {
        return Vec::new();
    }
    let passed = results.iter().filter(|r| r.status == "pass").count() as f64;
    let failed = results.iter().filter(|r| r.status == "fail" || r.status == "compile_error").count() as f64;
    let skipped = results.iter().filter(|r| r.status == "skip").count() as f64;
    vec![HydratedChartSpec { route: "chart-status".into(), title: "Test Status".into(), series: test_status_series(passed, failed, skipped) }]
}

/// Charts for a coverage report.
pub fn charts_for_coverage_report(report: &CoverageReport) -> Vec<ChartIslandSpec> {
    let mut charts = Vec::new();
    if report.total > 0 {
        charts.push(HydratedChartSpec {
            route: "chart-coverage-balance".into(),
            title: "Coverage Balance".into(),
            series: coverage_balance_series(report.covered as f64, report.total as f64),
        });
    }
    if !report.features.is_empty() {
        charts.push(HydratedChartSpec {
            route: "chart-coverage-features".into(),
            title: "Feature Coverage".into(),
            series: coverage_features_series(&report.features),
        });
    }
    charts
}

/// Charts for a bench report (empty when no rows).
pub fn charts_for_bench_report(rows: &[BenchResultRow]) -> Vec<ChartIslandSpec> {
    if rows.is_empty() {
        return Vec::new();
    }
    vec![
        HydratedChartSpec { route: "chart-bench-runtime".into(), title: "Bench Runtime (ms)".into(), series: bench_series(rows, true) },
        HydratedChartSpec { route: "chart-bench-compile".into(), title: "Bench Compile (ms)".into(), series: bench_series(rows, false) },
    ]
}

fn format_series_value(value: f64, unit: &str) -> String {
    if unit.is_empty() { format!("{value}") } else { format!("{value} {unit}") }
}

fn palette_color(index: usize) -> &'static str {
    if PALETTE.is_empty() { "#3873ad" } else { PALETTE[index % PALETTE.len()] }
}

fn status_color(status: &str) -> &'static str {
    match status {
        "pass" | "通过" | "covered" | "已覆盖" => "#22c55e",
        "fail" | "失败" | "uncovered" | "未覆盖" | "compile_error" => "#ef4444",
        "skip" | "跳过" => "#f97316",
        _ => "#3873ad",
    }
}

fn col_series_items(keys: &[&str], labels: &[&str], values: &[f64], fills: &[&str], unit: &str) -> Vec<ColSeriesItem> {
    let max_value = values.iter().copied().fold(1.0_f64, f64::max);
    keys.iter()
        .enumerate()
        .map(|(index, key)| {
            let value = values.get(index).copied().unwrap_or(0.0);
            let pct = (value / max_value) * 100.0;
            let height_pct = if pct < 4.0 { 4.0 } else { pct };
            let fill = fills.get(index).copied().unwrap_or_else(|| palette_color(index));
            let label = labels.get(index).copied().unwrap_or(key);
            ColSeriesItem {
                key: (*key).to_string(),
                label: label.to_string(),
                value_text: format_series_value(value, unit),
                fill: fill.to_string(),
                height_pct,
            }
        })
        .collect()
}

fn test_status_series(passed: f64, failed: f64, skipped: f64) -> Vec<ColSeriesItem> {
    col_series_items(
        &["pass", "fail", "skip"],
        &["pass", "fail", "skip"],
        &[passed, failed, skipped],
        &[status_color("pass"), status_color("fail"), status_color("skip")],
        "tests",
    )
}

fn coverage_balance_series(covered: f64, total: f64) -> Vec<ColSeriesItem> {
    let uncovered = total - covered;
    col_series_items(
        &["covered", "uncovered"],
        &["covered", "uncovered"],
        &[covered, uncovered],
        &[status_color("covered"), status_color("uncovered")],
        "features",
    )
}

fn coverage_features_series(features: &[CoverageFeatureEntry]) -> Vec<ColSeriesItem> {
    let take: Vec<_> = features.iter().take(16).collect();
    let flags: Vec<f64> = take.iter().map(|f| if f.covered { 1.0 } else { 0.0 }).collect();
    let max_value = flags.iter().copied().fold(1.0_f64, f64::max);
    take.iter()
        .enumerate()
        .map(|(index, f)| {
            let value = flags.get(index).copied().unwrap_or(0.0);
            let pct = (value / max_value) * 100.0;
            let height_pct = if pct < 4.0 { 4.0 } else { pct };
            let covered = value >= 0.5;
            let fill = if covered { status_color("covered") } else { status_color("uncovered") };
            let unit = if covered { "covered" } else { "missed" };
            ColSeriesItem {
                key: format!("f{index}"),
                label: f.display.clone(),
                value_text: format_series_value(value, unit),
                fill: fill.to_string(),
                height_pct,
            }
        })
        .collect()
}

fn bench_series(rows: &[BenchResultRow], runtime: bool) -> Vec<ColSeriesItem> {
    let mut ranked: Vec<&BenchResultRow> = rows.iter().collect();
    if runtime {
        ranked.sort_by(|a, b| b.runtime_ms.total_cmp(&a.runtime_ms));
    }
    else {
        ranked.sort_by(|a, b| b.compile_ms.total_cmp(&a.compile_ms));
    }
    ranked.truncate(BENCH_TOP_N);

    let mut keys = Vec::new();
    let mut labels = Vec::new();
    let mut values = Vec::new();
    let fill_offset = if runtime { 0 } else { 2 };
    for (index, row) in ranked.iter().enumerate() {
        let prefix = if runtime { "r" } else { "c" };
        keys.push(format!("{prefix}{index}"));
        labels.push(bench_label(row));
        let ms = if runtime { row.runtime_ms } else { row.compile_ms };
        values.push(ms);
    }
    let fills: Vec<&str> = (0..keys.len()).map(|i| palette_color(i + fill_offset)).collect();
    let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    col_series_items(&key_refs, &label_refs, &values, &fills, "ms")
}

fn bench_label(row: &BenchResultRow) -> String {
    if row.project.is_empty() { format!("{}@{}", row.test, row.target) } else { format!("{}::{}", row.project, row.test) }
}
#[cfg(test)]
#[path = "../../../tests/path_wired/cmds_report_codegen.rs"]
mod cmds_report_codegen_tests;
