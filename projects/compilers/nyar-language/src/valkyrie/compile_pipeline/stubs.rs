//! Scaffold / fail-closed stage implementations for the correct stream.
//!
//! Empty* stages produce success shells so the driver can be unit-tested without
//! reviving God parallel authorities. FailClosed* stages hard-fail until real
//! link / M2 / planner land — preferred default for production entry points.

use nyar_types::{
    CanonicalProgram, CanonicalSemanticMir, CompileStage, LinkedSemanticProgram, StageResult,
    layout_choice::RepresentationPlan,
    pipeline::{LinkStage, RepresentationPlanStage, ValidateMirStage},
};

use super::diagnostics::fail_stage;

/// Linker that returns an empty closed program shell (tests / scaffolding only).
#[derive(Debug, Clone, Default)]
pub struct EmptyLinker {
    /// Module identity written into the linked shell.
    pub module_name: String,
}

impl LinkStage for EmptyLinker {
    fn link(&self) -> StageResult<LinkedSemanticProgram> {
        Ok(LinkedSemanticProgram { module_name: self.module_name.clone(), ..LinkedSemanticProgram::default() })
    }
}

/// Validator that wraps linked identity into a empty-body [`CanonicalProgram`].
///
/// Does **not** claim M2 completeness — function bodies / Invoke are not attached.
#[derive(Debug, Clone, Default)]
pub struct EmptyCanonicalValidator;

impl ValidateMirStage for EmptyCanonicalValidator {
    fn validate(&self, linked: &LinkedSemanticProgram) -> StageResult<CanonicalProgram> {
        Ok(CanonicalProgram {
            linked: linked.clone(),
            mir: CanonicalSemanticMir { module_name: linked.module_name.clone(), function_count: 0 },
        })
    }
}

/// Planner that returns an empty sparse [`RepresentationPlan`] (no CFG rewrite).
#[derive(Debug, Clone, Default)]
pub struct EmptyRepresentationPlanner;

impl RepresentationPlanStage for EmptyRepresentationPlanner {
    fn plan(&self, _program: &CanonicalProgram) -> StageResult<RepresentationPlan> {
        Ok(RepresentationPlan::default())
    }
}

/// Production-shaped linker until real adaptor selection exists.
#[derive(Debug, Clone, Default)]
pub struct FailClosedLinker {
    /// Module name for diagnostic attribution.
    pub module_name: String,
}

impl LinkStage for FailClosedLinker {
    fn link(&self) -> StageResult<LinkedSemanticProgram> {
        fail_stage(
            CompileStage::LinkTime,
            "PIPE001",
            &self.module_name,
            "LinkStage not wired: std adaptor selection and item-instance closure are required before Invoke",
        )
    }
}

/// Production-shaped M2 validator until verifier consumes envelope MIR.
#[derive(Debug, Clone, Default)]
pub struct FailClosedValidator;

impl ValidateMirStage for FailClosedValidator {
    fn validate(&self, linked: &LinkedSemanticProgram) -> StageResult<CanonicalProgram> {
        fail_stage(
            CompileStage::ValidateMir,
            "PIPE002",
            &linked.module_name,
            "ValidateMirStage not wired: M2 verifier must reject unknown constructs fail-closed",
        )
    }
}

/// Production-shaped planner until stable-ID side tables are populated.
#[derive(Debug, Clone, Default)]
pub struct FailClosedPlanner;

impl RepresentationPlanStage for FailClosedPlanner {
    fn plan(&self, program: &CanonicalProgram) -> StageResult<RepresentationPlan> {
        fail_stage(
            CompileStage::RepresentationPlan,
            "PIPE003",
            &program.linked.module_name,
            "RepresentationPlanStage not wired: sparse plans must key only stable semantic ids",
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::valkyrie::compile_pipeline::{
        CompilePipeline, EmptyCanonicalValidator, EmptyLinker, EmptyRepresentationPlanner, FailClosedLinker, FailClosedPlanner, FailClosedValidator,
    };
    use nyar_types::CompileStage;

    #[test]
    fn empty_scaffold_runs_analysis_and_processing() {
        let pipeline =
            CompilePipeline::new(EmptyLinker { module_name: "scaffold".into() }, EmptyCanonicalValidator, EmptyRepresentationPlanner);
        let outcome = pipeline.run_through_representation_plan().expect("scaffold ok");
        assert_eq!(outcome.program.linked.module_name, "scaffold");
        assert!(outcome.representation.invoke_lowerings.is_empty());
    }

    #[test]
    fn fail_closed_linker_rejects_before_validate() {
        let pipeline = CompilePipeline::new(FailClosedLinker { module_name: "x".into() }, FailClosedValidator, FailClosedPlanner);
        let err = pipeline.run_analysis().expect_err("must fail closed");
        assert_eq!(err.records[0].code, "PIPE001");
        assert_eq!(err.records[0].stage, CompileStage::LinkTime);
    }
}
