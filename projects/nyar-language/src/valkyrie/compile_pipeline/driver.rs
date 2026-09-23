//! Compile pipeline driver: link → validate → sparse representation plan.
//!
//! BackendPrivatePlan / Emit remain intentionally unwired here (backend crates).

use nyar_types::{
    CanonicalProgram, LinkedSemanticProgram, StageResult,
    layout_choice::RepresentationPlan,
    pipeline::{LinkStage, RepresentationPlanStage, ValidateMirStage},
};

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
    /// Canonical program (owned copy for downstream private plan).
    pub program: CanonicalProgram,
    /// Sparse layout / invoke / evidence choices keyed by stable ids.
    pub representation: RepresentationPlan,
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
        let representation = self.planner.plan(program)?;
        Ok(ProcessingOutcome { program: program.clone(), representation })
    }

    /// Run analysis then sparse representation planning (through ADR 0011 processing entry).
    pub fn run_through_representation_plan(&self) -> StageResult<ProcessingOutcome> {
        let analysis = self.run_analysis()?;
        self.run_processing(&analysis.program)
    }
}
