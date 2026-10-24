//! Legion 装配层 crate 根：workspace/项目规划、构建缓存与产物装配，以及 `vcc` CLI 入口。
#![deny(missing_docs)]

pub mod api;
pub mod artifact_formats;
pub mod cache;
pub mod cli;
pub mod cmds;
pub mod forward;
pub mod unity_export;
pub mod von;

pub use api::{LegionBuildSummary, LegionPlanSummary, LegionSemanticGroupSummary, build_project, plan_project};
pub use artifact_formats::{artifact_format_from_extension, artifact_format_slug, artifact_formats_for_publish_format};
pub use cmds::{
    doc::generate_for_project,
    spy::{SpyMode, SpyOptions, SpyTargetOptions, SpyTargetPlatform, run as run_spy},
};
pub use legion_workspace::{
    AutoLinkConfig, BuildPluginSpec, BuildTargetSpec, DependencySpec, LocalLegionConfig, ProjectArtifactKind, ProjectManifest,
    PublishTargetSpec, RunnerBinding, WorkspaceDefaults, WorkspaceManifest,
};

/// On-disk layout for Legion projects (`legion.von` / `legions.von` / `legion-lock.von` / `~/.valkyrie`).
pub const LEGION_PROJECT_LAYOUT: nyar_package_manager::ProjectLayout = nyar_package_manager::ProjectLayout {
    package_manifest: "legion.von",
    workspace_manifest: "legions.von",
    ignore_file: "legion.ignore",
    lockfile: "legion-lock.von",
    home_dirname: ".valkyrie",
    home_env: "VALKYRIE_HOME",
    token_env_vars: &["VALKYRIE_TOKEN", "LEGION_TOKEN"],
    entry_aliases: no_entry_aliases,
};

/// 本地依赖覆盖配置（类似 Rust `.cargo/config.toml`），默认位于 `.config/legion/legions.von`。
pub const LOCAL_LEGION_CONFIG: &str = ".config/legion/legions.von";

fn no_entry_aliases(_: &str) -> &'static [&'static str] {
    &[]
}

pub use legion_workspace::{
    planner::{
        BuildPlan, BuildRequest, PlannedDependency, PlannedHostContract, PlannedHostProvider, PlannedProject, PlannedSemanticSourceGroup,
        WorkspaceResolver, collect_project_root_v_files, collect_project_v_files, collect_test_build_sources, collect_test_v_files,
        project_uses_single_script_layout,
    },
    script,
};
pub use nyar_language::printer::{to_string as write_von, to_string_indented as write_von_indented};
pub use rusty_valkyrie::{
    ArtifactPolicy, CanonicalAbi, CanonicalArch, CanonicalSpecification, CanonicalTarget, CanonicalTargetParseError, CanonicalVendor,
    EntryPolicy, PublishFormat, RunnerFamily, RunnerSelector, TargetHostKind, TargetMode, TargetProfile, WrapStrategy,
};
pub use von::{VonError, VonParser, VonValue, from_str as parse_von, from_value as parse_von_value};
