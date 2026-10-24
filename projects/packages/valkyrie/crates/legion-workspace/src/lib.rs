#![deny(missing_docs)]

//! Legion 与 ASGARD 共同使用的 manifest、依赖闭包和源码快照解析层。

/// `legion.von` / `legions.von` 清单模型与解析错误。
pub mod manifest;
pub mod oak;
/// workspace 发现、依赖解析与构建计划生成。
pub mod planner;
pub mod script;
pub mod source_snapshot;

pub use manifest::{
    AutoLinkConfig, BuildPluginSpec, BuildTargetSpec, DependencySpec, LocalLegionConfig, ProjectArtifactKind, ProjectManifest,
    PublishTargetSpec, RunnerBinding, WorkspaceDefaults, WorkspaceManifest,
};
pub use rusty_valkyrie::{SemanticClosure, plan_semantic_closure};
pub use oak::{labeled_report, labeled_report_with_context, labeled_span, source_point_span};
pub use planner::{
    BuildPlan, BuildRequest, PlannedDependency, PlannedHostContract, PlannedHostProvider, PlannedProject, PlannedSemanticSourceGroup,
    PlannerError, ProjectResolutionMode, WorkspaceResolver, canonical_target, collect_project_root_v_files, collect_project_v_files,
    collect_test_build_sources, collect_test_v_files, project_uses_single_script_layout,
};
pub use source_snapshot::compile_source_snapshot;

/// On-disk layout for Legion projects.
pub const LEGION_PROJECT_LAYOUT: nyar_package_manager::ProjectLayout = nyar_package_manager::ProjectLayout {
    package_manifest: "legion.von",
    workspace_manifest: "legions.von",
    ignore_file: "legion.ignore",
    lockfile: "legion-lock.von",
    home_dirname: ".valkyrie",
    home_env: "VALKYRIE_HOME",
    token_env_vars: &["VALKYRIE_TOKEN", "LEGION_TOKEN"],
    entry_aliases: bootstrap_entry_aliases,
};

/// 本地依赖覆盖配置文件名。
pub const LOCAL_LEGION_CONFIG: &str = ".config/legion/legions.von";

/// Node bootstrap 物理入口别名。
pub fn bootstrap_entry_aliases(physical_entry: &str) -> &'static [&'static str] {
    match physical_entry {
        "legion_legion.mjs" | "legion__main_legion.mjs" => &["legion.mjs"],
        "legion_legion.wasm" | "legion__main_legion.wasm" => &["legion.wasm"],
        _ => &[],
    }
}
