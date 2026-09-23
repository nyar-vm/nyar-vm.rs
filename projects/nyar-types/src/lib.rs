#![warn(missing_docs)]

//! Minimal shared types used by the current Rust bootstrap path.

pub use self::{
    canonical_program::{
        CanonicalProgram, CanonicalSemanticMir, CompileStage, DiagnosticRecord, EvidenceRecord, ItemInstanceRecord, LinkedSemanticProgram,
        NominalInstanceRecord, StageResult, StructuredDiagnosticSet, TypeRecord, pipeline,
    },
    errors::{NyarError, NyarErrorKind},
    executable::{
        ArrayInitialization, Block, BlockRef, CarrierTable, CaseArm, CaseChain, Constant, Continuation, Diagnostic, EffectKind,
        ExecutableFunction, FrameLayout, FrameSlot, Instruction, InstructionKind, Operand, SuspendLoweringPlan, SuspendPoint, SuspendState,
        Terminator, Value, ValueOrigin, ValueRef,
    },
    external_import::{ExternalCallArgument, ExternalCallEdge, ExternalImportLink, InternalCallEdge},
    layout::{
        AggregateLayout, AggregateLayoutPlan, FieldLayout, FlagsLayout, LayoutId, NominalInstanceKey, RepresentationId,
        SINGLETON_CONSTRUCTOR_NAME, SINGLETON_EAGER_ACCESSOR, SINGLETON_FINALIZER_NAME, SINGLETON_INSTANCE_FIELD, SINGLETON_LAZY_ACCESSOR,
        SINGLETON_UNLOAD_ACCESSOR, SingletonInstancePlan, StorageKind, SumTypeLayout, SumVariantLayout, layout_id_for_nyar_type,
        layout_key_for_nyar_type, nyar_type_layout_key_component, sum_representation_key,
    },
    neutral_contract::{
        ArtifactContract, BootstrapStage, EvidencePackage, EvidenceStatus, PrimitiveDefinition, PrimitiveRegistry, Provenance,
        SemanticObservation, SemanticPackageInterface,
    },
    nullable::{FragmentNullableBoolProfile, FragmentNullableIntrinsicKind, FragmentNullableIntrinsicUse, FragmentNullableTryCall},
    semantic_ids::{
        EffectEdgeId, EffectSiteId, EvidenceId, FieldId, GenericFunctionId, InstructionId, ItemInstanceId, MirValueDefinition, MirValueId,
        NominalInstanceId, ProvenanceId, SubstitutionId, TypeId, VariantId, layout_choice,
    },
    source::{Location, Position, SourceID, SourceSpan},
    symbols::{Identifier, NamePath, QualifiedName, SymbolIdentity},
    ty::{NyarFunctionType, NyarType, WitnessObject},
    witness_submission::{WitnessCallEdge, WitnessMethodSlotSubmission, WitnessSubmission},
};
pub use core_surface::{CoreFeature, CoreSurfaceManifest};

pub mod canonical_program;
pub mod core_surface;
mod errors;
/// Backend-private executable views for lowering.
pub mod executable;
mod external_import;
/// Aggregate / singleton layout contracts for executable lowering.
pub mod layout;
/// Neutral, auditable contracts shared by frontends, planners, emitters and runtimes.
pub mod neutral_contract;
/// Nullable intrinsic profiles shared by language assembly and backends.
pub mod nullable;
/// Parametric MIR semantic identities and sparse RepresentationPlan.
pub mod semantic_ids;
mod source;
mod symbols;
mod ty;
mod witness_submission;

/// Stable capability tag shared across analyzers, planners and backends.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CapabilityTag(String);

impl CapabilityTag {
    /// Creates a new capability tag.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the tag as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for CapabilityTag {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for CapabilityTag {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Display for CapabilityTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
