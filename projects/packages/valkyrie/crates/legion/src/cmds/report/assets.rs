//! 报告静态资源：从 `valkyrie.v/projects/legion._/projects/legion.report` 加载（SSG 源）。

use asgard::{
    codegen::StaticRenderResult,
    ssg::project::{read_project_source, render_project_awsl_static},
};
use miette::Result;
use serde_json::Value;

use super::project::legion_report_project_dir;

/// 报告基础样式。
pub fn report_base_css() -> Result<String> {
    let project = legion_report_project_dir()?;
    read_project_source(&project, "assets/base.css")
}

/// 静态渲染报告页面 AWSL（`pages/*.awsl`）。
pub fn render_report_page(relative: &str, component_name: &str, data: &Value) -> Result<StaticRenderResult> {
    let project = legion_report_project_dir()?;
    render_project_awsl_static(&project, relative, component_name, data)
}
