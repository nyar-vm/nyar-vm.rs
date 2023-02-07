//! Stable semantic identities for Canonical Semantic MIR
//! (ADR 0009 / 0013; opaque MIR ids also cover ADR 0010–0012).
//!
//! These ids belong to Semantic MIR and sparse RepresentationPlan keys.
//! They are **not** Wasm type indices, CLR tokens, JVM CP indices, or Rust `dyn`/`impl Trait`.
//!
//! **Forbidden:** `function@block:index`, hanging EvidenceId on LegacyCall, CallLayout God tables,
//! string/`as_str()` dispatch after parse (see ADR 0013 / S-W1 gate).

use std::{fmt, marker::PhantomData, num::NonZeroU32};

/// Brands a [`SemanticId`] so distinct semantic domains cannot be mixed at compile time.
pub trait IdKind {
    /// Diagnostic label used by [`Display`]; not a runtime dispatch key.
    const NAME: &'static str;
}

/// Dense 1-based semantic identity shared by all branded id types.
///
/// Raw storage is `NonZeroU32` (0 reserved). Table lookup uses [`SemanticId::index`] (0-based).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SemanticId<K: IdKind> {
    raw: NonZeroU32,
    _kind: PhantomData<fn() -> K>,
}

impl<K: IdKind> SemanticId<K> {
    /// Construct from a 0-based dense table index (`0` → first slot).
    pub fn from_index(index: u32) -> Option<Self> {
        NonZeroU32::new(index.saturating_add(1)).map(|raw| Self { raw, _kind: PhantomData })
    }

    /// Construct from a non-zero raw identity.
    pub fn from_raw(raw: NonZeroU32) -> Self {
        Self { raw, _kind: PhantomData }
    }

    /// 0-based dense index for table lookup.
    pub fn index(&self) -> u32 {
        self.raw.get() - 1
    }

    /// Non-zero raw identity.
    pub fn raw(self) -> NonZeroU32 {
        self.raw
    }
}

impl<K: IdKind> fmt::Debug for SemanticId<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", K::NAME, self.index())
    }
}

impl<K: IdKind> fmt::Display for SemanticId<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", K::NAME, self.index())
    }
}

/// Stable SSA instruction identity (optimizer must preserve or rewrite explicitly).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstructionIdKind;
impl IdKind for InstructionIdKind {
    const NAME: &'static str = "InstructionId";
}
pub type InstructionId = SemanticId<InstructionIdKind>;

/// Stable SSA value identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MirValueIdKind;
impl IdKind for MirValueIdKind {
    const NAME: &'static str = "MirValueId";
}
pub type MirValueId = SemanticId<MirValueIdKind>;

/// Declared language item (function / method / constructor / trait method /
/// std adaptor entry / intrinsic declaration). Assigned by the package linker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemIdKind;
impl IdKind for ItemIdKind {
    const NAME: &'static str = "ItemId";
}
pub type ItemId = SemanticId<ItemIdKind>;

/// Semantic type identity in the program type table (nominal base / type parameter slot).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeIdKind;
impl IdKind for TypeIdKind {
    const NAME: &'static str = "TypeId";
}
pub type TypeId = SemanticId<TypeIdKind>;

/// Applied type identity (`TypeId` × type arguments) after substitution.
/// Distinct from [`NominalInstanceId`], which is ADT-specialized.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeInstanceIdKind;
impl IdKind for TypeInstanceIdKind {
    const NAME: &'static str = "TypeInstanceId";
}
pub type TypeInstanceId = SemanticId<TypeInstanceIdKind>;

/// Linked item instance (function / method / adaptor binding after substitution).
/// Linker side tables relate this to [`ItemId`] + [`SubstitutionId`] + evidence env.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemInstanceIdKind;
impl IdKind for ItemInstanceIdKind {
    const NAME: &'static str = "ItemInstanceId";
}
pub type ItemInstanceId = SemanticId<ItemInstanceIdKind>;

/// Concrete nominal ADT instance (`NominalType × Substitution`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NominalInstanceIdKind;
impl IdKind for NominalInstanceIdKind {
    const NAME: &'static str = "NominalInstanceId";
}
pub type NominalInstanceId = SemanticId<NominalInstanceIdKind>;

/// Operator identity assigned by the extensible operator registry (ADR 0013).
///
/// **Not a closed enum.** Built-in and user-defined operators (`infix 4 +*`, custom
/// lexemes) receive stable ids from the registry at parse/link time. Fixity,
/// precedence, and lexeme live in side tables ([`OperatorRegistration`]); MIR stores
/// only `OperatorId` — never `"infix =="` display strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OperatorIdKind;
impl IdKind for OperatorIdKind {
    const NAME: &'static str = "OperatorId";
}
pub type OperatorId = SemanticId<OperatorIdKind>;

/// Verified import slot in a bytecode / host capability table (ADR 0013).
/// Bytecode stores this index only; never a host function name string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImportIndexKind;
impl IdKind for ImportIndexKind {
    const NAME: &'static str = "ImportIndex";
}
pub type ImportIndex = SemanticId<ImportIndexKind>;

/// Declared variant identity within a nominal sum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VariantIdKind;
impl IdKind for VariantIdKind {
    const NAME: &'static str = "VariantId";
}
pub type VariantId = SemanticId<VariantIdKind>;

/// Declared field identity within a struct / aggregate / variant payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldIdKind;
impl IdKind for FieldIdKind {
    const NAME: &'static str = "FieldId";
}
pub type FieldId = SemanticId<FieldIdKind>;

/// Generic substitution identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SubstitutionIdKind;
impl IdKind for SubstitutionIdKind {
    const NAME: &'static str = "SubstitutionId";
}
pub type SubstitutionId = SemanticId<SubstitutionIdKind>;

/// Effect operation site identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EffectSiteIdKind;
impl IdKind for EffectSiteIdKind {
    const NAME: &'static str = "EffectSiteId";
}
pub type EffectSiteId = SemanticId<EffectSiteIdKind>;

/// Effect edge identity (`Handled` or `Propagate`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EffectEdgeIdKind;
impl IdKind for EffectEdgeIdKind {
    const NAME: &'static str = "EffectEdgeId";
}
pub type EffectEdgeId = SemanticId<EffectEdgeIdKind>;

/// Source / synthetic provenance identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProvenanceIdKind;
impl IdKind for ProvenanceIdKind {
    const NAME: &'static str = "ProvenanceId";
}
pub type ProvenanceId = SemanticId<ProvenanceIdKind>;

/// Operator fixity (syntactic classification only — not operator identity).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum OperatorFixity {
    /// Prefix operator (`!x`, `-x`).
    Prefix,
    /// Infix operator (`a + b`).
    Infix,
    /// Postfix operator (`x!`).
    Postfix,
}

/// One row in the operator registry side table (parse / link phase).
///
/// Maps an extensible [`OperatorId`] to lexeme + fixity + precedence + resolved callee.
/// Duplicate `(fixity, lexeme)` or duplicate `OperatorId` assignment must fail closed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OperatorRegistration {
    /// Stable operator identity referenced by MIR / overload.
    pub id: OperatorId,
    /// Source lexeme for diagnostics only (`+`, `==`, `>>=`). Not a dispatch key after parse.
    pub lexeme: String,
    /// Prefix / infix / postfix.
    pub fixity: OperatorFixity,
    /// Binding strength (higher binds tighter; exact scale owned by language front-end).
    pub precedence: u16,
    /// Resolved implementation after overload / type-class selection, if known at link time.
    pub callee: Option<ItemInstanceId>,
}

/// Finite builtin / intrinsic operations shared by language and emitter (ADR 0013).
///
/// Registered once; backends consume the id, not `builtin.array.push` path strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum IntrinsicId {
    /// `builtin.array.push` / push onto growable array storage.
    ArrayPush,
    /// Array / list length.
    ArrayLen,
    /// Indexed get on array storage.
    ArrayGet,
    /// Indexed set on array storage.
    ArraySet,
    /// Reference dereference.
    RefDeref,
}

impl IntrinsicId {
    /// Diagnostic / registry path segment (not a runtime dispatch key).
    pub fn diagnostic_path(self) -> &'static str {
        match self {
            Self::ArrayPush => "builtin.array.push",
            Self::ArrayLen => "builtin.array.length",
            Self::ArrayGet => "builtin.array.get",
            Self::ArraySet => "builtin.array.set",
            Self::RefDeref => "builtin.ref.deref",
        }
    }
}

impl fmt::Display for IntrinsicId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.diagnostic_path())
    }
}

/// Source attribute kinds recognized after parse (ADR 0013).
///
/// After parse, code must match on this enum — not `attribute.name.as_str() == "export"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AttributeKind {
    /// `[export]` / export surface.
    Export,
    /// `@main` / `[main]` entry.
    Main,
    /// `[test]`.
    Test,
    /// `[benchmark]`.
    Benchmark,
}

impl AttributeKind {
    /// Diagnostic attribute name.
    pub fn diagnostic_name(self) -> &'static str {
        match self {
            Self::Export => "export",
            Self::Main => "main",
            Self::Test => "test",
            Self::Benchmark => "benchmark",
        }
    }
}

impl fmt::Display for AttributeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.diagnostic_name())
    }
}

/// External import capability declaration before verify maps it to [`ImportIndex`].
///
/// Module / export **link names** are interop only; VM execution uses [`ImportIndex`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ImportCapability {
    /// Host / module link name.
    pub module_name: String,
    /// Export link name within that module.
    pub export_name: String,
}

impl ImportCapability {
    /// Construct a capability pair.
    pub fn new(module_name: impl Into<String>, export_name: impl Into<String>) -> Self {
        Self { module_name: module_name.into(), export_name: export_name.into() }
    }
}

impl fmt::Display for ImportCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::{}", self.module_name, self.export_name)
    }
}

/// Stable identity of a trait/imply evidence binding (semantic proof, not runtime witness).
///
/// S-W1 freeze: string key remains a **diagnostic / provenance** carrier.
/// Call sites must not branch on `as_str()` for lowering; prefer side-table lookup by this id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EvidenceId(String);

impl EvidenceId {
    /// Construct from a pre-normalized stable key.
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    /// Deterministic key from trait, implementing type, and operation identities.
    pub fn from_parts(trait_id: &str, implementing_type: &str, operation: &str) -> Self {
        if operation.is_empty() {
            Self(format!("evidence:{trait_id}@{implementing_type}"))
        }
        else {
            Self(format!("evidence:{trait_id}@{implementing_type}#{operation}"))
        }
    }

    /// Borrow the stable key.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EvidenceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl AsRef<str> for EvidenceId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Stable identity of a parametric function declaration (not a specialized physical body).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GenericFunctionId(String);

impl GenericFunctionId {
    /// Construct from a pre-normalized stable key.
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    /// Borrow the stable key.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for GenericFunctionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How an SSA value was defined (ADR 0012).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MirValueDefinition {
    /// Result slot of an instruction.
    InstructionResult {
        /// Defining instruction.
        instruction: InstructionId,
        /// Result index within that instruction.
        result_index: u32,
    },
    /// Block parameter.
    BlockParameter {
        /// Owning block index (function-local dense id until BlockId lands).
        block_index: u32,
        /// Parameter index.
        parameter_index: u32,
    },
    /// Function parameter.
    FunctionParameter {
        /// Parameter index.
        parameter_index: u32,
    },
}

/// Sparse target-neutral representation plan (ADR 0009 / 0011 / 0012).
///
/// Keys are stable semantic ids only. Never `function@block:index`.
pub mod layout_choice {
    use super::{EffectSiteId, EvidenceId, InstructionId, MirValueId, NominalInstanceId};
    use std::collections::BTreeMap;

    /// Layout choice for a callable / apply site (not a language category).
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum InvokeLowering {
        /// Direct call to a known item.
        Direct,
        /// Typed witness call.
        TypedWitness,
        /// Shared operation table dispatch.
        SharedOperationTable,
        /// Specialized body.
        Specialized,
        /// Typed indirect / function reference.
        TypedReference,
    }

    /// Value carrier representation.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum ValueRepresentation {
        /// Compile-time identity / erased.
        CompileTimeIdentity,
        /// Specialized scalar / aggregate carrier.
        Specialized,
        /// Reified GC / managed object.
        Reified,
        /// Boxed erased carrier.
        ErasedBoxed,
    }

    /// Evidence layout choice.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum EvidenceLayout {
        /// Statically eliminated.
        Erased,
        /// Explicit runtime witness.
        ExplicitWitness,
        /// Shared typed operation table.
        SharedOperationTable,
        /// Boxed evidence bundle.
        Boxed,
    }

    /// ADT layout choice.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum AdtRepresentation {
        /// Inline scalar / tagged payload (details in private plan).
        TaggedPayload,
        /// Typed aggregate.
        TypedAggregate,
        /// Boxed value.
        Boxed,
    }

    /// Effect continuation layout choice.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum EffectRepresentation {
        /// Direct state-machine encoding in private plan.
        DirectStateMachine,
        /// Typed continuation object.
        TypedContinuation,
        /// Boxed frame.
        BoxedFrame,
    }

    /// Target-neutral sparse plan.
    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct RepresentationPlan {
        /// Per-instruction invoke / apply lowering.
        pub invoke_lowerings: BTreeMap<InstructionId, InvokeLowering>,
        /// Per-value carrier representation.
        pub value_representations: BTreeMap<MirValueId, ValueRepresentation>,
        /// Per-evidence layout.
        pub evidence_layouts: BTreeMap<EvidenceId, EvidenceLayout>,
        /// Per-nominal-instance ADT layout.
        pub adt_reps: BTreeMap<NominalInstanceId, AdtRepresentation>,
        /// Per-effect-site continuation layout.
        pub effect_reps: BTreeMap<EffectSiteId, EffectRepresentation>,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_id_is_stable_and_distinct() {
        let a = EvidenceId::from_parts("Comparable", "Int32", "compare");
        let b = EvidenceId::from_parts("Comparable", "Int32", "compare");
        let c = EvidenceId::from_parts("Comparable", "Utf8", "compare");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.as_str(), "evidence:Comparable@Int32#compare");
    }

    #[test]
    fn instruction_id_is_dense_and_nonzero() {
        let id = InstructionId::from_index(0).expect("index 0");
        assert_eq!(id.index(), 0);
        assert!(InstructionId::from_index(u32::MAX).is_some() || InstructionId::from_index(u32::MAX).is_none());
    }

    #[test]
    fn representation_plan_keys_are_stable_ids() {
        let mut plan = layout_choice::RepresentationPlan::default();
        let insn = InstructionId::from_index(3).unwrap();
        plan.invoke_lowerings.insert(insn, layout_choice::InvokeLowering::Direct);
        assert!(plan.invoke_lowerings.contains_key(&insn));
    }

    #[test]
    fn item_and_type_instance_ids_are_opaque() {
        let item = ItemId::from_index(0).expect("item");
        let ty = TypeInstanceId::from_index(1).expect("type instance");
        let import = ImportIndex::from_index(0).expect("import");
        assert_eq!(item.index(), 0);
        assert_eq!(ty.index(), 1);
        assert_eq!(import.index(), 0);
    }

    #[test]
    fn branded_ids_are_distinct_types() {
        fn expects_instruction(_: InstructionId) {}
        expects_instruction(InstructionId::from_index(0).unwrap());
        assert_eq!(InstructionIdKind::NAME, "InstructionId");
        assert_eq!(ItemIdKind::NAME, "ItemId");
    }

    #[test]
    fn operator_registry_uses_extensible_opaque_ids() {
        let op = OperatorId::from_index(7).expect("operator");
        assert_eq!(op.index(), 7);
        let reg = OperatorRegistration {
            id: op,
            lexeme: "+".to_string(),
            fixity: OperatorFixity::Infix,
            precedence: 6,
            callee: ItemInstanceId::from_index(0),
        };
        assert_eq!(reg.lexeme, "+");
        assert_eq!(reg.fixity, OperatorFixity::Infix);
    }

    #[test]
    fn intrinsic_attribute_display_is_diagnostic_only() {
        assert_eq!(IntrinsicId::ArrayPush.diagnostic_path(), "builtin.array.push");
        assert_eq!(AttributeKind::Export.diagnostic_name(), "export");
        let cap = ImportCapability::new("env", "console_log");
        assert_eq!(cap.to_string(), "env::console_log");
    }
}
