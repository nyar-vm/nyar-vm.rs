//! Unified parameter matching entry points for the current `HIR` layer.
//!
//! This module centralizes the settled matching categories used by overload
//! resolution and future call checking:
//! - nominal parameter matching
//! - named trait satisfaction
//! - anonymous row requirement satisfaction

#![allow(missing_docs)]

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    hir::{
        nominal::{NominalModuleError, NominalModuleView},
        row::{RowMethodSignature, RowRequirement, RowRequirementError},
        trait_system::{NamedTraitWitness, TraitModuleError, TraitModuleView},
    },
    types::{
        Identifier,
        hir::{HirModule, HirStruct, RowType, TraitObject, ValkyrieType},
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRelationDiagnosticSeed {
    IncompatibleTypes { actual: ValkyrieType, expected: ValkyrieType },
    UnsupportedActualType { actual: ValkyrieType },
    UnsupportedExpectedType { expected: ValkyrieType },
    Nominal { error: NominalModuleError },
    Trait { error: TraitModuleError },
    Row { errors: Vec<RowRequirementError> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParameterMatchResult {
    NominalExact,
    NominalSubtype { distance: usize },
    Trait { witness: NamedTraitWitness },
    Row,
    NoMatch { diagnostic_seed: TypeRelationDiagnosticSeed },
}

#[derive(Debug, Clone)]
pub struct TypeRelationContext {
    nominal: NominalModuleView,
    traits: TraitModuleView,
    structs: BTreeMap<Identifier, HirStruct>,
    /// 本模块中所有声明的泛型类型参数名称集合（如 `T`、`E`、`K`、`V`）。
    ///
    /// 在重载匹配 / 参数匹配时，`ValkyrieType::Named("T")` 无法与具体类型
    /// （如 `i64`）区分——两者都是 `Named`。此集合用于在 `match_parameter` 中
    /// 将期望类型为泛型参数的位置视为类型变量，接受任意实际类型，从而让
    /// `Some(42)` 能匹配 `Some { value: T }` 的构造函数签名。
    generic_params: BTreeSet<Identifier>,
}

impl TypeRelationContext {
    pub fn from_module(module: &HirModule) -> Self {
        let mut generic_params = BTreeSet::new();
        for enum_def in module.enums.iter().chain(module.imported_nominal_enums()) {
            for generic in &enum_def.generics {
                generic_params.insert(generic.name.clone());
            }
        }
        for struct_def in &module.structs {
            for generic in &struct_def.generics {
                generic_params.insert(generic.name.clone());
            }
        }
        for function in &module.functions {
            for generic in &function.generics {
                generic_params.insert(generic.name.clone());
            }
        }
        Self {
            nominal: NominalModuleView::from_module(module),
            traits: TraitModuleView::from_module(module),
            structs: module.structs.iter().map(|item| (item.name.clone(), item.clone())).collect(),
            generic_params,
        }
    }

    pub fn match_parameter(&self, actual: &ValkyrieType, expected: &ValkyrieType) -> ParameterMatchResult {
        if trivially_compatible(actual, expected) {
            return ParameterMatchResult::NominalExact;
        }

        // 泛型类型参数（如 `T`）在 HIR 中表示为 `Named("T")`，与具体名义类型
        // 无法区分。当期望类型是已注册的泛型参数时，视其为类型变量，接受任意
        // 实际类型——这与 `unify_constructor_type_vars` 中将 `Named` 视为类型
        // 变量的既有行为一致。
        if let ValkyrieType::Named(expected_name) = expected {
            if self.generic_params.contains(expected_name) {
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

    pub fn match_named_trait_parameter(&self, actual_name: &Identifier, trait_name: &Identifier) -> Option<ParameterMatchResult> {
        match self.traits.satisfy_named_trait(actual_name, trait_name) {
            Ok(witness) => Some(ParameterMatchResult::Trait { witness }),
            Err(TraitModuleError::UnknownTrait { .. }) => None,
            Err(error) => Some(ParameterMatchResult::NoMatch { diagnostic_seed: TypeRelationDiagnosticSeed::Trait { error } }),
        }
    }

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
        // `Self` is only a placeholder inside an imply body; overload matching must
        // substitute it with the owner before calling this (see `substitute_self_type`).
        // Keep identity for unresolved Self-vs-Self only — never "Self matches anything".
        (ValkyrieType::r#SelfType, ValkyrieType::r#SelfType) => true,
        (ValkyrieType::Array(actual), ValkyrieType::Array(expected)) => trivially_compatible(actual, expected),
        // `[T]` sugar and bare `Array` owner (after Self→Named(Array)) are the same shape.
        (ValkyrieType::Array(_), ValkyrieType::Named(name)) | (ValkyrieType::Named(name), ValkyrieType::Array(_))
            if name.as_str() == "Array" =>
        {
            true
        }
        // `[T]` sugar and `Array<T>` / `Apply(Array, [T])` are the same array shape for dispatch.
        (ValkyrieType::Array(actual_elem), ValkyrieType::Apply(base, args))
            if args.len() == 1 && matches!(named_type_name(base), Some(name) if name.as_str() == "Array") =>
        {
            trivially_compatible(actual_elem, &args[0])
        }
        (ValkyrieType::Apply(base, args), ValkyrieType::Array(expected_elem))
            if args.len() == 1 && matches!(named_type_name(base), Some(name) if name.as_str() == "Array") =>
        {
            trivially_compatible(&args[0], expected_elem)
        }
        (ValkyrieType::Tuple(actual), ValkyrieType::Tuple(expected)) if actual.len() == expected.len() => {
            actual.iter().zip(expected).all(|(actual, expected)| trivially_compatible(actual, expected))
        }
        (ValkyrieType::Intersection(actual), ValkyrieType::Intersection(expected)) if actual.len() == expected.len() => {
            actual.iter().zip(expected).all(|(actual, expected)| trivially_compatible(actual, expected))
        }
        _ => {
            collection_facade_compatible(actual, expected)
                || primitive_types_compatible(actual, expected)
                || actual == expected
                || matches!(actual, ValkyrieType::AutoType)
                || matches!(expected, ValkyrieType::AutoType)
        }
    }
}

/// `List<T>` is the collection facade for `ArrayList` / `LinkedList`.
/// - Method dispatch: a `List`-typed receiver may bind concrete list methods.
/// - Assignment: a concrete list value may satisfy a `List` parameter.
/// Never equates `List` with `[T]` / `Array` — those stay ArrayPush-only.
fn collection_facade_compatible(actual: &ValkyrieType, expected: &ValkyrieType) -> bool {
    let Some(act_name) = named_type_name(actual)
    else {
        return false;
    };
    let Some(exp_name) = named_type_name(expected)
    else {
        return false;
    };
    let list_receiver_to_concrete = act_name.as_str() == "List" && matches!(exp_name.as_str(), "ArrayList" | "LinkedList");
    let concrete_value_to_list = exp_name.as_str() == "List" && matches!(act_name.as_str(), "ArrayList" | "LinkedList");
    if !(list_receiver_to_concrete || concrete_value_to_list) {
        return false;
    }
    match (actual, expected) {
        (ValkyrieType::Apply(_, actual_args), ValkyrieType::Apply(_, expected_args)) if actual_args.len() == expected_args.len() => {
            actual_args.iter().zip(expected_args.iter()).all(|(actual_arg, expected_arg)| {
                trivially_compatible(actual_arg, expected_arg)
                    || actual_arg == expected_arg
                    || matches!(actual_arg, ValkyrieType::Named(_))
                    || matches!(expected_arg, ValkyrieType::Named(_))
            })
        }
        (ValkyrieType::Apply(_, _), ValkyrieType::Named(_))
        | (ValkyrieType::Named(_), ValkyrieType::Apply(_, _))
        | (ValkyrieType::Named(_), ValkyrieType::Named(_)) => true,
        _ => false,
    }
}

/// Primitive spellings may be introduced by a source declaration (`f64`), a
/// qualified core export (`core::primitive::f64`), or the typed HIR scalar
/// variant (`Float64`).  They denote the same language type; this relation is
/// resolved here, before overload selection, so no backend or intrinsic name
/// needs to infer a type width.
fn primitive_types_compatible(actual: &ValkyrieType, expected: &ValkyrieType) -> bool {
    primitive_type_key(actual).is_some_and(|left| primitive_type_key(expected) == Some(left))
}

fn primitive_type_key(ty: &ValkyrieType) -> Option<&'static str> {
    match ty {
        ValkyrieType::Boolean => Some("bool"),
        ValkyrieType::Character => Some("char"),
        ValkyrieType::Integer8 { signed: true } => Some("i8"),
        ValkyrieType::Integer8 { signed: false } => Some("u8"),
        ValkyrieType::Integer16 { signed: true } => Some("i16"),
        ValkyrieType::Integer16 { signed: false } => Some("u16"),
        ValkyrieType::Integer32 { signed: true } => Some("i32"),
        ValkyrieType::Integer32 { signed: false } => Some("u32"),
        ValkyrieType::Integer64 { signed: true } => Some("i64"),
        ValkyrieType::Integer64 { signed: false } => Some("u64"),
        ValkyrieType::Integer128 { signed: true } => Some("i128"),
        ValkyrieType::Integer128 { signed: false } => Some("u128"),
        ValkyrieType::Float32 => Some("f32"),
        ValkyrieType::Float64 => Some("f64"),
        ValkyrieType::Utf8 => Some("utf8"),
        ValkyrieType::Utf16 => Some("utf16"),
        ValkyrieType::Named(name) => match name.as_str() {
            "bool" | "core.primitive.bool" | "core::primitive::bool" => Some("bool"),
            "char" | "core.primitive.char" | "core::primitive::char" => Some("char"),
            "i8" | "core.primitive.i8" | "core::primitive::i8" => Some("i8"),
            "u8" | "core.primitive.u8" | "core::primitive::u8" => Some("u8"),
            "i16" | "core.primitive.i16" | "core::primitive::i16" => Some("i16"),
            "u16" | "core.primitive.u16" | "core::primitive::u16" => Some("u16"),
            "i32" | "core.primitive.i32" | "core::primitive::i32" => Some("i32"),
            "u32" | "core.primitive.u32" | "core::primitive::u32" => Some("u32"),
            "i64" | "core.primitive.i64" | "core::primitive::i64" => Some("i64"),
            "u64" | "core.primitive.u64" | "core::primitive::u64" => Some("u64"),
            "i128" | "core.primitive.i128" | "core::primitive::i128" => Some("i128"),
            "u128" | "core.primitive.u128" | "core::primitive::u128" => Some("u128"),
            "f32" | "core.primitive.f32" | "core::primitive::f32" => Some("f32"),
            "f64" | "core.primitive.f64" | "core::primitive::f64" => Some("f64"),
            "utf8" | "Utf8Text" | "core.primitive.utf8" | "core::primitive::utf8" => Some("utf8"),
            "utf16" | "Utf16Text" | "core.primitive.utf16" | "core::primitive::utf16" => Some("utf16"),
            _ => None,
        },
        _ => None,
    }
}
