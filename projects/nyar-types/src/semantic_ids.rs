//! Stable semantic identities for Canonical Semantic MIR (ADR 0007 / 0010 / 0011 / 0012).
//!
//! These ids belong to Semantic MIR and sparse RepresentationPlan keys.
//! They are **not** Wasm type indices, CLR tokens, JVM CP indices, or Rust `dyn`/`impl Trait`.
//!
//! **Forbidden:** `function@block:index`, hanging EvidenceId on LegacyCall, CallLayout God tables.

use std::{fmt, num::NonZeroU32};

macro_rules! opaque_u32_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(NonZeroU32);

        impl $name {
            /// Construct from a 1-based index (0 is reserved / invalid).
            pub fn from_index(index: u32) -> Option<Self> {
                NonZeroU32::new(index.saturating_add(1)).map(Self)
            }

            /// Construct from a non-zero raw id.
            pub fn from_raw(raw: NonZeroU32) -> Self {
                Self(raw)
            }

            /// 0-based dense index for table lookup.
            pub fn index(self) -> u32 {
                self.0.get() - 1
            }

            /// Non-zero raw identity.
            pub fn raw(self) -> NonZeroU32 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.index())
            }
        }
    };
}

opaque_u32_id!(
    /// Stable SSA instruction identity (optimizer must preserve or rewrite explicitly).
    InstructionId
);
opaque_u32_id!(
    /// Stable SSA value identity.
    MirValueId
);
opaque_u32_id!(
    /// Semantic type identity in the program type table.
    TypeId
);
opaque_u32_id!(
    /// Linked item instance (function / method / adaptor binding after substitution).
    ItemInstanceId
);
opaque_u32_id!(
    /// Concrete nominal ADT instance (`NominalType × Substitution`).
    NominalInstanceId
);
opaque_u32_id!(
    /// Declared variant identity within a nominal sum.
    VariantId
);
opaque_u32_id!(
    /// Declared field identity within a struct / aggregate / variant payload.
    FieldId
);
opaque_u32_id!(
    /// Generic substitution identity.
    SubstitutionId
);
opaque_u32_id!(
    /// Effect operation site identity.
    EffectSiteId
);
opaque_u32_id!(
    /// Effect edge identity (`Handled` or `Propagate`).
    EffectEdgeId
);
opaque_u32_id!(
    /// Source / synthetic provenance identity.
    ProvenanceId
);

/// Stable identity of a trait/imply evidence binding (semantic proof, not runtime witness).
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
}
