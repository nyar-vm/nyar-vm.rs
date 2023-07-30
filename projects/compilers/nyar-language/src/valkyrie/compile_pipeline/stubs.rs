//! Fail-closed stage implementations for the correct stream.
//!
//! 未实现的阶段必须失败；不得用空壳程序伪造完整合同。

use nyar_types::{
    CanonicalProgram, CompileStage, LinkedSemanticProgram, StageResult,
    layout_choice::RepresentationPlan,
    pipeline::{LinkStage, RepresentationPlanStage, ValidateMirStage},
};

use super::diagnostics::fail_stage;

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
    use crate::valkyrie::compile_pipeline::{CompilePipeline, FailClosedLinker, FailClosedPlanner, FailClosedValidator};
    use nyar_types::CompileStage;

    #[test]
    fn fail_closed_linker_rejects_before_validate() {
        let pipeline = CompilePipeline::new(FailClosedLinker { module_name: "x".into() }, FailClosedValidator, FailClosedPlanner);
        let err = pipeline.run_analysis().expect_err("must fail closed");
        assert_eq!(err.records[0].code, "PIPE001");
        assert_eq!(err.records[0].stage, CompileStage::LinkTime);
    }
}
