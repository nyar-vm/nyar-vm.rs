//! 类型占位、未解析引用与类型身份表分析。

use super::{MirSumDeclaration, model::*};
use crate::types::hir::ValkyrieType;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn type_contains_placeholder(ty: &ValkyrieType, include_generic_binders: bool) -> bool {
    match ty {
        ValkyrieType::SelfType | ValkyrieType::Associated(_) | ValkyrieType::AutoType => true,
        ValkyrieType::Generic(_) if include_generic_binders => true,
        ValkyrieType::Nominal { arguments, .. } => arguments.iter().any(|arg| type_contains_placeholder(arg, include_generic_binders)),
        ValkyrieType::Apply(base, args) => {
            type_contains_placeholder(base, include_generic_binders)
                || args.iter().any(|arg| type_contains_placeholder(arg, include_generic_binders))
        }
        ValkyrieType::Tuple(items) | ValkyrieType::Union(items) | ValkyrieType::Intersection(items) => {
            items.iter().any(|item| type_contains_placeholder(item, include_generic_binders))
        }
        ValkyrieType::Function(function) => {
            function.params.iter().any(|param| type_contains_placeholder(param, include_generic_binders))
                || type_contains_placeholder(&function.return_type, include_generic_binders)
        }
        ValkyrieType::Array(element) | ValkyrieType::Nullable(element) => type_contains_placeholder(element, include_generic_binders),
        ValkyrieType::FixedArray { element, .. } => type_contains_placeholder(element, include_generic_binders),
        _ => false,
    }
}

/// Canonical 类型表只收录可无损 lower 的形状；泛型形参与 `Self`/关联类型保留在 MIR 局部语义中。
pub(crate) fn type_contains_unresolved_reference(ty: &ValkyrieType) -> bool {
    type_contains_placeholder(ty, true)
}

/// 函数签名允许携带泛型形参，但不允许 `Self`/关联类型占位。
pub(crate) fn type_contains_non_monomorphic_signature_reference(ty: &ValkyrieType) -> bool {
    type_contains_placeholder(ty, false)
}

/// 为模块函数、外部导入、结构体与 sum 声明建立类型事实表；外部测试入口。
#[doc(hidden)]
pub fn type_identity_table(
    functions: &[MirFunction],
    external_calls: &[MirExternalCallContract],
    structs: &[MirStruct],
    sums: &[MirSumDeclaration],
) -> BTreeMap<ValkyrieType, nyar_types::TypeId> {
    fn collect(set: &mut BTreeSet<ValkyrieType>, ty: &ValkyrieType) {
        match ty {
            ValkyrieType::Nominal { arguments, .. } => {
                for arg in arguments {
                    collect(set, arg);
                }
            }
            ValkyrieType::Apply(base, args) => {
                collect(set, base);
                for arg in args {
                    collect(set, arg);
                }
            }
            ValkyrieType::Tuple(items) | ValkyrieType::Union(items) | ValkyrieType::Intersection(items) => {
                for item in items {
                    collect(set, item);
                }
            }
            ValkyrieType::Function(function) => {
                for param in &function.params {
                    collect(set, param);
                }
                collect(set, &function.return_type);
            }
            ValkyrieType::Array(element) | ValkyrieType::Nullable(element) => collect(set, element),
            ValkyrieType::FixedArray { element, .. } => collect(set, element),
            ValkyrieType::Associated(associated) => {
                collect(set, &associated.base);
                for argument in &associated.type_arguments {
                    collect(set, argument);
                }
            }
            _ => {}
        }
        if !type_contains_unresolved_reference(ty) {
            set.insert(ty.clone());
        }
    }

    /// 聚合声明的规范类型键：已绑定 ItemId 的无参 Nominal。
    fn aggregate_type_key(structure: &MirStruct) -> Option<ValkyrieType> {
        structure.declaration.map(|declaration| ValkyrieType::Nominal { declaration, arguments: Vec::new() })
    }

    let mut types = BTreeSet::new();
    for function in functions {
        collect(&mut types, &function.return_type);
        for ty in &function.param_types {
            collect(&mut types, ty);
        }
        for ty in function.value_types.values() {
            collect(&mut types, ty);
        }
    }
    for contract in external_calls {
        for ty in &contract.parameter_types {
            collect(&mut types, ty);
        }
        collect(&mut types, &contract.return_type);
    }
    for structure in structs {
        if let Some(key) = aggregate_type_key(structure) {
            collect(&mut types, &key);
        }
        for field in &structure.fields {
            collect(&mut types, &field.ty);
        }
    }
    for sum in sums {
        if let Some(declaration) = sum.declaration {
            collect(&mut types, &ValkyrieType::Nominal { declaration, arguments: Vec::new() });
        }
        for variant in &sum.variants {
            for field in &variant.fields {
                collect(&mut types, &field.ty);
            }
            if let Some(result) = &variant.result_type {
                collect(&mut types, result);
            }
        }
    }

    types
        .into_iter()
        .enumerate()
        .map(|(index, ty)| (ty, nyar_types::TypeId::from_index(index as u32).expect("type identity overflow")))
        .collect()
}
