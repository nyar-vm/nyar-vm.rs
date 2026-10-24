//! Legion 程序化宿主 API（N-API / 内嵌调用）；**不是** CLI argv 封装。

use std::path::{Path, PathBuf};

use miette::{Result, miette};
use rusty_valkyrie::CanonicalTarget;
use serde::Serialize;

use crate::cmds::{build::compile_plan, project_input::resolve_project_path};
use legion_workspace::planner::{BuildRequest, WorkspaceResolver};

/// `legion plan` 结构化摘要。
#[derive(Debug, Clone, Serialize)]
pub struct LegionPlanSummary {
    /// 项目名。
    pub project: String,
    /// 编译目标平台字符串。
    pub target: String,
    /// workspace 根目录路径。
    pub workspace_root: String,
    /// 产物输出目录路径。
    pub output_dir: String,
    /// 参与构建的源码文件数。
    pub source_count: usize,
    /// 语义源码组数量。
    pub semantic_group_count: usize,
    /// 直接依赖数量。
    pub dependency_count: usize,
    /// 各语义源码组的摘要列表。
    pub groups: Vec<LegionSemanticGroupSummary>,
}

/// 语义源码组摘要。
#[derive(Debug, Clone, Serialize)]
pub struct LegionSemanticGroupSummary {
    /// 语义源码组对应的依赖键（`std` / `core` 或依赖名）。
    pub dependency_key: String,
    /// 该组包含的源码文件数。
    pub file_count: usize,
}

/// `legion build` 结构化摘要。
#[derive(Debug, Clone, Serialize)]
pub struct LegionBuildSummary {
    /// 项目名。
    pub project: String,
    /// 编译目标平台字符串。
    pub target: String,
    /// 产物输出目录路径。
    pub output_dir: String,
    /// 落盘产物路径列表。
    pub artifacts: Vec<String>,
    /// 产物入口符号名（可能为空）。
    pub entry_symbol: Option<String>,
}

/// 解析项目并生成 build plan，不进入 Compiler。
pub fn plan_project(project_dir: impl AsRef<Path>, target: CanonicalTarget) -> Result<LegionPlanSummary> {
    let project_input = resolve_project_path(project_dir.as_ref())?;
    let workspace = WorkspaceResolver::discover_for_project(&project_input)?;
    let request = BuildRequest { project_dir: project_input, target, output_dir: None };
    let plan = workspace.build_plan(&request)?;
    if plan.project.source_files.is_empty() {
        return Err(miette!("没有找到任何源码文件"));
    }
    Ok(LegionPlanSummary {
        project: plan.project.name.clone(),
        target: plan.project.build_target.target.to_string(),
        workspace_root: plan.workspace_root.display().to_string(),
        output_dir: plan.output_dir.display().to_string(),
        source_count: plan.project.source_files.len(),
        semantic_group_count: plan.project.semantic_source_groups.len(),
        dependency_count: plan.project.dependencies.len(),
        groups: plan
            .project
            .semantic_source_groups
            .iter()
            .map(|group| LegionSemanticGroupSummary { dependency_key: group.dependency_key.clone(), file_count: group.source_files.len() })
            .collect(),
    })
}

/// 解析、编译并落盘产物。
pub fn build_project(project_dir: impl AsRef<Path>, target: CanonicalTarget, output_dir: Option<PathBuf>) -> Result<LegionBuildSummary> {
    let project_input = resolve_project_path(project_dir.as_ref())?;
    let workspace = WorkspaceResolver::discover_for_project(&project_input)?;
    let request = BuildRequest { project_dir: project_input, target, output_dir };
    let plan = workspace.build_plan(&request)?;
    if plan.project.source_files.is_empty() {
        return Err(miette!("没有找到任何源码文件"));
    }
    let report = compile_plan(&plan, false)?;
    let artifacts = report.artifacts.artifacts.iter().map(|artifact| plan.output_dir.join(&artifact.path).display().to_string()).collect();
    Ok(LegionBuildSummary {
        project: plan.project.name.clone(),
        target: plan.project.build_target.target.to_string(),
        output_dir: plan.output_dir.display().to_string(),
        artifacts,
        entry_symbol: report.entry_symbol.clone(),
    })
}
