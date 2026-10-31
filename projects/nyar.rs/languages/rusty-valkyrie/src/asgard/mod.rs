#![doc = include_str!("readme.md")]
#![deny(missing_docs)]

pub mod awsl;
pub mod chunk_planning;
pub mod cli;
pub mod codegen;
pub mod compile;
pub mod config_discovery;
pub mod config_model;
pub mod delivery;
pub mod deploy;
pub mod error;
pub mod host;
pub mod host_backend;
pub mod host_validation;
pub mod package;
pub mod pipeline;
pub mod platform_contract;
pub mod publish;
pub mod report_build;
pub mod server_runtime;
pub mod sourcemap;
pub mod ssg;
pub mod static_site;
pub mod tailwind;
pub mod wasm;

pub use compile::HostArtifactKind;
pub use delivery::{PackOptions, PackReport, PackTarget, pack_voa_delivery};
pub use deploy::{DeployPlan, DeployProfile, list_deploy_profiles, print_deploy_plan};
pub use host::HostPlatform;
pub use publish::{PublishReport, PublishTarget, publish_voa};
pub use report_build::{asgard_boot_head_tags, asgard_start_snippet, emit_report_chart_islands};
pub use ssg::{
    generate_static_page,
    project::{read_project_source, render_project_awsl_static, valkyrie_v_roots},
    render_awsl_static, wrap_with_layout,
};
