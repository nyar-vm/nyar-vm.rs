#![warn(missing_docs)]

//! Rust seed 路径共用的最小类型与语义身份。

pub use self::{
    canonical_program::{
        CanonicalArrayInitialization, CanonicalBlock, CanonicalBlockId, CanonicalCallee, CanonicalConstant, CanonicalFunction, CanonicalInstruction, CanonicalMirError, CanonicalOperation,
        CanonicalOperand, CanonicalProgram, CanonicalSemanticMir, CanonicalTerminator, CompiledProgram, CompiledProgramError, CompileStage, DiagnosticRecord, EvidenceRecord,
        FieldRecord, ImportRecord, ItemInstanceRecord, LinkedSemanticProgram, NominalInstanceRecord, StageResult, StructuredDiagnosticSet, TypeRecord, VariantRecord, CanonicalPrimitiveType,
        CanonicalTypeKind, pipeline,
    },
    contract_versions::{IDENTITY_SCHEMA_VERSION, LAYOUT_PLAN_VERSION, MIR_CONTRACT_VERSION, contract_version_fingerprint},
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
    registries::{
        AttributeRegistry, AttributeRegistryError, OperatorRegistry, OperatorRegistryError, builtin_operator, parse_operator_display_name,
    },
    semantic_ids::{
        AttributeId, AttributeRegistration, EffectEdgeId, EffectSiteId, EvidenceId, FieldId, GenericFunctionId, IdKind, ImportCapability,
        ImportIndex, InstructionId, IntrinsicId, ItemId, ItemInstanceId, MirValueDefinition, MirValueId, NominalInstanceId, OperatorFixity,
        ValueIdentity,
        OperatorId, OperatorRegistration, ProvenanceId, SemanticId, SubstitutionId, TypeId, TypeInstanceId, VariantId, builtin_attribute,
        layout_choice,
    },
    source::{Location, Position, SourceID, SourceSpan},
    symbols::{Identifier, NamePath, QualifiedName, SymbolIdentity},
    ty::{NyarFunctionType, NyarType, WitnessObject},
    witness_submission::{WitnessCallEdge, WitnessMethodSlotSubmission, WitnessSubmission},
};
pub use core_surface::{CoreFeature, CoreSurfaceManifest};

pub mod canonical_program;
/// 产物 / cache 合同版本（identity / MIR / layout）。
pub mod contract_versions;
pub mod core_surface;
mod errors;
/// 后端私有的可执行视图，供 lowering 使用。
pub mod executable;
mod external_import;
/// 聚合 / singleton 布局合同，供可执行 lowering 使用。
pub mod layout;
/// 前端、规划器、emitter 与运行时共享的中立可审计合同。
pub mod neutral_contract;
/// 语言装配与后端共享的可空 intrinsic 配置。
pub mod nullable;
/// 可扩展属性 / 运算符注册表。
pub mod registries;
/// 参数化 MIR 语义身份与稀疏 RepresentationPlan。
pub mod semantic_ids;
mod source;
mod symbols;
mod ty;
mod witness_submission;

/// 分析器、规划器与后端共享的稳定能力标签。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CapabilityTag(String);

impl CapabilityTag {
    /// 新建能力标签。
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// 返回标签字符串切片。
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
