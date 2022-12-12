//! Minimal named-trait satisfaction helpers.
//!
//! These helpers intentionally model only the currently settled semantics:
//! named traits are protocols with identity, satisfaction yields a named witness,
//! and explicit impl blocks outrank structural entry.

#![allow(missing_docs)]

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    hir::row::{RowMethodSignature, RowRequirement, RowRequirementError},
    types::{
        Identifier, NamePath,
        hir::{HirImpl, HirModule, HirStruct, HirTrait, ValkyrieType as HirType},
        witness::{MethodEntry, MethodId, MethodPath, ModuleId, TraitId, TypeId, TypeMetadata, WitnessTable, WitnessTableBuilder},
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraitWitnessSource {
    ExplicitImpl,
    Structural,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraitMethodBindingSource {
    ExplicitImpl,
    DefaultMethod,
    Structural,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitMethodBinding {
    pub name: Identifier,
    pub source: TraitMethodBindingSource,
    pub implementation_container: Identifier,
    pub is_default: bool,
}

impl TraitMethodBinding {
    pub fn to_method_entry(&self, method_id: MethodId, module_id: ModuleId, module: Vec<Identifier>, implementing_type: TypeId) -> MethodEntry {
        MethodEntry::new(
            method_id,
            self.name.clone(),
            implementing_type,
            MethodPath::new_with_module(module_id, module, self.implementation_container.clone(), self.name.clone()),
            self.is_default,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedTraitWitness {
    pub trait_path: NamePath,
    pub target: HirType,
    pub source: TraitWitnessSource,
    pub method_bindings: Vec<TraitMethodBinding>,
    pub associated_types: BTreeMap<Identifier, HirType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraitSatisfactionError {
    AmbiguousExplicitImpls { trait_path: NamePath, target: HirType },
    MissingExplicitImplForSuperTraits { trait_path: NamePath },
    MissingExplicitImplForAssociatedTypes { trait_path: NamePath },
    MissingMethodBinding { trait_path: NamePath, name: Identifier },
    AmbiguousMethodBinding { trait_path: NamePath, name: Identifier },
    MissingAssociatedTypeBinding { trait_path: NamePath, name: Identifier },
    AmbiguousAssociatedTypeBinding { trait_path: NamePath, name: Identifier },
    StructuralMismatch { errors: Vec<RowRequirementError> },
    StructuralAmbiguity { errors: Vec<RowRequirementError> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraitModuleError {
    UnknownTrait { name: Identifier },
    UnknownStruct { name: Identifier },
    Satisfaction { error: TraitSatisfactionError },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TraitModuleView {
    traits: BTreeMap<Identifier, HirTrait>,
    structs: BTreeMap<Identifier, HirStruct>,
    impls: Vec<HirImpl>,
}

impl TraitModuleView {
    pub fn from_module(module: &HirModule) -> Self {
        Self {
            traits: module.traits.iter().map(|item| (item.name.clone(), item.clone())).collect(),
            structs: module.structs.iter().map(|item| (item.name.clone(), item.clone())).collect(),
            impls: module.impls.clone(),
        }
    }

    pub fn satisfy_named_trait(&self, candidate_name: &Identifier, trait_name: &Identifier) -> Result<NamedTraitWitness, TraitModuleError> {
        let candidate = self.structs.get(candidate_name).ok_or_else(|| TraitModuleError::UnknownStruct { name: candidate_name.clone() })?;
        let trait_def = self.traits.get(trait_name).ok_or_else(|| TraitModuleError::UnknownTrait { name: trait_name.clone() })?;

        satisfy_named_trait_with_traits(candidate, trait_def, &self.impls, Some(&self.traits))
            .map_err(|error| TraitModuleError::Satisfaction { error })
    }
}

pub fn satisfy_named_trait(
    candidate: &HirStruct,
    trait_def: &HirTrait,
    impls: &[HirImpl],
) -> Result<NamedTraitWitness, TraitSatisfactionError> {
    satisfy_named_trait_with_traits(candidate, trait_def, impls, None)
}

fn satisfy_named_trait_with_traits(
    candidate: &HirStruct,
    trait_def: &HirTrait,
    impls: &[HirImpl],
    known_traits: Option<&BTreeMap<Identifier, HirTrait>>,
) -> Result<NamedTraitWitness, TraitSatisfactionError> {
    let trait_path = NamePath::new(vec![trait_def.name.clone()]);
    let target = HirType::Named(candidate.name.clone());

    if let Some(explicit_impl) = find_matching_explicit_impl(&target, &trait_path, impls)? {
        if let Some(traits) = known_traits {
            ensure_explicit_super_trait_chain(&target, trait_def, impls, traits)?;
        }
        let associated_types = collect_associated_types(explicit_impl, trait_def, &trait_path)?;
        let method_bindings = collect_explicit_method_bindings(candidate, trait_def, explicit_impl, &trait_path)?;

        return Ok(NamedTraitWitness { trait_path, target, source: TraitWitnessSource::ExplicitImpl, method_bindings, associated_types });
    }

    if !trait_def.super_traits.is_empty() {
        return Err(TraitSatisfactionError::MissingExplicitImplForSuperTraits { trait_path });
    }

    if !trait_def.associated_types.is_empty() {
        return Err(TraitSatisfactionError::MissingExplicitImplForAssociatedTypes { trait_path });
    }

    let requirement = RowRequirement::from_methods(
        trait_def
            .methods
            .iter()
            .map(|method| {
                RowMethodSignature::new(
                    method.name.as_str(),
                    callable_surface_params(method.params.iter().map(|param| param.ty.clone()).collect()),
                    method.return_type.clone(),
                )
            })
            .collect(),
    );

    match requirement.check_struct(candidate) {
        Ok(()) => Ok(NamedTraitWitness {
            trait_path,
            target,
            source: TraitWitnessSource::Structural,
            method_bindings: collect_structural_method_bindings(candidate, trait_def),
            associated_types: BTreeMap::new(),
        }),
        Err(errors) if errors.iter().any(is_structural_ambiguity) => Err(TraitSatisfactionError::StructuralAmbiguity { errors }),
        Err(errors) => Err(TraitSatisfactionError::StructuralMismatch { errors }),
    }
}

fn ensure_explicit_super_trait_chain(
    target: &HirType,
    trait_def: &HirTrait,
    impls: &[HirImpl],
    traits: &BTreeMap<Identifier, HirTrait>,
) -> Result<(), TraitSatisfactionError> {
    let mut visited = BTreeSet::new();
    ensure_explicit_super_trait_chain_inner(target, trait_def, impls, traits, &mut visited)
}

fn ensure_explicit_super_trait_chain_inner(
    target: &HirType,
    trait_def: &HirTrait,
    impls: &[HirImpl],
    traits: &BTreeMap<Identifier, HirTrait>,
    visited: &mut BTreeSet<NamePath>,
) -> Result<(), TraitSatisfactionError> {
    for super_trait in &trait_def.super_traits {
        let Some(super_trait_path) = trait_type_to_path(super_trait)
        else {
            continue;
        };
        if !visited.insert(super_trait_path.clone()) {
            continue;
        }
        if find_matching_explicit_impl(target, &super_trait_path, impls)?.is_none() {
            return Err(TraitSatisfactionError::MissingExplicitImplForSuperTraits { trait_path: super_trait_path });
        }
        if let Some(super_trait_name) = super_trait_path.parts().last() {
            if let Some(super_trait_def) = traits.get(super_trait_name) {
                ensure_explicit_super_trait_chain_inner(target, super_trait_def, impls, traits, visited)?;
            }
        }
    }
    Ok(())
}

pub fn build_witness_method_entries(
    witness: &NamedTraitWitness,
    module_id: ModuleId,
    module: Vec<Identifier>,
    implementing_type: TypeId,
) -> Vec<MethodEntry> {
    witness
        .method_bindings
        .iter()
        .enumerate()
        .map(|(index, binding)| binding.to_method_entry(MethodId::new((index + 1) as u32), module_id, module.clone(), implementing_type))
        .collect()
}

pub fn build_witness_table(
    witness: &NamedTraitWitness,
    trait_id: TraitId,
    module_id: ModuleId,
    module: Vec<Identifier>,
    type_metadata: TypeMetadata,
    mut resolve_associated_type_id: impl FnMut(&Identifier, &HirType) -> TypeId,
) -> WitnessTable {
    let implementing_type = type_metadata.type_id;
    let mut builder = WitnessTableBuilder::new_in_module(module_id, trait_id, implementing_type).with_metadata(type_metadata);

    for method_entry in build_witness_method_entries(witness, module_id, module, implementing_type) {
        builder = builder.add_method(method_entry);
    }

    for (name, ty) in &witness.associated_types {
        builder = builder.add_associated_type(name.clone(), resolve_associated_type_id(name, ty));
    }

    builder.build().expect("witness table builder must succeed when type metadata is provided")
}

pub fn resolve_associated_type(
    candidate: &HirStruct,
    trait_def: &HirTrait,
    impls: &[HirImpl],
    name: &Identifier,
) -> Result<HirType, TraitSatisfactionError> {
    let trait_path = NamePath::new(vec![trait_def.name.clone()]);
    let target = HirType::Named(candidate.name.clone());
    let explicit_impl = find_matching_explicit_impl(&target, &trait_path, impls)?
        .ok_or_else(|| TraitSatisfactionError::MissingExplicitImplForAssociatedTypes { trait_path: trait_path.clone() })?;

    find_associated_type_binding(explicit_impl, &trait_path, name)
}

fn find_matching_explicit_impl<'a>(
    target: &HirType,
    trait_path: &NamePath,
    impls: &'a [HirImpl],
) -> Result<Option<&'a HirImpl>, TraitSatisfactionError> {
    let matching = impls.iter().filter(|item| item.target == *target && item.trait_path.as_ref() == Some(trait_path)).collect::<Vec<_>>();

    if matching.is_empty() {
        return Ok(None);
    }

    let maximal = matching
        .iter()
        .copied()
        .filter(|candidate| {
            !matching
                .iter()
                .copied()
                .any(|other| !std::ptr::eq(other, *candidate) && compare_impl_specificity(other, candidate) == ImplSpecificity::MoreSpecific)
        })
        .collect::<Vec<_>>();

    if maximal.len() != 1 {
        return Err(TraitSatisfactionError::AmbiguousExplicitImpls { trait_path: trait_path.clone(), target: target.clone() });
    }

    Ok(maximal.into_iter().next())
}

fn collect_associated_types(
    explicit_impl: &HirImpl,
    trait_def: &HirTrait,
    trait_path: &NamePath,
) -> Result<BTreeMap<Identifier, HirType>, TraitSatisfactionError> {
    let mut associated_types = BTreeMap::new();

    for associated_type in &trait_def.associated_types {
        let concrete = find_associated_type_binding(explicit_impl, trait_path, &associated_type.name)?;
        associated_types.insert(associated_type.name.clone(), concrete);
    }

    Ok(associated_types)
}

fn collect_explicit_method_bindings(
    candidate: &HirStruct,
    trait_def: &HirTrait,
    explicit_impl: &HirImpl,
    trait_path: &NamePath,
) -> Result<Vec<TraitMethodBinding>, TraitSatisfactionError> {
    let mut bindings = Vec::new();
    let mut seen = BTreeSet::new();

    for method in &trait_def.methods {
        let binding = resolve_explicit_method_binding(candidate, trait_def, explicit_impl, trait_path, &method.name, false)?;
        seen.insert(binding.name.clone());
        bindings.push(binding);
    }

    for method in &trait_def.default_methods {
        if !seen.insert(method.name.clone()) {
            continue;
        }
        bindings.push(resolve_explicit_method_binding(candidate, trait_def, explicit_impl, trait_path, &method.name, true)?);
    }

    Ok(bindings)
}

fn collect_structural_method_bindings(candidate: &HirStruct, trait_def: &HirTrait) -> Vec<TraitMethodBinding> {
    let mut bindings = Vec::new();
    let mut seen = BTreeSet::new();

    for method in &trait_def.methods {
        if !seen.insert(method.name.clone()) {
            continue;
        }
        bindings.push(TraitMethodBinding {
            name: method.name.clone(),
            source: TraitMethodBindingSource::Structural,
            implementation_container: candidate.name.clone(),
            is_default: false,
        });
    }

    for method in &trait_def.default_methods {
        if !seen.insert(method.name.clone()) {
            continue;
        }
        bindings.push(TraitMethodBinding {
            name: method.name.clone(),
            source: TraitMethodBindingSource::DefaultMethod,
            implementation_container: trait_def.name.clone(),
            is_default: true,
        });
    }

    bindings
}

fn callable_surface_params(mut params: Vec<HirType>) -> Vec<HirType> {
    if matches!(params.first(), Some(HirType::SelfType)) {
        params.remove(0);
    }
    params
}

fn resolve_explicit_method_binding(
    candidate: &HirStruct,
    trait_def: &HirTrait,
    explicit_impl: &HirImpl,
    trait_path: &NamePath,
    name: &Identifier,
    allow_default: bool,
) -> Result<TraitMethodBinding, TraitSatisfactionError> {
    if let Some(_method) = find_explicit_method(explicit_impl, trait_path, name)? {
        return Ok(TraitMethodBinding {
            name: name.clone(),
            source: TraitMethodBindingSource::ExplicitImpl,
            implementation_container: candidate.name.clone(),
            is_default: false,
        });
    }

    if allow_default && trait_def.default_methods.iter().any(|method| method.name == *name) {
        return Ok(TraitMethodBinding {
            name: name.clone(),
            source: TraitMethodBindingSource::DefaultMethod,
            implementation_container: trait_def.name.clone(),
            is_default: true,
        });
    }

    Err(TraitSatisfactionError::MissingMethodBinding { trait_path: trait_path.clone(), name: name.clone() })
}

fn find_explicit_method<'a>(
    explicit_impl: &'a HirImpl,
    trait_path: &NamePath,
    name: &Identifier,
) -> Result<Option<&'a crate::types::hir::HirFunction>, TraitSatisfactionError> {
    let mut matching = explicit_impl.methods.iter().filter(|item| item.name == *name);
    let first = matching.next();

    if matching.next().is_some() {
        return Err(TraitSatisfactionError::AmbiguousMethodBinding { trait_path: trait_path.clone(), name: name.clone() });
    }

    Ok(first)
}

fn find_associated_type_binding(explicit_impl: &HirImpl, trait_path: &NamePath, name: &Identifier) -> Result<HirType, TraitSatisfactionError> {
    let mut matching = explicit_impl.associated_type_impls.iter().filter(|item| item.name == *name);

    let first = matching.next().map(|item| item.concrete_type.clone());

    match (first, matching.next()) {
        (Some(concrete), None) => Ok(concrete),
        (Some(_), Some(_)) => {
            Err(TraitSatisfactionError::AmbiguousAssociatedTypeBinding { trait_path: trait_path.clone(), name: name.clone() })
        }
        (None, _) => Err(TraitSatisfactionError::MissingAssociatedTypeBinding { trait_path: trait_path.clone(), name: name.clone() }),
    }
}

fn is_structural_ambiguity(error: &RowRequirementError) -> bool {
    matches!(error, RowRequirementError::DuplicateMethodRequirement { .. } | RowRequirementError::AmbiguousCandidateMethod { .. })
}

fn trait_type_to_path(ty: &HirType) -> Option<NamePath> {
    match ty {
        HirType::Named(name) => Some(NamePath::new(vec![name.clone()])),
        HirType::Apply(base, _) => match base.as_ref() {
            HirType::Named(name) => Some(NamePath::new(vec![name.clone()])),
            _ => None,
        },
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImplSpecificity {
    Equivalent,
    MoreSpecific,
    LessSpecific,
    Incomparable,
}

fn compare_impl_specificity(left: &HirImpl, right: &HirImpl) -> ImplSpecificity {
    let left_pairs = impl_where_constraint_pairs(left);
    let right_pairs = impl_where_constraint_pairs(right);

    match (left_pairs == right_pairs, left_pairs.is_superset(&right_pairs), right_pairs.is_superset(&left_pairs)) {
        (true, _, _) => ImplSpecificity::Equivalent,
        (false, true, false) => ImplSpecificity::MoreSpecific,
        (false, false, true) => ImplSpecificity::LessSpecific,
        _ => ImplSpecificity::Incomparable,
    }
}

fn impl_where_constraint_pairs(impl_block: &HirImpl) -> BTreeSet<(HirType, NamePath)> {
    impl_block
        .where_constraints
        .iter()
        .flat_map(|constraint| constraint.bounds.iter().cloned().map(|bound| (constraint.target.clone(), bound)))
        .collect()
}
