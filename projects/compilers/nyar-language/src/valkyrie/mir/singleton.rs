//! Singleton instance lifecycle plans for downstream materialization.
//!
//! Contract (shared by HIR, MIR, Driver, and downstream consumers):
//! - Generic singletons are forbidden. Semantic analysis rejects `singleton Foo<T>`; the
//!   `generics` field on [`HirSingleton`] is retained for AST fidelity only.
//! - Every singleton has exactly one global instance field named [`SINGLETON_INSTANCE_FIELD`].
//! - Eager singletons become available as part of fragment activation; accessor is
//!   [`SINGLETON_EAGER_ACCESSOR`].
//! - Lazy singletons initialize on first access; accessor is [`SINGLETON_LAZY_ACCESSOR`].
//! - A singleton may define a constructor ([`SINGLETON_CONSTRUCTOR_NAME`]) that runs when the
//!   unique instance is created, and a finalizer ([`SINGLETON_FINALIZER_NAME`]) that runs when
//!   a lazy singleton is deactivated. Eager singletons stay available for the whole fragment
//!   lifetime.
//! - Lazy singletons expose an `unload` operation ([`SINGLETON_UNLOAD_ACCESSOR`]): the finalizer
//!   (if any) is invoked, the global slot is cleared, and the next accessor call re-runs the
//!   constructor to reactivate a fresh instance. Eager singletons do not expose this lifecycle
//!   transition.
//! - Layout and field types come only from [`merge_singleton_field_layouts`] / [`singleton_as_struct`]
//!   merged into `aggregate_layouts`; downstream consumers read [`SingletonInstancePlan`] via
//!   `singleton_instances`.
//! - `singleton_instances` answers "when/how to materialize the unique instance"; `aggregate_layouts`
//!   answers "what fields/signatures/receiver layout that instance has". Backends must not invent
//!   singleton-only field or signature rules outside these two payloads.
//! - This shared contract only fixes naming plus activation/deactivation boundaries; synchronization
//!   and storage strategy remain target-side decisions.

use std::collections::BTreeMap;

use crate::{
    hir::trait_system::{build_witness_method_entries, satisfy_named_trait},
    types::{
        Identifier,
        hir::{HirField, HirModule, HirSingleton, HirStruct, HirTrait},
        witness::{MethodEntry, ModuleId, TypeId},
    },
};

pub use nyar_types::{
    SINGLETON_CONSTRUCTOR_NAME, SINGLETON_EAGER_ACCESSOR, SINGLETON_FINALIZER_NAME, SINGLETON_INSTANCE_FIELD, SINGLETON_LAZY_ACCESSOR,
    SINGLETON_UNLOAD_ACCESSOR, SingletonInstancePlan,
};

/// Collect struct + singleton field layouts for overload resolution and type inference.
pub fn collect_aggregate_field_map(module: &HirModule) -> BTreeMap<Identifier, Vec<HirField>> {
    let mut fields = module.structs.iter().map(|item| (item.name.clone(), item.fields.clone())).collect::<BTreeMap<_, _>>();
    for singleton in &module.singletons {
        fields.insert(singleton.name.clone(), singleton.fields.clone());
    }
    fields
}

/// Collect singleton instance plans from a HIR module.
pub fn collect_singleton_instance_plans(module: &HirModule) -> Vec<SingletonInstancePlan> {
    module.singletons.iter().map(plan_from_singleton).collect()
}

/// Map singleton type name to its instance accessor method.
pub fn singleton_accessor_map(module: &HirModule) -> std::collections::BTreeMap<String, String> {
    module
        .singletons
        .iter()
        .map(|singleton| {
            let plan = plan_from_singleton(singleton);
            (singleton.name.to_string(), plan.accessor_method().to_string())
        })
        .collect()
}

/// Merge singleton field layouts into struct field layout maps used by MIR.
///
/// This keeps singleton field typing aligned with ordinary aggregate layout lookup so later
/// lowering and downstream consumers can resolve fields through one layout source.
pub fn merge_singleton_field_layouts(
    module: &HirModule,
    layouts: &mut std::collections::BTreeMap<String, Vec<(String, crate::types::hir::ValkyrieType)>>,
) {
    for singleton in &module.singletons {
        layouts.insert(singleton.name.to_string(), singleton.fields.iter().map(|field| (field.name.to_string(), field.ty.clone())).collect());
    }
}

/// Collect singleton method return types keyed by method name.
pub fn collect_singleton_return_types(module: &HirModule) -> std::collections::BTreeMap<String, crate::types::hir::ValkyrieType> {
    let mut types = std::collections::BTreeMap::new();
    for singleton in &module.singletons {
        for method in &singleton.methods {
            types.insert(method.name.to_string(), method.return_type.clone());
        }
    }
    types
}

fn plan_from_singleton(singleton: &HirSingleton) -> SingletonInstancePlan {
    let constructor_symbol = singleton.constructor.as_ref().map(|func| format!("{}.{}", singleton.name, func.name));
    let finalizer_symbol = singleton.finalizer.as_ref().map(|func| format!("{}.{}", singleton.name, func.name));
    SingletonInstancePlan {
        name: singleton.name.to_string(),
        namespace: singleton.namespace.iter().map(|part| part.as_str()).collect::<Vec<_>>().join("."),
        instance_field: singleton.instance_name.to_string(),
        is_lazy: singleton.is_lazy,
        constructor_symbol,
        finalizer_symbol,
    }
}

/// View a singleton as a reference `HirStruct` for aggregate layout planning.
pub fn singleton_as_struct(singleton: &HirSingleton) -> HirStruct {
    HirStruct {
        name: singleton.name.clone(),
        namespace: singleton.namespace.clone(),
        doc: singleton.doc.clone(),
        generics: singleton.generics.clone(),
        parents: singleton.parents.clone(),
        fields: singleton.fields.clone(),
        methods: singleton.methods.clone(),
        properties: singleton.properties.clone(),
        visibility: singleton.visibility,
        is_value_type: false,
        is_abstract: false,
        is_sealed: false,
        is_final: false,
        is_open: false,
        abstract_methods: Vec::new(),
        abstract_properties: Vec::new(),
        derives: singleton.derives.clone(),
    }
}

/// Witness entries produced for a singleton's trait implementation.
///
/// Each entry records the singleton name, the trait it implements, and the
/// method entries that dispatch to the singleton's own methods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SingletonWitnessEntries {
    /// The singleton type simple name.
    pub singleton_name: Identifier,
    /// The trait simple name implemented by the singleton.
    pub trait_name: Identifier,
    /// Method entries dispatching to the singleton's methods.
    pub method_entries: Vec<MethodEntry>,
}

/// Collect witness table entries for singletons that implement traits.
///
/// For each singleton with trait parents (for example `singleton Settings: Serializable`),
/// this function converts the singleton to its struct view via [`singleton_as_struct`]
/// and runs named-trait satisfaction against the trait definitions in the module.
/// The resulting method entries dispatch to the singleton's own methods.
///
/// Singletons without trait parents produce no entries. Explicit `impl` blocks
/// targeting a singleton type are honored through the module's `impls` list,
/// matching the struct satisfaction path.
///
/// This closes the gap where [`crate::hir::trait_system::TraitModuleView`] only
/// resolves struct candidates: callers can merge these entries into a
/// [`crate::types::witness::WitnessRegistry`] so singleton methods become
/// dispatchable through witness tables.
pub fn collect_singleton_witness_entries(module: &HirModule) -> Vec<SingletonWitnessEntries> {
    let trait_map: BTreeMap<Identifier, &HirTrait> = module.traits.iter().map(|trait_def| (trait_def.name.clone(), trait_def)).collect();
    let mut entries = Vec::new();
    for singleton in &module.singletons {
        let struct_view = singleton_as_struct(singleton);
        for parent in &singleton.parents {
            let Some(trait_name) = parent.name.parts().last().cloned()
            else {
                continue;
            };
            let Some(trait_def) = trait_map.get(&trait_name)
            else {
                continue;
            };
            let Ok(witness) = satisfy_named_trait(&struct_view, trait_def, &module.impls)
            else {
                continue;
            };
            let type_id = singleton_type_id(&singleton.name);
            let method_entries = build_witness_method_entries(&witness, ModuleId::LOCAL, singleton.namespace.clone(), type_id);
            entries.push(SingletonWitnessEntries { singleton_name: singleton.name.clone(), trait_name, method_entries });
        }
    }
    entries
}

/// Computes a stable [`TypeId`] for a singleton from its name.
///
/// The hash is reduced to 32 bits so it fits in [`TypeId`]. Collisions are
/// acceptable for the witness-entry collection path because the singleton
/// name is also carried in [`SingletonWitnessEntries::singleton_name`].
fn singleton_type_id(name: &Identifier) -> TypeId {
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };
    let mut hasher = DefaultHasher::new();
    name.as_str().hash(&mut hasher);
    TypeId::new((hasher.finish() & 0xFFFF_FFFF) as u32)
}
