//! Anonymous row requirements over method surfaces.
//!
//! This module intentionally models only the narrow semantics currently settled
//! in the architecture notes:
//! - anonymous rows are shape-only method requirements
//! - row satisfaction is checked against methods/property accessors
//! - associated types are rejected because they belong to named traits

use std::collections::{BTreeMap, BTreeSet};

use crate::types::{
    Identifier,
    hir::{HirField, HirProperty, HirStruct, ValkyrieType},
};

/// 匿名 row 要求中的单个方法签名。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowMethodSignature {
    /// 方法名。
    pub name: Identifier,
    /// 形参类型列表（已剥离 `Self` 接收者）。
    pub params: Vec<ValkyrieType>,
    /// 返回类型。
    pub return_type: ValkyrieType,
}

impl RowMethodSignature {
    /// 以方法名、形参列表与返回类型构造签名。
    pub fn new(name: &str, params: Vec<ValkyrieType>, return_type: ValkyrieType) -> Self {
        Self { name: Identifier::new(name), params, return_type }
    }

    fn matches(&self, actual: &Self) -> bool {
        self.name == actual.name && self.params == actual.params && self.return_type == actual.return_type
    }
}

/// 匿名 row 要求中的单个条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowRequirementItem {
    /// 方法要求。
    Method(RowMethodSignature),
    /// 关联类型要求；匿名 row 不支持，仅用于给出明确报错。
    AssociatedType(Identifier),
}

/// 匿名 row 的方法集合要求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowRequirement {
    methods: Vec<RowMethodSignature>,
}

impl RowRequirement {
    /// 以方法签名集合构造 row 要求。
    pub fn from_methods(methods: Vec<RowMethodSignature>) -> Self {
        Self { methods }
    }

    /// 由条目列表构造 row 要求；含关联类型条目时报错。
    pub fn try_from_items(items: Vec<RowRequirementItem>) -> Result<Self, RowRequirementError> {
        let mut methods = Vec::new();

        for item in items {
            match item {
                RowRequirementItem::Method(method) => methods.push(method),
                RowRequirementItem::AssociatedType(name) => {
                    return Err(RowRequirementError::UnsupportedAssociatedType { name });
                }
            }
        }

        Ok(Self { methods })
    }

    /// 要求中声明的方法签名列表。
    pub fn methods(&self) -> &[RowMethodSignature] {
        &self.methods
    }

    /// 校验候选 struct 是否满足全部方法要求，返回全部失败原因。
    pub fn check_struct(&self, candidate: &HirStruct) -> Result<(), Vec<RowRequirementError>> {
        let available = RowCandidate::from_struct(candidate);
        let mut errors = Vec::new();
        let duplicate_requirements = duplicate_method_names(self.methods.iter().map(|method| method.name.clone()));

        for expected in &self.methods {
            if duplicate_requirements.contains(&expected.name) {
                continue;
            }

            match available.find_by_name(&expected.name).as_slice() {
                [] => errors.push(RowRequirementError::MissingMethod { name: expected.name.clone() }),
                [actual] if !expected.matches(actual) => errors.push(RowRequirementError::SignatureMismatch {
                    name: expected.name.clone(),
                    expected: expected.clone(),
                    actual: (*actual).clone(),
                }),
                [_] => {}
                _ => errors.push(RowRequirementError::AmbiguousCandidateMethod { name: expected.name.clone() }),
            }
        }

        errors.extend(duplicate_requirements.into_iter().map(|name| RowRequirementError::DuplicateMethodRequirement { name }));

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    /// 候选 struct 是否满足全部方法要求。
    pub fn is_satisfied_by(&self, candidate: &HirStruct) -> bool {
        self.check_struct(candidate).is_ok()
    }
}

/// 匿名 row 校验失败的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowRequirementError {
    /// 候选类型缺少要求的方法。
    MissingMethod {
        /// 缺失的方法名。
        name: Identifier,
    },
    /// 候选方法存在但签名不匹配。
    SignatureMismatch {
        /// 方法名。
        name: Identifier,
        /// 要求中的签名。
        expected: RowMethodSignature,
        /// 候选侧的实际签名。
        actual: RowMethodSignature,
    },
    /// 要求本身重复声明了同名方法。
    DuplicateMethodRequirement {
        /// 重复的方法名。
        name: Identifier,
    },
    /// 候选侧存在多个同名方法，无法唯一匹配。
    AmbiguousCandidateMethod {
        /// 歧义的方法名。
        name: Identifier,
    },
    /// 匿名 row 声明了关联类型（不支持）。
    UnsupportedAssociatedType {
        /// 被拒绝的关联类型名。
        name: Identifier,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RowCandidate {
    methods: Vec<RowMethodSignature>,
}

impl RowCandidate {
    fn from_struct(candidate: &HirStruct) -> Self {
        let mut methods: Vec<RowMethodSignature> = candidate
            .methods
            .iter()
            .filter(|method| method.visibility.is_public())
            .map(|method| RowMethodSignature {
                name: method.name.clone(),
                params: callable_surface_params(method.params.iter().map(|param| param.ty.clone()).collect()),
                return_type: method.return_type.clone(),
            })
            .collect();

        for field in &candidate.fields {
            if !field.visibility.is_public() {
                continue;
            }

            methods.push(synthetic_getter_signature(field));
            if field.is_mutable {
                methods.push(synthetic_setter_signature_from_field(field));
            }
        }

        for property in &candidate.properties {
            if !property.visibility.is_public() {
                continue;
            }

            if let Some(getter) = property.getter.as_ref() {
                methods.push(RowMethodSignature {
                    name: getter.name.clone(),
                    params: callable_surface_params(getter.params.iter().map(|param| param.ty.clone()).collect()),
                    return_type: getter.return_type.clone(),
                });
            }
            if let Some(setter) = property.setter.as_ref() {
                methods.push(RowMethodSignature {
                    name: setter.name.clone(),
                    params: callable_surface_params(setter.params.iter().map(|param| param.ty.clone()).collect()),
                    return_type: setter.return_type.clone(),
                });
            }
            else if !property.is_readonly {
                methods.push(synthetic_setter_signature(property));
            }
        }

        Self { methods }
    }

    fn find_by_name(&self, name: &Identifier) -> Vec<&RowMethodSignature> {
        self.methods.iter().filter(|method| &method.name == name).collect()
    }
}

fn synthetic_getter_signature(field: &HirField) -> RowMethodSignature {
    RowMethodSignature { name: Identifier::new(&format!("get_{}", field.name)), params: vec![], return_type: field.ty.clone() }
}

fn synthetic_setter_signature_from_field(field: &HirField) -> RowMethodSignature {
    RowMethodSignature {
        name: Identifier::new(&format!("set_{}", field.name)),
        params: vec![field.ty.clone()],
        return_type: ValkyrieType::Unit,
    }
}

fn synthetic_setter_signature(property: &HirProperty) -> RowMethodSignature {
    RowMethodSignature {
        name: Identifier::new(&format!("set_{}", property.name)),
        params: vec![property.ty.clone()],
        return_type: ValkyrieType::Unit,
    }
}

fn callable_surface_params(mut params: Vec<ValkyrieType>) -> Vec<ValkyrieType> {
    if matches!(params.first(), Some(ValkyrieType::SelfType)) {
        params.remove(0);
    }
    params
}

impl From<&str> for RowRequirementItem {
    fn from(value: &str) -> Self {
        RowRequirementItem::AssociatedType(Identifier::new(value))
    }
}

fn duplicate_method_names(names: impl Iterator<Item = Identifier>) -> BTreeSet<Identifier> {
    let mut counts = BTreeMap::new();

    for name in names {
        *counts.entry(name).or_insert(0usize) += 1;
    }

    counts.into_iter().filter_map(|(name, count)| (count > 1).then_some(name)).collect()
}
