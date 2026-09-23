//! 通用测试与 fixture 护栏。

/// 通用 fixture 收集与 sidecar 基线能力。
pub mod fixtures;
/// Legend fixture manifest loading helpers.
pub mod legend_fixtures;
/// 运行时 fixture 的通用结果模型。
pub mod runtime;

pub use self::{
    fixtures::{
        assert_or_regenerate_text_sidecar, assert_or_regenerate_yaml_sidecar, collect_fixture_cases_with_extensions,
        load_optional_text_sidecar, load_optional_yaml_sidecar, regenerate_enabled, text_sidecar_path, yaml_sidecar_path,
    },
    legend_fixtures::{
        LegendFixtureEntry, LegendFixtureManifest, load_legend_fixture_manifest, resolve_legend_fixture_paths, resolve_legend_fixture_targets,
        verify_legend_fixture_case,
    },
    runtime::{
        RuntimeFixtureResult, RuntimeFixtureSpec, load_runtime_fixture_spec, resolve_runtime_fixture_targets, verify_runtime_fixture_case,
        verify_runtime_fixture_spec,
    },
};
