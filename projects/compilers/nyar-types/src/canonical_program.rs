//! Canonical program success types and compile pipeline stages (ADR 0011).
//!
//! Failures use **structured diagnostics** (a *family* of diagnostic types sharing a
//! common contract) — not a single struct named `StructuredDiagnostics`.

use crate::semantic_ids::{EvidenceId, ItemInstanceId, NominalInstanceId, TypeId};
use std::collections::BTreeMap;

/// One structured diagnostic record (minimum contract fields).
///
/// Concrete compile stages may wrap or extend this; the category is plural.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticRecord {
    /// Stable machine code (e.g. `SMIR006`).
    pub code: String,
    /// Severity label (`error` / `warning` / …).
    pub severity: String,
    /// Pipeline stage that produced the diagnostic.
    pub stage: CompileStage,
    /// Owning module / package symbol when known.
    pub module: String,
    /// Human message (not used for semantic decisions).
    pub message: String,
    /// Deterministic sort key.
    pub stable_sort_key: String,
}

/// A non-empty structured diagnostics payload (category, not a singleton type name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuredDiagnosticSet {
    /// Ordered diagnostic records.
    pub records: Vec<DiagnosticRecord>,
}

impl StructuredDiagnosticSet {
    /// Construct from one or more records. Empty sets are not allowed for `Err`.
    pub fn from_records(records: Vec<DiagnosticRecord>) -> Option<Self> {
        if records.is_empty() { None } else { Some(Self { records }) }
    }
}

/// Stages of the one-way compile / analysis / processing stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CompileStage {
    /// Package AST.
    Ast,
    /// HIR elaboration / overload / evidence solving (may use work types).
    Hir,
    /// Package semantic MIR + SPI (M1).
    SemanticMir,
    /// Cross-package link with selected std adaptors.
    LinkTime,
    /// Validated Semantic MIR (M2).
    ValidateMir,
    /// Sparse representation / layout planning.
    RepresentationPlan,
    /// Target-private plan.
    BackendPrivatePlan,
    /// Artifact emit.
    Emit,
}

/// Result alias for pipeline stages: success value or structured diagnostics category.
pub type StageResult<T> = Result<T, StructuredDiagnosticSet>;

/// Linked program after adaptor selection and cross-package closure (ADR 0011).
///
/// This is the **success** type entering validated Semantic MIR — not a parallel
/// `FrontendNeutralPlan` / `FragmentSubmission` authority.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinkedSemanticProgram {
    /// Module / package identity key.
    pub module_name: String,
    /// Closed item instances (bodies + evidence reachable).
    pub item_instances: BTreeMap<ItemInstanceId, ItemInstanceRecord>,
    /// Closed nominal ADT instances.
    pub nominal_instances: BTreeMap<NominalInstanceId, NominalInstanceRecord>,
    /// Selected evidence bindings.
    pub evidence: BTreeMap<EvidenceId, EvidenceRecord>,
    /// Semantic type table.
    pub types: BTreeMap<TypeId, TypeRecord>,
}

/// Placeholder item instance row (filled by linker / adaptor selection).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemInstanceRecord {
    /// Stable declaration / adaptor key.
    pub symbol: String,
    /// Optional generic substitution identity.
    pub substitution: Option<String>,
}

/// Placeholder nominal instance row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NominalInstanceRecord {
    /// Nominal type name / identity key.
    pub nominal: String,
    /// Substitution identity key when parametric.
    pub substitution: Option<String>,
}

/// Placeholder evidence row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRecord {
    /// Trait / imply identity.
    pub trait_id: String,
    /// Implementing type identity.
    pub implementing_type: String,
}

/// Placeholder type table row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRecord {
    /// Human-readable / debug key (not a backend carrier).
    pub debug_name: String,
}

/// Validated Semantic MIR package owned by the success path (no embedded diagnostics).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CanonicalSemanticMir {
    /// Owning linked program identity.
    pub module_name: String,
    /// Function count placeholder until CFG bodies are attached by identity.
    pub function_count: u32,
}

/// Top-level canonical success bundle after link + MIR validation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CanonicalProgram {
    /// Linked semantic closure.
    pub linked: LinkedSemanticProgram,
    /// Validated MIR (semantic only).
    pub mir: CanonicalSemanticMir,
}

/// One-way compile stream orchestration points (no God parallel authorities).
///
/// Implementations live in `nyar-language` / `nyar-emitter`; this module only
/// defines the stage contracts.
pub mod pipeline {
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
        CompileStage::BackendPrivatePlan,
        CompileStage::Emit,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic_ids::ItemInstanceId;

    #[test]
    fn canonical_program_is_success_only() {
        let mut linked = LinkedSemanticProgram::default();
        linked.module_name = "demo".into();
        linked.item_instances.insert(ItemInstanceId::from_index(0).unwrap(), ItemInstanceRecord { symbol: "main".into(), substitution: None });
        let program = CanonicalProgram { linked, mir: CanonicalSemanticMir { module_name: "demo".into(), function_count: 1 } };
        assert_eq!(program.mir.function_count, 1);
    }

    #[test]
    fn structured_diagnostic_set_rejects_empty() {
        assert!(StructuredDiagnosticSet::from_records(Vec::new()).is_none());
    }

    #[test]
    fn stage_order_is_one_way() {
        assert_eq!(pipeline::STAGE_ORDER[0], CompileStage::Ast);
        assert_eq!(*pipeline::STAGE_ORDER.last().unwrap(), CompileStage::Emit);
    }
}
