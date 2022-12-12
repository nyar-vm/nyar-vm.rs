//! Aggregate layout contracts for backend-private executable lowering.
//!
//! These types describe value/reference storage and field offsets. They are not a
//! language-level IR and are not a cross-frontend semantic bus.

use std::collections::{BTreeMap, BTreeSet};

use crate::NyarType;

/// Stable layout identifier referenced by executable instructions and backends.
pub type LayoutId = u32;

/// Physical storage class for aggregate values at lowering time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageKind {
    /// Inline value aggregate: `structure`, tuple, `[T; N]`.
    Value,
    /// Heap / GC reference aggregate: `class`, dynamic `[T]`, trait objects.
    Reference,
}

impl StorageKind {
    /// Returns `true` when this is an inline value aggregate.
    pub fn is_value(self) -> bool {
        matches!(self, Self::Value)
    }

    /// Returns `true` when this is a heap / GC reference aggregate.
    pub fn is_reference(self) -> bool {
        matches!(self, Self::Reference)
    }
}

/// One field slot inside an aggregate layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldLayout {
    /// Field name.
    pub name: String,
    /// Platform field type.
    pub ty: NyarType,
    /// Byte offset within the aggregate.
    pub offset: u32,
    /// Field size in bytes.
    pub size: u32,
    /// Field alignment in bytes.
    pub align: u32,
}

/// Memory layout for one aggregate type (structure, tuple, or fixed array).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggregateLayout {
    /// Layout identifier.
    pub id: LayoutId,
    /// Type simple name (or synthetic tuple/array key).
    pub name: String,
    /// Dot-separated namespace (empty for global).
    pub namespace: String,
    /// Value vs reference storage class.
    pub storage: StorageKind,
    /// Total size in bytes.
    pub size: u32,
    /// Aggregate alignment in bytes.
    pub align: u32,
    /// Field slots in declaration order.
    pub fields: Vec<FieldLayout>,
}

/// Module-wide layout plan consumed by executable lowering and backends.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AggregateLayoutPlan {
    /// Registered layouts.
    pub layouts: Vec<AggregateLayout>,
    /// Names treated as inline value types.
    pub value_type_names: BTreeSet<String>,
    /// Map from layout key / type name to layout id.
    pub type_name_to_layout: BTreeMap<String, LayoutId>,
}

/// Sum type variant layout for nominal lowering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SumVariantLayout {
    /// Variant name.
    pub name: String,
    /// Discriminant tag.
    pub tag: u32,
    /// Optional payload type.
    pub payload_type: Option<NyarType>,
}

/// Sum / enum discriminant layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SumTypeLayout {
    /// Sum type name.
    pub name: String,
    /// Whether this is a unite-style sum.
    pub is_unite: bool,
    /// Tag width in bits/bytes as used by the backend plan.
    pub tag_width: u32,
    /// Variant layouts.
    pub variants: Vec<SumVariantLayout>,
}

/// Semantic identity of a concrete nominal sum instance (`NominalTypeId × TypeArguments`).
///
/// This is the M1/M3 contract key. Bare sum names alone are not identity:
/// `Result<Plan, E>` and `Result<OtherPlan, E>` must remain distinct.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NominalInstanceKey {
    /// Nominal type id (declaration name; package-qualified when available).
    pub nominal: String,
    /// Concrete type arguments. Empty means a monomorphic / non-generic nominal.
    pub type_args: Vec<NyarType>,
}

impl NominalInstanceKey {
    /// Build a key from nominal name and type arguments.
    pub fn new(nominal: impl Into<String>, type_args: Vec<NyarType>) -> Self {
        Self { nominal: nominal.into(), type_args }
    }

    /// Monomorphic / non-generic nominal (empty type arguments).
    pub fn monomorphic(nominal: impl Into<String>) -> Self {
        Self::new(nominal, Vec::new())
    }

    /// Interim Wasm prepare RepresentationId (`sum:Name` / `sum:Name<a,b>`).
    pub fn representation_id(&self) -> RepresentationId {
        RepresentationId(sum_representation_key(&self.nominal, &self.type_args))
    }
}

/// Wasm-planning identity produced from a [`NominalInstanceKey`].
///
/// Layering (strictly one-way):
/// `NominalInstanceKey` → [`RepresentationId`] → GC type index.
/// Emit consumes this id; it must never invent identity from a type index,
/// `anyref`, or bare `layout_id` (ADR 0008 / M3).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RepresentationId(String);

impl RepresentationId {
    /// Borrow the stable string key used in Wasm type registries.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RepresentationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for RepresentationId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Interim Wasm / prepare RepresentationId key for a concrete sum instance.
///
/// Matches V `wasm_sum_representation_key`: `sum:Name` or `sum:Name<a,b>`.
/// Emit must look up this key — never bare short names alone (ADR 0008 / M3).
pub fn sum_representation_key(sum_type: &str, type_args: &[NyarType]) -> String {
    if type_args.is_empty() {
        format!("sum:{sum_type}")
    }
    else {
        let args = type_args.iter().map(nyar_type_layout_key_component).collect::<Vec<_>>().join(",");
        format!("sum:{sum_type}<{args}>")
    }
}

#[cfg(test)]
mod nominal_instance_identity_tests {
    use super::*;
    use crate::Identifier;

    #[test]
    fn distinct_type_args_yield_distinct_representation_ids() {
        let plan = NyarType::Named(Identifier::new("Plan"));
        let other = NyarType::Named(Identifier::new("OtherPlan"));
        let diag = NyarType::Named(Identifier::new("VonDiagnostic"));
        let a = NominalInstanceKey::new("Result", vec![plan.clone(), diag.clone()]);
        let b = NominalInstanceKey::new("Result", vec![other, diag.clone()]);
        let c = NominalInstanceKey::new("Option", vec![plan]);
        let d = NominalInstanceKey::new("Option", vec![diag]);
        assert_ne!(a.representation_id(), b.representation_id());
        assert_ne!(c.representation_id(), d.representation_id());
        assert_ne!(a.representation_id(), c.representation_id());
        // Same shape payload carrier must not collapse identity.
        assert!(a.representation_id().as_str().contains("Plan"));
        assert!(b.representation_id().as_str().contains("OtherPlan"));
    }

    #[test]
    fn package_qualified_nominals_do_not_merge_by_simple_name() {
        let a = NominalInstanceKey::monomorphic("pkg.a.Result");
        let b = NominalInstanceKey::monomorphic("pkg.b.Result");
        let bare = NominalInstanceKey::monomorphic("Result");
        assert_ne!(a.representation_id(), b.representation_id());
        assert_ne!(a.representation_id(), bare.representation_id());
    }

    #[test]
    fn missing_vs_present_type_args_are_distinct() {
        let apply = NominalInstanceKey::new("Option", vec![NyarType::Integer32 { signed: true }]);
        let mono = NominalInstanceKey::monomorphic("Option");
        assert_ne!(apply.representation_id(), mono.representation_id());
    }
}

/// Flags bitmask nominal layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlagsLayout {
    /// Flags type name.
    pub name: String,
}

/// Fixed static field name holding the unique singleton instance.
pub const SINGLETON_INSTANCE_FIELD: &str = "INSTANCE";

/// Eager singleton accessor method name (`instance()`).
pub const SINGLETON_EAGER_ACCESSOR: &str = "instance";

/// Lazy singleton accessor method name (`get_instance()`).
pub const SINGLETON_LAZY_ACCESSOR: &str = "get_instance";

/// Method name recognized as the singleton constructor.
pub const SINGLETON_CONSTRUCTOR_NAME: &str = "init";

/// Method name recognized as the singleton finalizer.
pub const SINGLETON_FINALIZER_NAME: &str = "finalize";

/// Lazy singleton unload accessor method name (`unload()`).
pub const SINGLETON_UNLOAD_ACCESSOR: &str = "unload";

/// Describes the lifecycle contract for a singleton's global instance.
///
/// This plan only carries activation mode and fixed symbol names. Concrete
/// field offsets, field types, and receiver shape still come from the
/// singleton's merged aggregate layout.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SingletonInstancePlan {
    /// Singleton type simple name.
    pub name: String,
    /// Dot-separated namespace (empty for global).
    pub namespace: String,
    /// Static field holding the unique instance. Fixed to [`SINGLETON_INSTANCE_FIELD`] by convention.
    pub instance_field: String,
    /// When `false`, available during fragment activation.
    /// When `true`, materialized on first access and supports `unload`.
    pub is_lazy: bool,
    /// Symbol name of the user-defined constructor (`init`), if any.
    pub constructor_symbol: Option<String>,
    /// Symbol name of the user-defined finalizer (`finalize`), if any.
    pub finalizer_symbol: Option<String>,
}

impl SingletonInstancePlan {
    /// Returns the dot-separated qualified name `namespace.name`.
    pub fn qualified_name(&self) -> String {
        if self.namespace.is_empty() { self.name.clone() } else { format!("{}.{}", self.namespace, self.name) }
    }

    /// Returns the accessor method name based on lazy mode.
    pub fn accessor_method(&self) -> &'static str {
        if self.is_lazy { SINGLETON_LAZY_ACCESSOR } else { SINGLETON_EAGER_ACCESSOR }
    }

    /// Returns whether this plan exposes the lazy deactivation transition.
    pub fn supports_unload(&self) -> bool {
        self.is_lazy
    }

    /// Returns the function symbol for the constructor, if any.
    pub fn constructor_mir_symbol(&self) -> Option<String> {
        self.constructor_symbol.clone()
    }

    /// Returns the function symbol for the finalizer, if any.
    pub fn finalizer_mir_symbol(&self) -> Option<String> {
        self.finalizer_symbol.clone()
    }
}

/// Layout key for a platform [`NyarType`].
pub fn layout_key_for_nyar_type(ty: &NyarType) -> Option<String> {
    match ty {
        NyarType::Named(name) => Some(name.to_string()),
        NyarType::Tuple(types) => Some(format!("__tuple_{}", types.iter().map(nyar_type_layout_key_component).collect::<Vec<_>>().join("_"))),
        NyarType::FixedArray { element, length } => Some(format!("__fixedarray_{length}_{}", nyar_type_layout_key_component(element))),
        _ => None,
    }
}

/// Resolve layout id from a platform [`NyarType`].
pub fn layout_id_for_nyar_type(ty: &NyarType, plan: &AggregateLayoutPlan) -> Option<LayoutId> {
    layout_key_for_nyar_type(ty).and_then(|key| plan.type_name_to_layout.get(&key).copied())
}

/// Stable component string used inside synthetic layout keys.
pub fn nyar_type_layout_key_component(ty: &NyarType) -> String {
    match ty {
        NyarType::Named(name) => name.to_string(),
        NyarType::Boolean => "bool".to_string(),
        NyarType::Character => "char".to_string(),
        NyarType::Integer8 { signed } => format!("i8_{signed}"),
        NyarType::Integer16 { signed } => format!("i16_{signed}"),
        NyarType::Integer32 { signed } => format!("i32_{signed}"),
        NyarType::Integer64 { signed } => format!("i64_{signed}"),
        NyarType::Integer128 { signed } => format!("i128_{signed}"),
        NyarType::Float32 => "f32".to_string(),
        NyarType::Float64 => "f64".to_string(),
        NyarType::Utf8 => "utf8".to_string(),
        NyarType::Utf16 => "utf16".to_string(),
        NyarType::Tuple(types) => format!("tuple_{}", types.iter().map(nyar_type_layout_key_component).collect::<Vec<_>>().join("_")),
        NyarType::FixedArray { element, length } => {
            format!("arr{length}_{}", nyar_type_layout_key_component(element))
        }
        NyarType::Array(inner) => format!("heaparr_{}", nyar_type_layout_key_component(inner)),
        NyarType::Apply(base, args) => {
            if args.is_empty() {
                nyar_type_layout_key_component(base)
            }
            else {
                format!(
                    "{}<{}>",
                    nyar_type_layout_key_component(base),
                    args.iter().map(nyar_type_layout_key_component).collect::<Vec<_>>().join(",")
                )
            }
        }
        _ => "ref".to_string(),
    }
}
