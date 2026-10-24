//! 程序化 N-API 宿主面（`legionPlan` / `legionBuild`），不经 CLI argv。

use std::{path::PathBuf, thread};

use legion::{CanonicalTarget, build_project, plan_project};
use napi_derive::napi;

/// N-API 暴露的单个语义源码组摘要。
#[napi(object)]
pub struct LegionSemanticGroupItem {
    /// 语义源码组对应的依赖键。
    pub dependency_key: String,
    /// 该组包含的源码文件数。
    pub file_count: u32,
}

/// `legionPlan` 入参。
#[napi(object)]
pub struct LegionPlanOptions {
    /// 项目目录。
    pub project_dir: String,
    /// 编译目标平台字符串，缺省为 `clr`。
    pub target: Option<String>,
}

/// `legionPlan` 的结构化返回。
#[napi(object)]
pub struct LegionPlanOutcome {
    /// 是否成功。
    pub ok: bool,
    /// 失败时的错误描述。
    pub error: Option<String>,
    /// 项目名。
    pub project: Option<String>,
    /// 编译目标平台字符串。
    pub target: Option<String>,
    /// workspace 根目录路径。
    pub workspace_root: Option<String>,
    /// 产物输出目录路径。
    pub output_dir: Option<String>,
    /// 参与构建的源码文件数。
    pub source_count: u32,
    /// 语义源码组数量。
    pub semantic_group_count: u32,
    /// 直接依赖数量。
    pub dependency_count: u32,
    /// 各语义源码组摘要。
    pub groups: Vec<LegionSemanticGroupItem>,
}

/// `legionBuild` 入参。
#[napi(object)]
pub struct LegionBuildOptions {
    /// 项目目录。
    pub project_dir: String,
    /// 编译目标平台字符串，缺省为 `clr`。
    pub target: Option<String>,
    /// 产物输出目录，缺省由规划决定。
    pub output_dir: Option<String>,
}

/// `legionBuild` 的结构化返回。
#[napi(object)]
pub struct LegionBuildOutcome {
    /// 是否成功。
    pub ok: bool,
    /// 失败时的错误描述。
    pub error: Option<String>,
    /// 项目名。
    pub project: Option<String>,
    /// 编译目标平台字符串。
    pub target: Option<String>,
    /// 产物输出目录路径。
    pub output_dir: Option<String>,
    /// 落盘产物路径列表。
    pub artifacts: Vec<String>,
    /// 产物入口符号名（可能为空）。
    pub entry_symbol: Option<String>,
}

/// 解析 workspace 并输出 build plan（不进入 Compiler）。
#[napi]
pub fn legion_plan(options: LegionPlanOptions) -> LegionPlanOutcome {
    match parse_target(options.target) {
        Ok(target) => match plan_project(&options.project_dir, target) {
            Ok(summary) => LegionPlanOutcome {
                ok: true,
                error: None,
                project: Some(summary.project),
                target: Some(summary.target),
                workspace_root: Some(summary.workspace_root),
                output_dir: Some(summary.output_dir),
                source_count: summary.source_count as u32,
                semantic_group_count: summary.semantic_group_count as u32,
                dependency_count: summary.dependency_count as u32,
                groups: summary
                    .groups
                    .into_iter()
                    .map(|group| LegionSemanticGroupItem { dependency_key: group.dependency_key, file_count: group.file_count as u32 })
                    .collect(),
            },
            Err(report) => LegionPlanOutcome {
                ok: false,
                error: Some(format!("{report:?}")),
                project: None,
                target: None,
                workspace_root: None,
                output_dir: None,
                source_count: 0,
                semantic_group_count: 0,
                dependency_count: 0,
                groups: Vec::new(),
            },
        },
        Err(error) => LegionPlanOutcome {
            ok: false,
            error: Some(error.reason),
            project: None,
            target: None,
            workspace_root: None,
            output_dir: None,
            source_count: 0,
            semantic_group_count: 0,
            dependency_count: 0,
            groups: Vec::new(),
        },
    }
}

/// 编译项目并落盘产物（结构化结果，不经 CLI）。
#[napi]
pub fn legion_build(options: LegionBuildOptions) -> LegionBuildOutcome {
    let target = match parse_target(options.target) {
        Ok(target) => target,
        Err(error) => {
            return LegionBuildOutcome {
                ok: false,
                error: Some(error.reason),
                project: None,
                target: None,
                output_dir: None,
                artifacts: Vec::new(),
                entry_symbol: None,
            };
        }
    };
    let output_dir = options.output_dir.map(PathBuf::from);
    let project_dir = options.project_dir.clone();
    let worker = thread::Builder::new()
        .name("legion-build".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || build_project(&project_dir, target, output_dir))
        .expect("failed to spawn legion build thread");
    match worker.join().expect("legion build thread panicked") {
        Ok(summary) => LegionBuildOutcome {
            ok: true,
            error: None,
            project: Some(summary.project),
            target: Some(summary.target),
            output_dir: Some(summary.output_dir),
            artifacts: summary.artifacts,
            entry_symbol: summary.entry_symbol,
        },
        Err(report) => LegionBuildOutcome {
            ok: false,
            error: Some(format!("{report:?}")),
            project: None,
            target: None,
            output_dir: None,
            artifacts: Vec::new(),
            entry_symbol: None,
        },
    }
}

fn parse_target(target: Option<String>) -> napi::Result<CanonicalTarget> {
    let raw = target.as_deref().unwrap_or("clr");
    CanonicalTarget::parse(raw).map_err(|error| napi::Error::from_reason(error.to_string()))
}
