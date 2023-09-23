//! Compile pipeline driver: link → validate → sparse representation plan.
//!
//! BackendPrivatePlan / Emit remain intentionally unwired here (backend crates).

use nyar_types::{
    CanonicalProgram, CompiledProgram, LinkedSemanticProgram, StageResult,
    pipeline::{LinkStage, RepresentationPlanStage, ValidateMirStage},
};

use super::diagnostics::fail_stage;

/// Analysis-stream success through M2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisOutcome {
    /// Closed linked program.
    pub linked: LinkedSemanticProgram,
    /// Validated canonical bundle.
    pub program: CanonicalProgram,
}

/// Processing-stream success through sparse representation planning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessingOutcome {
    /// Canonical 与 RepresentationPlan 的不可拆分成功载荷。
    pub artifact: CompiledProgram,
}

/// One-way driver over stage contracts from `nyar_types::pipeline`.
///
/// Does not accept `FrontendNeutralPlan` or `FragmentSubmission` as inputs.
#[derive(Debug, Clone)]
pub struct CompilePipeline<L, V, P> {
    linker: L,
    validator: V,
    planner: P,
}

impl<L, V, P> CompilePipeline<L, V, P>
where
    L: LinkStage,
    V: ValidateMirStage,
    P: RepresentationPlanStage,
{
    /// Construct a pipeline from stage implementations.
    pub fn new(linker: L, validator: V, planner: P) -> Self {
        Self { linker, validator, planner }
    }

    /// Run analysis half: link → validate → [`CanonicalProgram`].
    pub fn run_analysis(&self) -> StageResult<AnalysisOutcome> {
        let linked = self.linker.link()?;
        let program = self.validator.validate(&linked)?;
        Ok(AnalysisOutcome { linked, program })
    }

    /// Run processing half starting from an already-validated program.
    pub fn run_processing(&self, program: &CanonicalProgram) -> StageResult<ProcessingOutcome> {
        if let Err(error) = program.validate() {
            return fail_stage(
                nyar_types::CompileStage::ValidateMir,
                "PIPE004",
                &program.mir.module_name,
                format!("canonical Semantic MIR contract failed: {error:?}"),
            );
        }
        let representation = self.planner.plan(program)?;
        let artifact = CompiledProgram::new(program.clone(), representation).map_err(|error| {
            fail_stage::<()>(
                nyar_types::CompileStage::RepresentationPlan,
                "PIPE005",
                &program.mir.module_name,
                format!("CanonicalProgram 与 RepresentationPlan 合同不一致: {error:?}"),
            )
            .expect_err("fail_stage 必须返回结构化错误")
        })?;
        Ok(ProcessingOutcome { artifact })
    }

    /// 先跑分析，再进入稀疏 representation planning。
    pub fn run_through_representation_plan(&self) -> StageResult<ProcessingOutcome> {
        let analysis = self.run_analysis()?;
        self.run_processing(&analysis.program)
    }
}
