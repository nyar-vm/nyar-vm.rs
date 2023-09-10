//! Compile pipeline driver: link → validate → sparse representation plan.
//!
//! BackendPrivatePlan / Emit remain intentionally unwired here (backend crates).

use nyar_types::{
    CanonicalProgram, LinkedSemanticProgram, StageResult,
    layout_choice::RepresentationPlan,
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
    /// Canonical program (owned copy for downstream private plan).
    pub program: CanonicalProgram,
    /// Sparse layout / invoke / evidence choices keyed by stable ids.
    pub representation: RepresentationPlan,
}

/// Compiler 在进入装配层前必须持有的完整语义成功产物。
///
/// `CanonicalProgram` 与 `RepresentationPlan` 是同一次编译的配对结果。
/// 装配层不得重新生成其中任一项，也不得只把它们当作前置校验。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilerArtifact {
    /// 已验证的 Canonical Semantic MIR 与链接事实。
    pub program: CanonicalProgram,
    /// 与该程序严格配对的稀疏表示计划。
    pub representation: RepresentationPlan,
}

impl CompilerArtifact {
    /// 从已完成处理阶段的结果取得 Compiler 成功产物。
    pub fn from_processing(outcome: ProcessingOutcome) -> Self {
        Self { program: outcome.program, representation: outcome.representation }
    }
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
        Ok(ProcessingOutcome { program: program.clone(), representation })
    }

    /// 先跑分析，再进入稀疏 representation planning。
    pub fn run_through_representation_plan(&self) -> StageResult<ProcessingOutcome> {
        let analysis = self.run_analysis()?;
        self.run_processing(&analysis.program)
    }
}
