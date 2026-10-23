//! 测试与单组闭包用的占位内置名义类型语义。
//!
//! 正式编译由 `core/primitive` 等依赖组提供完整 `structure` 声明；此处只登记
//! carrier 语义事实，避免 bootstrap 阶段鸡生蛋。不得把短名当作唯一身份。

use nyar_types::{ItemId, NominalValueSemantics};

use super::model::MirStruct;

/// 占位内置标量：按值拷贝的 primitive / 文本编码。
const VALUE_TYPE_NAMES: &[&str] = &[
    "bool", "char", "i8", "i16", "i32", "i64", "i128", "u8", "u16", "u32", "u64", "u128", "usize", "isize", "f32", "f64", "utf8", "utf16",
    "unit", "void",
];

/// 由 MIR 聚合声明决定 carrier 语义；builtin 名义类型必须有明确值语义。
/// 若 `is_value_type = false`，说明聚合未声明值语义，直接 panic 阻止默认 Reference 回填。
pub(crate) fn semantics_for_declared_aggregate(aggregate: &MirStruct) -> NominalValueSemantics {
    if aggregate.is_value_type {
        NominalValueSemantics::Value
    } else {
        panic!(
            "builtin nominal `{}` (declaration={:?}) 缺少明确值语义；builtin 不得默认 Reference 回填。\
             请在 lowering 阶段显式冻结 `NominalValueSemantics` 或修复 is_value_type 赋值路径。",
            aggregate.name, aggregate.declaration,
        )
    }
}

/// 由冻结的声明 `ItemId` 在模块聚合表中查找语义。
pub(crate) fn semantics_for_item_declaration(module: &super::model::MirModule, item_id: ItemId) -> Option<NominalValueSemantics> {
    module.structs.iter().find(|aggregate| aggregate.declaration == Some(item_id)).map(semantics_for_declared_aggregate)
}

/// 测试占位：仅当短名属于预设内置集合时返回语义，否则 `None`。
pub(crate) fn placeholder_semantics_for_surface_name(name: &str) -> Option<NominalValueSemantics> {
    if VALUE_TYPE_NAMES.contains(&name) { Some(NominalValueSemantics::Value) } else { None }
}
