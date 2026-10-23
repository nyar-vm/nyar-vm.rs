//! Unified parameter matching entry points for the current `HIR` layer.
//!
//! This module centralizes the settled matching categories used by overload
//! resolution and future call checking:
//! - nominal parameter matching
//! - named trait satisfaction
//! - anonymous row requirement satisfaction

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    hir::{
        nominal::{NominalModuleError, NominalModuleView},
        row::{RowMethodSignature, RowRequirement, RowRequirementError},
        trait_system::{NamedTraitWitness, TraitModuleError, TraitModuleView},
    },
    types::{
        Identifier, NamePath,
        hir::{GenericBinderId, HirEnum, HirModule, HirStruct, HirWhereConstraint, RowType, TraitObject, ValkyrieType},
    },
};

/// 类型关系判定失败时携带的诊断种子。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRelationDiagnosticSeed {
    /// 两侧类型互不相容。
    IncompatibleTypes {
        /// 实参类型。
        actual: ValkyrieType,
        /// 期望类型。
        expected: ValkyrieType,
    },
    /// 实参类型形态不受支持。
    UnsupportedActualType {
        /// 不受支持的实参类型。
        actual: ValkyrieType,
    },
    /// 期望类型形态不受支持。
    UnsupportedExpectedType {
        /// 不受支持的期望类型。
        expected: ValkyrieType,
    },
    /// 名义类型判定失败。
    Nominal {
        /// 名义模块层错误。
        error: NominalModuleError,
    },
    /// 具名 trait 判定失败。
    Trait {
        /// trait 模块层错误。
        error: TraitModuleError,
    },
    /// 匿名 row 判定失败。
    Row {
        /// 全部 row 校验失败原因。
        errors: Vec<RowRequirementError>,
    },
}

/// 单个形参的匹配结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParameterMatchResult {
    /// 名义类型完全一致。
    NominalExact,
    /// 名义子类型，距离越小越接近。
    NominalSubtype {
        /// 继承距离。
        distance: usize,
    },
    /// 通过具名 trait 满足，携带 witness。
    Trait {
        /// 生成的 trait witness。
        witness: NamedTraitWitness,
    },
    /// 通过匿名 row 形状满足。
    Row,
    /// 不匹配，携带诊断种子。
    NoMatch {
        /// 失败诊断种子。
        diagnostic_seed: TypeRelationDiagnosticSeed,
    },
}

/// 类型关系判定上下文：模块的名义、trait 与可调用约束视图。
#[derive(Debug, Clone)]
pub struct TypeRelationContext {
    nominal: NominalModuleView,
    traits: TraitModuleView,
    structs: BTreeMap<Identifier, HirStruct>,
    structs_by_declaration: BTreeMap<nyar_types::ItemId, HirStruct>,
    enums: BTreeMap<Identifier, HirEnum>,
    enums_by_declaration: BTreeMap<nyar_types::ItemId, HirEnum>,
    callable_constraints: Vec<HirWhereConstraint>,
    /// 当前正在解析调用的模块路径；无限定自由函数按结构化路径匹配，禁止末段回退。
    enclosing_module: NamePath,
}

impl TypeRelationContext {
    /// 由模块（含子模块与导入语义导出）构建判定上下文。
    pub fn from_module(module: &HirModule) -> Self {
        let mut structs = BTreeMap::new();
        let mut structs_by_declaration = BTreeMap::new();
        let mut enums = BTreeMap::new();
        let mut enums_by_declaration = BTreeMap::new();
        collect_relation_structs(module, &mut structs, &mut structs_by_declaration);
        collect_relation_enums(module, &mut enums, &mut enums_by_declaration);
        Self {
            nominal: NominalModuleView::from_module(module),
            traits: TraitModuleView::from_module(module),
            structs,
            structs_by_declaration,
            enums,
            enums_by_declaration,
            callable_constraints: Vec::new(),
            enclosing_module: module.name.clone(),
        }
    }

    /// 限定当前模块作用域，供无限定自由函数 overload 解析使用。
    pub(crate) fn with_enclosing_module(&self, module: &NamePath) -> Self {
        let mut context = self.clone();
        context.enclosing_module = module.clone();
        context
    }

    pub(crate) fn enclosing_module(&self) -> &NamePath {
        &self.enclosing_module
    }

    /// 短名 → 已冻结的 `Nominal{ItemId}`（有 declaration 时）；供 Self/AutoType 回收，禁止只写回 Named。
    pub fn nominal_type_for_name(&self, name: &Identifier) -> Option<ValkyrieType> {
        if let Some(item) = self.structs.get(name) {
            return Some(ValkyrieType::Nominal { declaration: item.constructor_declaration?, arguments: Vec::new() });
        }
        let item = self.enums.get(name)?;
        Some(ValkyrieType::Nominal { declaration: item.declaration?, arguments: Vec::new() })
    }

    /// 字面量特化用：`Named("usize")` 与对应 `Nominal{ItemId}` 都算数值类型。
    pub fn is_numeric_type(&self, ty: &ValkyrieType) -> bool {
        match ty {
            ValkyrieType::Nominal { declaration, .. } => {
                self.structs_by_declaration.get(declaration).is_some_and(|item| is_numeric_type_name(item.name.as_str()))
            }
            other => is_numeric_type_name_of(other),
        }
    }

    /// 派生一份带可调用声明级 where 约束的上下文副本。
    pub fn with_callable_constraints(&self, constraints: &[HirWhereConstraint]) -> Self {
        let mut context = self.clone();
        context.callable_constraints = constraints.to_vec();
        context
    }

    /// 当前上下文携带的可调用 where 约束。
    pub fn callable_constraints(&self) -> &[HirWhereConstraint] {
        &self.callable_constraints
    }

    /// 统一形参匹配入口：按 nominal、trait、row 的既定优先级返回结果。
    pub fn match_parameter(&self, actual: &ValkyrieType, expected: &ValkyrieType) -> ParameterMatchResult {
        if trivially_compatible(actual, expected) {
            return ParameterMatchResult::NominalExact;
        }

        // `Nominal{ItemId}` 只认 declaration；同 declaration + 实参相容即为 exact。
        if let (
            ValkyrieType::Nominal { declaration: actual_decl, arguments: actual_args },
            ValkyrieType::Nominal { declaration: expected_decl, arguments: expected_args },
        ) = (actual, expected)
        {
            if actual_decl == expected_decl
                && actual_args.len() == expected_args.len()
                && actual_args.iter().zip(expected_args.iter()).all(|(left, right)| self.matches_non_error(left, right))
            {
                return ParameterMatchResult::NominalExact;
            }
        }

        if let (ValkyrieType::Generic(actual), ValkyrieType::Generic(expected)) = (actual, expected) {
            if actual.binder.is_some() && actual.binder == expected.binder {
                return ParameterMatchResult::NominalExact;
            }
        }

        // Nominal applications retain their type arguments at the semantic
        // boundary.  Match the constructor nominally and each argument using
        // the same parameter relation, so exported `Choice<T, E>` can accept
        // a consumer-side `Choice<i32, utf8>` without erasing either generic
        // argument or falling back to a backend representation.
        if let (ValkyrieType::Apply(actual_base, actual_args), ValkyrieType::Apply(expected_base, expected_args)) = (actual, expected) {
            if actual_args.len() == expected_args.len()
                && matches!(
                    self.match_parameter(actual_base, expected_base),
                    ParameterMatchResult::NominalExact | ParameterMatchResult::NominalSubtype { .. }
                )
                && actual_args.iter().zip(expected_args).all(|(actual_arg, expected_arg)| {
                    !matches!(self.match_parameter(actual_arg, expected_arg), ParameterMatchResult::NoMatch { .. })
                })
            {
                return ParameterMatchResult::NominalExact;
            }
        }

        match expected {
            ValkyrieType::Union(expected_items) => {
                if expected_items.iter().any(|item| self.matches_non_error(actual, item)) {
                    return ParameterMatchResult::NominalExact;
                }
                if let ValkyrieType::Union(actual_items) = actual {
                    let all_assignable = actual_items
                        .iter()
                        .all(|actual_item| expected_items.iter().any(|expected_item| self.matches_non_error(actual_item, expected_item)));
                    if all_assignable {
                        return ParameterMatchResult::NominalExact;
                    }
                }
            }
            ValkyrieType::Intersection(expected_items) => {
                let all_assignable = expected_items.iter().all(|item| self.matches_non_error(actual, item));
                if all_assignable {
                    return ParameterMatchResult::NominalExact;
                }
            }
            ValkyrieType::Named(expected_name) => {
                if let Some(actual_name) = named_type_name(actual) {
                    let nominal = self.match_nominal_parameter(&actual_name, expected_name);
                    match &nominal {
                        ParameterMatchResult::NominalExact | ParameterMatchResult::NominalSubtype { .. } => return nominal,
                        ParameterMatchResult::NoMatch {
                            diagnostic_seed: TypeRelationDiagnosticSeed::Nominal { error: NominalModuleError::UnknownType { .. } },
                        } => {
                            if let Some(result) = self.match_named_trait_parameter(&actual_name, expected_name) {
                                return result;
                            }
                        }
                        ParameterMatchResult::NoMatch { diagnostic_seed: TypeRelationDiagnosticSeed::Nominal { .. } } => return nominal,
                        ParameterMatchResult::NoMatch { .. } => {
                            if let Some(result) = self.match_named_trait_parameter(&actual_name, expected_name) {
                                return result;
                            }
                            return nominal;
                        }
                        ParameterMatchResult::Trait { .. } | ParameterMatchResult::Row => {
                            unreachable!("nominal matching should not yield trait/row")
                        }
                    }
                }
            }
            ValkyrieType::TraitObject(TraitObject { trait_path, .. }) => {
                if let Some(actual_name) = named_type_name(actual) {
                    return self.match_named_trait_parameter(&actual_name, trait_path).unwrap_or(ParameterMatchResult::NoMatch {
                        diagnostic_seed: TypeRelationDiagnosticSeed::IncompatibleTypes { actual: actual.clone(), expected: expected.clone() },
                    });
                }
            }
            ValkyrieType::Row(row) => {
                if let Some(actual_name) = named_type_name(actual) {
                    return self.match_row_requirement(&actual_name, &row_type_to_requirement(row));
                }
            }
            _ => {}
        }

        ParameterMatchResult::NoMatch {
            diagnostic_seed: TypeRelationDiagnosticSeed::IncompatibleTypes { actual: actual.clone(), expected: expected.clone() },
        }
    }

    /// 按名义继承关系匹配两个类型短名。
    pub fn match_nominal_parameter(&self, actual_name: &Identifier, expected_name: &Identifier) -> ParameterMatchResult {
        match self.nominal.nominal_match_distance(actual_name, expected_name) {
            Ok(Some(0)) => ParameterMatchResult::NominalExact,
            Ok(Some(distance)) => ParameterMatchResult::NominalSubtype { distance },
            Ok(None) => ParameterMatchResult::NoMatch {
                diagnostic_seed: TypeRelationDiagnosticSeed::IncompatibleTypes {
                    actual: ValkyrieType::Named(actual_name.clone()),
                    expected: ValkyrieType::Named(expected_name.clone()),
                },
            },
            Err(error) => ParameterMatchResult::NoMatch { diagnostic_seed: TypeRelationDiagnosticSeed::Nominal { error } },
        }
    }

    /// 按具名 trait 满足关系匹配；trait 未知时返回 `None`。
    pub fn match_named_trait_parameter(&self, actual_name: &Identifier, trait_name: &Identifier) -> Option<ParameterMatchResult> {
        match self.traits.satisfy_named_trait(actual_name, trait_name) {
            Ok(witness) => Some(ParameterMatchResult::Trait { witness }),
            Err(TraitModuleError::UnknownTrait { .. }) => None,
            Err(error) => Some(ParameterMatchResult::NoMatch { diagnostic_seed: TypeRelationDiagnosticSeed::Trait { error } }),
        }
    }

    /// 在泛型实参完成代入后验证 callable 的声明级 where 合同。
    pub fn satisfies_where_constraints(
        &self,
        constraints: &[HirWhereConstraint],
        substitutions: &BTreeMap<GenericBinderId, ValkyrieType>,
    ) -> bool {
        constraints.iter().all(|constraint| {
            let target = substitute_relation_type(&constraint.target, substitutions);
            constraint.bounds.iter().all(|bound| {
                if let Some(context) = self.callable_constraints.iter().find(|context| {
                    let context_target = substitute_relation_type(&context.target, substitutions);
                    context_target == target
                }) {
                    return bound_is_satisfied_by_context(bound, context, &self.callable_constraints, substitutions);
                }
                let Ok(witness) = self.traits.satisfy_explicit_type(&target, bound)
                else {
                    return false;
                };
                bound.associated_types.iter().all(|equation| {
                    let expected = substitute_relation_type(&equation.ty, substitutions);
                    witness.associated_types.get(&equation.name).is_some_and(|actual| actual == &expected)
                })
            })
        })
    }

    /// 校验具名类型是否满足匿名 row 的方法要求。
    pub fn match_row_requirement(&self, actual_name: &Identifier, requirement: &RowRequirement) -> ParameterMatchResult {
        let Some(candidate) = self.structs.get(actual_name)
        else {
            return ParameterMatchResult::NoMatch {
                diagnostic_seed: TypeRelationDiagnosticSeed::UnsupportedActualType { actual: ValkyrieType::Named(actual_name.clone()) },
            };
        };

        match requirement.check_struct(candidate) {
            Ok(()) => ParameterMatchResult::Row,
            Err(errors) => ParameterMatchResult::NoMatch { diagnostic_seed: TypeRelationDiagnosticSeed::Row { errors } },
        }
    }

    fn matches_non_error(&self, actual: &ValkyrieType, expected: &ValkyrieType) -> bool {
        !matches!(self.match_parameter(actual, expected), ParameterMatchResult::NoMatch { .. })
    }
}

fn collect_relation_enums(
    module: &HirModule,
    by_name: &mut BTreeMap<Identifier, HirEnum>,
    by_declaration: &mut BTreeMap<nyar_types::ItemId, HirEnum>,
) {
    for item in &module.enums {
        by_name.entry(item.name.clone()).or_insert_with(|| item.clone());
        if let Some(declaration) = item.declaration {
            by_declaration.entry(declaration).or_insert_with(|| item.clone());
        }
    }
    for export in &module.imported_semantic_exports {
        for item in &export.enums {
            by_name.entry(item.name.clone()).or_insert_with(|| item.clone());
            if let Some(declaration) = item.declaration {
                by_declaration.entry(declaration).or_insert_with(|| item.clone());
            }
        }
    }
    for submodule in &module.submodules {
        collect_relation_enums(submodule, by_name, by_declaration);
    }
}

fn collect_relation_structs(
    module: &HirModule,
    by_name: &mut BTreeMap<Identifier, HirStruct>,
    by_declaration: &mut BTreeMap<nyar_types::ItemId, HirStruct>,
) {
    for item in &module.structs {
        by_name.entry(item.name.clone()).or_insert_with(|| item.clone());
        if let Some(declaration) = item.constructor_declaration {
            by_declaration.entry(declaration).or_insert_with(|| item.clone());
        }
    }
    for export in &module.imported_semantic_exports {
        for item in &export.structs {
            by_name.entry(item.name.clone()).or_insert_with(|| item.clone());
            if let Some(declaration) = item.constructor_declaration {
                by_declaration.entry(declaration).or_insert_with(|| item.clone());
            }
        }
    }
    for submodule in &module.submodules {
        collect_relation_structs(submodule, by_name, by_declaration);
    }
}

/// 数值名集合：源码写出的数值名与对应声明共用同一份口径。
///
/// 这里只判定“名字属于数值 primitive 声明族”，不生产任何低层 carrier；carrier 由表示阶段拥有。
pub(crate) fn is_numeric_type_name(name: &str) -> bool {
    matches!(
        name,
        "byte"
            | "sbyte"
            | "usize"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "f32"
            | "f64"
            | "f128"
            | "core.primitive.usize"
            | "core.primitive.isize"
            | "core::primitive::usize"
            | "core::primitive::isize"
    )
}

/// 数值类型判定（字面量特化与值位置字面量绑定共用同一份口径）。
///
/// `Named("i32")` 这类源码写出的数值名与对应 `Nominal{ItemId}` 都算数值；判定只认数值名集合，
/// 不解析声明本身。
pub(crate) fn is_numeric_type_name_of(ty: &ValkyrieType) -> bool {
    match ty {
        ValkyrieType::Integer8 { .. }
        | ValkyrieType::Integer16 { .. }
        | ValkyrieType::Integer32 { .. }
        | ValkyrieType::Integer64 { .. }
        | ValkyrieType::Integer128 { .. }
        | ValkyrieType::Float32
        | ValkyrieType::Float64 => true,
        ValkyrieType::Named(name) => is_numeric_type_name(name.as_str()),
        _ => false,
    }
}

fn bound_is_satisfied_by_context(
    required: &crate::types::hir::HirTraitBound,
    context: &HirWhereConstraint,
    all_contexts: &[HirWhereConstraint],
    substitutions: &BTreeMap<GenericBinderId, ValkyrieType>,
) -> bool {
    context.bounds.iter().any(|provided| {
        provided.declaration.is_some()
            && provided.declaration == required.declaration
            && provided.type_arguments.len() == required.type_arguments.len()
            && provided.type_arguments.iter().zip(&required.type_arguments).all(|(provided, required)| {
                normalize_context_type(provided, all_contexts, substitutions) == normalize_context_type(required, all_contexts, substitutions)
            })
            && required.associated_types.iter().all(|required_equation| {
                let expected = normalize_context_type(&required_equation.ty, all_contexts, substitutions);
                provided.associated_types.iter().any(|provided_equation| {
                    provided_equation.name == required_equation.name
                        && normalize_context_type(&provided_equation.ty, all_contexts, substitutions) == expected
                })
            })
    })
}

fn normalize_context_type(
    ty: &ValkyrieType,
    contexts: &[HirWhereConstraint],
    substitutions: &BTreeMap<GenericBinderId, ValkyrieType>,
) -> ValkyrieType {
    let ty = substitute_relation_type(ty, substitutions);
    let ValkyrieType::Associated(associated) = &ty
    else {
        return ty;
    };
    let base = &associated.base;
    contexts
        .iter()
        .filter(|context| substitute_relation_type(&context.target, substitutions) == *base)
        .flat_map(|context| &context.bounds)
        .flat_map(|bound| &bound.associated_types)
        .find(|equation| equation.name == associated.name)
        .map(|equation| normalize_context_type(&equation.ty, contexts, substitutions))
        .unwrap_or(ty)
}

fn substitute_relation_type(ty: &ValkyrieType, substitutions: &BTreeMap<GenericBinderId, ValkyrieType>) -> ValkyrieType {
    super::overload::substitute_type_vars(ty, substitutions)
}

fn row_type_to_requirement(row: &RowType) -> RowRequirement {
    RowRequirement::from_methods(
        row.methods
            .iter()
            .map(|method| RowMethodSignature {
                name: method.name.clone(),
                params: method.params.clone(),
                return_type: method.return_type.clone(),
            })
            .collect(),
    )
}

fn named_type_name(ty: &ValkyrieType) -> Option<Identifier> {
    match ty {
        ValkyrieType::Named(name) => Some(name.clone()),

        ValkyrieType::Apply(base, _) => named_type_name(base),
        _ => None,
    }
}

fn trivially_compatible(actual: &ValkyrieType, expected: &ValkyrieType) -> bool {
    match (actual, expected) {
        (ValkyrieType::Generic(actual), ValkyrieType::Generic(expected)) if actual.binder.is_some() && actual.binder == expected.binder => true,
        // `Self` is only a placeholder inside an imply body; overload matching must
        // substitute it with the owner before calling this (see `substitute_self_type`).
        // Keep identity for unresolved Self-vs-Self only — never "Self matches anything".
        (ValkyrieType::r#SelfType, ValkyrieType::r#SelfType) => true,
        (ValkyrieType::Array(actual), ValkyrieType::Array(expected)) => trivially_compatible(actual, expected),
        (ValkyrieType::Tuple(actual), ValkyrieType::Tuple(expected)) if actual.len() == expected.len() => {
            actual.iter().zip(expected).all(|(actual, expected)| trivially_compatible(actual, expected))
        }
        (ValkyrieType::Intersection(actual), ValkyrieType::Intersection(expected)) if actual.len() == expected.len() => {
            actual.iter().zip(expected).all(|(actual, expected)| trivially_compatible(actual, expected))
        }
        _ => actual == expected || matches!(actual, ValkyrieType::AutoType) || matches!(expected, ValkyrieType::AutoType),
    }
}
