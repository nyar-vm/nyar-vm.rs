//! Unified registry for enums, unite, and flags nominal types.

use std::collections::BTreeMap;

use crate::types::{
    Identifier,
    hir::{HirEnum, HirFlagMember, HirFlags, HirModule, HirStruct, HirVariant},
};

use super::nominal::{LoweredUnite, NominalModuleError, UniteLayout, lower_unite, validate_unite_definition};

/// Unified nominal type registry built from a HIR module.
#[derive(Debug, Clone, Default)]
pub struct NominalTypeRegistry {
    enums: BTreeMap<Identifier, HirEnum>,
    unites: BTreeMap<Identifier, HirEnum>,
    flags: BTreeMap<Identifier, HirFlags>,
    lowered_unites: BTreeMap<Identifier, LoweredUnite>,
}

impl NominalTypeRegistry {
    /// Build a registry from module enums and flags.
    pub fn from_module(module: &HirModule) -> Self {
        let mut registry = Self::default();
        for enum_def in module.enums.iter().chain(module.imported_nominal_enums()) {
            if enum_def.is_unity() {
                registry.unites.insert(enum_def.name.clone(), enum_def.clone());
                if validate_unite_definition(enum_def).is_ok() {
                    let lowered = lower_unite(enum_def, UniteLayout::Tagged);
                    registry.lowered_unites.insert(enum_def.name.clone(), lowered);
                }
            }
            else {
                registry.enums.insert(enum_def.name.clone(), enum_def.clone());
            }
        }
        for flags_def in &module.flags {
            registry.flags.insert(flags_def.name.clone(), flags_def.clone());
        }
        registry
    }

    /// Variant names for a sum type (enums or unite).
    pub fn variant_names(&self, name: &Identifier) -> Vec<Identifier> {
        self.enum_def(name).map(|enum_def| enum_def.variants.iter().map(|variant| variant.name.clone()).collect()).unwrap_or_default()
    }

    pub fn is_sum_type(&self, name: &Identifier) -> bool {
        self.enums.contains_key(name) || self.unites.contains_key(name)
    }

    pub fn enum_def(&self, name: &Identifier) -> Option<&HirEnum> {
        self.enums.get(name).or_else(|| self.unites.get(name))
    }

    pub fn flags_def(&self, name: &Identifier) -> Option<&HirFlags> {
        self.flags.get(name)
    }

    pub fn flags_members(&self, name: &Identifier) -> &[HirFlagMember] {
        self.flags.get(name).map(|flags| flags.members.as_slice()).unwrap_or(&[])
    }

    pub fn lower_unite(&self, name: &Identifier) -> Result<&LoweredUnite, NominalModuleError> {
        self.lowered_unites.get(name).ok_or_else(|| super::nominal::NominalModuleError::UnknownType { name: name.clone() })
    }

    pub fn enum_from_covered_variants(&self, covered: &[Identifier]) -> Option<Identifier> {
        let mut candidates = Vec::new();
        for (enum_name, enum_def) in self.enums.iter().chain(self.unites.iter()) {
            if covered.iter().any(|variant| enum_def.variants.iter().any(|declared| &declared.name == variant)) {
                candidates.push(enum_name.clone());
            }
        }
        if candidates.len() == 1 { candidates.into_iter().next() } else { None }
    }

    /// Materialize lowered unite structs into the module struct table.
    pub fn materialize_unite_structs(&self, structs: &mut Vec<HirStruct>) {
        let existing: std::collections::BTreeSet<_> = structs.iter().map(|item| item.name.clone()).collect();
        for lowered in self.lowered_unites.values() {
            if !existing.contains(&lowered.base.name) {
                structs.push(lowered.base.clone());
            }
            for variant in &lowered.variants {
                if !existing.contains(&variant.name) {
                    structs.push(variant.clone());
                }
            }
        }
    }

    pub fn enums(&self) -> impl Iterator<Item = &HirEnum> {
        self.enums.values()
    }

    pub fn unites(&self) -> impl Iterator<Item = &HirEnum> {
        self.unites.values()
    }

    pub fn flags(&self) -> impl Iterator<Item = &HirFlags> {
        self.flags.values()
    }

    pub fn variant_def(&self, enum_name: &Identifier, variant_name: &Identifier) -> Option<&HirVariant> {
        self.enum_def(enum_name)?.variants.iter().find(|variant| &variant.name == variant_name)
    }
}

/// Parameter types for a variant constructor overload candidate.
pub fn variant_constructor_param_types(variant: &HirVariant) -> Vec<crate::types::hir::ValkyrieType> {
    variant.fields.iter().map(|field| field.ty.clone()).collect()
}
