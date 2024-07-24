//! HIR 声明身份到名义名的事实查询。
//!
//! 这些查询只把已绑定的 HIR declaration 与类型映射为名称，供后续阶段使用；
//! 它们不按短名替代 declaration identity。

use crate::types::{
    Identifier,
    hir::{HirModule, ValkyrieType},
};
use std::collections::BTreeMap;

/// 收集模块中所有 `imply T: Effectful { type Resume = X }` 实现，构造载体类型名到 `Resume` 类型的映射。
///
/// 仅匹配 `trait_path` 最后一段为 `Effectful` 的 impl 块。载体类型按其 nominal 名提取：
/// - `ValkyrieType::Named(name)` → 直接使用 `name`
/// - `ValkyrieType::Nominal{ItemId}` → 按 declaration 反查结构/sum 名
/// - `ValkyrieType::Apply(base, _)` → 取 `base` 的 nominal 名
///
/// 若同一载体存在多个 impl，后注册者覆盖前者。`Resume` 关联类型未声明的 impl 不产生条目。
pub(super) fn collect_effectful_resume_map(module: &HirModule) -> BTreeMap<nyar_types::ItemId, ValkyrieType> {
    let mut map: BTreeMap<nyar_types::ItemId, ValkyrieType> = BTreeMap::new();
    for impl_block in &module.impls {
        if !impl_block.trait_path.as_ref().is_some_and(|path| path.name().as_str() == "Effectful") {
            continue;
        }
        let Some(carrier_declaration) = impl_block.target.nominal_declaration()
        else {
            continue;
        };
        if let Some(resume_impl) = impl_block.associated_type_impls.iter().find(|item| item.name.as_str() == "Resume") {
            map.insert(carrier_declaration, resume_impl.concrete_type.clone());
        }
    }
    map
}

/// 提取 `ValkyrieType` 的表面 nominal 名（Named / 内置形状）。
///
/// `Nominal{ItemId}` 无表面名，须走 [`impl_owner_type_name`] 按 declaration 反查。
pub(super) fn nominal_type_name(ty: &ValkyrieType) -> Option<&str> {
    // 与 overload 侧 `impl_nominal_type_name` 对齐：`imply bool` / `imply f64` 等目标常为内置
    // `ValkyrieType` 而非 `Named`，否则 impl 方法不会进入 Semantic MIR。
    match ty {
        ValkyrieType::Array(_) => Some("Array"),
        ValkyrieType::Named(name) => Some(name.as_str()),
        ValkyrieType::Apply(base, _) => nominal_type_name(base),
        ValkyrieType::Float32 => Some("f32"),
        ValkyrieType::Float64 => Some("f64"),
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
        // Nominal 无表面名；禁止当无名丢弃 — 见 `impl_owner_type_name`。
        ValkyrieType::Nominal { .. } => None,
        _ => None,
    }
}

/// imply owner 表面名：Named/内置走 [`nominal_type_name`]；`Nominal{ItemId}` 按 declaration 反查。
///
/// `bind_declared_nominal_types` 之后 `imply usize` 目标已是 `Nominal{ItemId}`；若只认 Named，
/// imply 方法不会进入 Semantic MIR，调用点却已持有其 `ItemInstanceId`，链接报无合同。
pub(super) fn impl_owner_type_name(module: &HirModule, ty: &ValkyrieType) -> Option<String> {
    match ty {
        ValkyrieType::Nominal { declaration, .. } => find_hir_struct_name_by_declaration(module, *declaration)
            .or_else(|| find_hir_enum_name_by_declaration(module, *declaration))
            .map(|name| name.to_string()),
        ValkyrieType::Apply(base, _) => impl_owner_type_name(module, base),
        other => nominal_type_name(other).map(str::to_string),
    }
}

pub(super) fn find_hir_struct_name_by_declaration(module: &HirModule, declaration: nyar_types::ItemId) -> Option<Identifier> {
    for item in &module.structs {
        if item.constructor_declaration == Some(declaration) {
            return Some(item.name.clone());
        }
    }
    for export in &module.imported_semantic_exports {
        for item in &export.structs {
            if item.constructor_declaration == Some(declaration) {
                return Some(item.name.clone());
            }
        }
    }
    for submodule in &module.submodules {
        if let Some(name) = find_hir_struct_name_by_declaration(submodule, declaration) {
            return Some(name);
        }
    }
    None
}

pub(super) fn find_hir_enum_name_by_declaration(module: &HirModule, declaration: nyar_types::ItemId) -> Option<Identifier> {
    for item in &module.enums {
        if item.declaration == Some(declaration) {
            return Some(item.name.clone());
        }
    }
    for export in &module.imported_semantic_exports {
        for item in &export.enums {
            if item.declaration == Some(declaration) {
                return Some(item.name.clone());
            }
        }
    }
    for submodule in &module.submodules {
        if let Some(name) = find_hir_enum_name_by_declaration(submodule, declaration) {
            return Some(name);
        }
    }
    None
}
