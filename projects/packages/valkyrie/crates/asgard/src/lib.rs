//! VOA（crate 名）— Asgard GUI 编译与 dist 打包实现（用户 CLI：`asgard build`）。
//!
//! **框架名是 Asgard**（`valkyrie.v/projects/asgard`）；本 crate 负责 AWSL 解析、RenderIR 降级、
//! 按 platform 编译逻辑/UI 二进制并写入 dist。交付组装见 `asgard pack`。

#![deny(missing_docs)]

pub mod awsl;
pub mod cli;
pub mod codegen;
pub mod compile;
pub mod config;
pub mod debug_sidecar;
pub mod delivery;
pub mod deploy;
pub mod deps;
pub mod dev_server;
pub mod host;
pub mod host_backend;
pub mod host_validation;
pub mod package;
pub mod pipeline;
pub mod platform_contract;
pub mod publish;
pub mod report_build;
pub mod sourcemap;
pub mod ssg;
pub mod tailwind;
pub mod wasm;

pub use compile::HostArtifactKind;
pub use delivery::{PackOptions, PackReport, PackTarget, pack_voa_delivery};
pub use deploy::{DeployPlan, DeployProfile, list_deploy_profiles, print_deploy_plan};
pub use host::HostPlatform;
pub use pipeline::{CompileOptions, CompileReport, IslandKind, awsl_island_kind, compile_voa_project};
pub use publish::{PublishReport, PublishTarget, publish_voa};
pub use report_build::{asgard_boot_head_tags, asgard_start_snippet, emit_report_chart_islands};
pub use ssg::{
    generate_static_page,
    project::{read_project_source, render_project_awsl_static, valkyrie_v_roots},
    render_awsl_static, wrap_with_layout,
};
