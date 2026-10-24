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

    /// 判断名称是否为 sum 类型（enum 或 unite）。
    pub fn is_sum_type(&self, name: &Identifier) -> bool {
        self.enums.contains_key(name) || self.unites.contains_key(name)
    }

    /// 查找 enum 或 unite 定义。
    pub fn enum_def(&self, name: &Identifier) -> Option<&HirEnum> {
        self.enums.get(name).or_else(|| self.unites.get(name))
    }

    /// 查找 flags 类型定义。
    pub fn flags_def(&self, name: &Identifier) -> Option<&HirFlags> {
        self.flags.get(name)
    }

    /// 返回 flags 类型的成员列表。
    pub fn flags_members(&self, name: &Identifier) -> &[HirFlagMember] {
        self.flags.get(name).map(|flags| flags.members.as_slice()).unwrap_or(&[])
    }

    /// 查找已降低的 unite 布局。
    pub fn lower_unite(&self, name: &Identifier) -> Result<&LoweredUnite, NominalModuleError> {
        self.lowered_unites.get(name).ok_or_else(|| super::nominal::NominalModuleError::UnknownType { name: name.clone() })
    }

    /// 根据已覆盖变体名推断唯一所属 enum。
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

    /// 遍历全部 enum 定义。
    pub fn enums(&self) -> impl Iterator<Item = &HirEnum> {
        self.enums.values()
    }

    /// 遍历全部 unite 定义。
    pub fn unites(&self) -> impl Iterator<Item = &HirEnum> {
        self.unites.values()
    }

    /// 遍历全部 flags 定义。
    pub fn flags(&self) -> impl Iterator<Item = &HirFlags> {
        self.flags.values()
    }

    /// 查找 enum 内指定变体定义。
    pub fn variant_def(&self, enum_name: &Identifier, variant_name: &Identifier) -> Option<&HirVariant> {
        self.enum_def(enum_name)?.variants.iter().find(|variant| &variant.name == variant_name)
    }
}

/// Parameter types for a variant constructor overload candidate.
pub fn variant_constructor_param_types(variant: &HirVariant) -> Vec<crate::types::hir::ValkyrieType> {
    variant.fields.iter().map(|field| field.ty.clone()).collect()
}
