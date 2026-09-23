//! One-way compile **analysis** and **processing** stream (ADR 0011 / 0012).
//!
//! ```text
//! Analysis:   AST → HIR → Semantic MIR → Link → Validate (M2) → CanonicalProgram
//! Processing: CanonicalProgram → RepresentationPlan → BackendPrivatePlan → Emit
//! ```
//!
//! This module owns **orchestration only**. It must not revive parallel authorities
//! (`FrontendNeutralPlan`, `FragmentSubmission` body bypass, God Call fields).
//! Invoke / ItemInstance wiring stays forbidden until CanonicalProgram + stable IDs
//! are the sole success path consumers actually use.

mod diagnostics;
mod driver;
mod envelope_checks;
mod stubs;

pub use diagnostics::{diagnostic, fail_stage};
pub use driver::{AnalysisOutcome, CompilePipeline, ProcessingOutcome};
pub use envelope_checks::{check_function_envelopes, check_instruction_envelope, expected_result_count};
pub use stubs::{EmptyCanonicalValidator, EmptyLinker, EmptyRepresentationPlanner, FailClosedLinker, FailClosedPlanner, FailClosedValidator};

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
        CompileStage::RepresentationPlan | CompileStage::BackendPrivatePlan | CompileStage::Emit => StreamHalf::Processing,
    }
}

/// Documented analysis stage order (subset of [`nyar_types::pipeline::STAGE_ORDER`]).
pub const ANALYSIS_STAGE_ORDER: &[CompileStage] =
    &[CompileStage::Ast, CompileStage::Hir, CompileStage::SemanticMir, CompileStage::LinkTime, CompileStage::ValidateMir];

/// Documented processing stage order.
pub const PROCESSING_STAGE_ORDER: &[CompileStage] = &[CompileStage::RepresentationPlan, CompileStage::BackendPrivatePlan, CompileStage::Emit];

#[cfg(test)]
mod tests {
    use super::*;
    use nyar_types::pipeline::STAGE_ORDER;

    #[test]
    fn analysis_then_processing_covers_full_order() {
        let mut joined: Vec<CompileStage> = ANALYSIS_STAGE_ORDER.to_vec();
        joined.extend_from_slice(PROCESSING_STAGE_ORDER);
        assert_eq!(joined.as_slice(), STAGE_ORDER);
    }

    #[test]
    fn stream_half_splits_at_representation_plan() {
        assert_eq!(stream_half(CompileStage::ValidateMir), StreamHalf::Analysis);
        assert_eq!(stream_half(CompileStage::RepresentationPlan), StreamHalf::Processing);
    }
}
