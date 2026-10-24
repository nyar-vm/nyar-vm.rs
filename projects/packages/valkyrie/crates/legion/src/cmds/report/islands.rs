//! Thin wrapper: legion report charts delegate to `asgard::report_build`.

use std::path::Path;

use asgard::report_build::emit_report_chart_islands;
use miette::Result;

use super::{codegen::ChartIslandSpec, project::legion_report_project_dir};

pub use asgard::report_build::{asgard_boot_head_tags, asgard_start_snippet};

/// Emit chart islands under `output_dir` and return `(route, mount_html)` pairs.
pub fn emit_chart_islands(output_dir: &Path, module_stem: &str, charts: &[ChartIslandSpec]) -> Result<Vec<(String, String)>> {
    let project_dir = legion_report_project_dir()?;
    emit_report_chart_islands(&project_dir, output_dir, module_stem, charts)
}
