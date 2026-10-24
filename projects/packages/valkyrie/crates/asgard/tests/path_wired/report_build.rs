//! Migrated from `src/report_build.rs` (`tests`).

use super::*;

use nyar_analyzer::report::ColSeriesItem;

use crate::ssg::project::{read_project_source, valkyrie_v_roots};

fn legion_report_fixture() -> std::path::PathBuf {
    let roots = valkyrie_v_roots().unwrap();
    let nested = roots[0].join("projects/legion._/projects/legion.report");
    if nested.is_dir() && nested.join("legion.von").is_file() {
        return nested;
    }
    roots[0].join("projects/legion._/projects/legion.report")
}

#[test]
fn expand_inlines_asgard_icol_from_report_project() {
    let project = legion_report_fixture();
    let roots = valkyrie_v_roots().unwrap();
    let Ok(icp) = load_interactive_col_plot(&roots)
    else {
        return;
    };
    let series = vec![ColSeriesItem {
        key: "pass".into(),
        label: "pass".into(),
        value_text: "1.0 tests".into(),
        fill: "#22c55e".into(),
        height_pct: 100.0,
    }];
    let wrapper = fill_chart_placeholders(
        &read_project_source(&project, "charts/chart-status.awsl").unwrap(),
        "\"Test Status\"",
        &series_to_init_literal(&series),
    );
    let awsl = expand_interactive_col_plot(&wrapper, &icp, "chart-status").unwrap();
    assert!(awsl.contains("asgard-icol"));
    assert!(awsl.contains("micro toggle"));
    assert!(!awsl.contains("test_status_series"));
}
