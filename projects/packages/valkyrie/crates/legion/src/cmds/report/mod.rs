//! Legion 报告生成（AWSL SSG + asgard hydrate 图表岛）。

mod assets;
mod codegen;
mod islands;
mod models;
mod project;
mod render;
mod standalone;
mod write;

pub use models::{BenchReport, BenchResultRow, CoverageFeatureEntry, CoverageReport, TestResultEntry};
pub use render::{render_bench_report, render_coverage_report, render_test_report};
pub use standalone::{build_standalone_html, finish_standalone_report};
pub use write::atomic_write_all_text;
