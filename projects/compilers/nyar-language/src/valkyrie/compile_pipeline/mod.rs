//! 单向编译**分析**与**处理**流。
//!
//! ```text
//! Analysis:   AST → HIR → Semantic MIR → Link → Validate (M2) → CanonicalProgram
//! Processing: CanonicalProgram → RepresentationPlan → TargetExecutionPlan → Emit
//! ```
//!
//! 本模块只负责编排。不得复活并行权威
//! （旧前端计划、`FragmentSubmission` 体旁路、God Call 字段）。
//! 依赖闭包完成后，由同一入口校验 Semantic MIR 并生产表示规划成功载荷。
//! 目标 preparation 与旧装配成功链的替换尚未完成。

mod backend_bundle;
mod canonical;
mod context;
mod diagnostics;
mod driver;
mod envelope_checks;
mod evidence;
mod host_bindings;
mod link;
mod representation;

pub use backend_bundle::{
    CompilerArtifactReport, CompilerBuildBundle, compile_source_groups_to_artifacts, compile_source_groups_to_backend_bundle,
};
pub use canonical::canonical_program_from_semantic_mir;
pub use context::{CompilerBuildContext, CompilerHostProviderBinding};
pub use diagnostics::{diagnostic, fail_stage};
pub use driver::compile_linked_semantic_mir;
pub use envelope_checks::{check_function_envelopes, check_instruction_envelope, expected_result_count};
pub use evidence::{CompilerCompileEvidence, generated_source_hash, host_provider_binding_hash_for_bindings, semantic_closure_hash_for_groups};
pub use host_bindings::apply_host_provider_bindings;
pub use link::{link_reachable_dependency_mir, merge_aggregate_layouts, merge_entry_group_surface};
pub use representation::CanonicalRepresentationPlanner;

use nyar_types::CompileStage;

/// Which half of the one-way stream a stage belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StreamHalf {
    /// Produce / close semantic facts (through `CanonicalProgram`).
    Analysis,
    /// Consume canonical facts into layout / private plan / artifacts.
    Processing,
}

/// Map a [`CompileStage`] to analysis vs processing.
pub fn stream_half(stage: CompileStage) -> StreamHalf {
    match stage {
        CompileStage::Ast | CompileStage::Hir | CompileStage::SemanticMir | CompileStage::LinkTime | CompileStage::ValidateMir => {
            StreamHalf::Analysis
        }
        CompileStage::RepresentationPlan | CompileStage::TargetExecutionPlan | CompileStage::Emit => StreamHalf::Processing,
    }
}

/// Documented analysis stage order (subset of [`nyar_types::pipeline::STAGE_ORDER`]).
pub const ANALYSIS_STAGE_ORDER: &[CompileStage] =
    &[CompileStage::Ast, CompileStage::Hir, CompileStage::SemanticMir, CompileStage::LinkTime, CompileStage::ValidateMir];

/// Documented processing stage order.
pub const PROCESSING_STAGE_ORDER: &[CompileStage] = &[CompileStage::RepresentationPlan, CompileStage::TargetExecutionPlan, CompileStage::Emit];
