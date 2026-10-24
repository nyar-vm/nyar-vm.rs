//! 单向编译流的阶段 trait 合同。
//!
//! 实现位于前端 crate（`examples/rusty-valkyrie`）/ `nyar-emitter`；本模块只定义阶段合同与阶段顺序。

use super::{CanonicalProgram, CompileStage, LinkedSemanticProgram, StageResult};
use crate::semantic_ids::layout_choice::RepresentationPlan;

/// Analysis / link stage: HIR elaboration consumed → linked program.
pub trait LinkStage {
    /// Produce a closed linked program or structured diagnostics.
    fn link(&self) -> StageResult<LinkedSemanticProgram>;
}

/// M2 validation stage.
pub trait ValidateMirStage {
    /// Validate linked program into canonical MIR success type.
    fn validate(&self, linked: &LinkedSemanticProgram) -> StageResult<CanonicalProgram>;
}

/// Representation planning stage (sparse side tables only).
pub trait RepresentationPlanStage {
    /// Plan layouts without rewriting CFG or inventing semantics.
    fn plan(&self, program: &CanonicalProgram) -> StageResult<RepresentationPlan>;
}

/// Documented stage order for maintainers / agents.
pub const STAGE_ORDER: &[CompileStage] = &[
    CompileStage::Ast,
    CompileStage::Hir,
    CompileStage::SemanticMir,
    CompileStage::LinkTime,
    CompileStage::ValidateMir,
    CompileStage::RepresentationPlan,
    CompileStage::TargetExecutionPlan,
    CompileStage::Emit,
];
