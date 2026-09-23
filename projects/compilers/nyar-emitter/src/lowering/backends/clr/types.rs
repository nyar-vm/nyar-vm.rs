use nyar::NyarType;
use nyar_types::{AggregateLayoutPlan, StorageKind, layout_key_for_nyar_type};

use crate::nyar_backend_clr::{
    MsilInstruction, MsilInstructionOperand, MsilMethodBody, MsilMethodRef, MsilMethodSignature, MsilOpcode, MsilType, MsilTypeDef,
};

use std_data::text::msil::MsilField;

fn object_ctor_ref() -> MsilMethodRef {
    MsilMethodRef {
        owner: Some("[mscorlib]System.Object".to_string()),
        name: ".ctor".to_string(),
        signature: MsilMethodSignature::new(MsilType::Void, Vec::new()),
    }
}

fn default_reference_ctor(type_name: &str) -> MsilMethodBody {
    MsilMethodBody {
        method: MsilMethodRef {
            owner: Some(type_name.to_string()),
            name: ".ctor".to_string(),
            signature: MsilMethodSignature::new(MsilType::Void, Vec::new()),
        },
        locals: Vec::new(),
        instructions: vec![
            MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
            MsilInstruction { label: None, opcode: MsilOpcode::Call, operand: Some(MsilInstructionOperand::Method(object_ctor_ref())) },
            MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
        ],
        max_stack: 8,
        is_entry_point: false,
        is_async: false,
    }
}

/// 从 `AggregateLayoutPlan` 生成 CLR `TypeDef` 表项。
///
/// `StorageKind::Value` 映射为 `is_value_type = true`（`Extends System.ValueType`）；
/// 引用类映射为 `is_value_type = false`（`Extends System.Object`）。
pub(crate) fn build_clr_type_defs(plan: &AggregateLayoutPlan) -> Vec<MsilTypeDef> {
    let mut seen = std::collections::BTreeSet::new();
    plan.layouts
        .iter()
        // Empty valuetypes need a ClassLayout size; PEVerify rejects them otherwise.
        // Primitive wrappers (`structure usize { }`, …) are mapped by `nyar_type_to_msil` and
        // must not appear as field-less ValueType TypeDefs.
        .filter(|layout| !(layout.storage == StorageKind::Value && layout.fields.is_empty()))
        .filter(|layout| seen.insert(layout.name.clone()))
        .map(|layout| {
            let is_value_type = layout.storage == StorageKind::Value;
            debug_assert_eq!(is_value_type, matches!(layout.storage, StorageKind::Value));
            let qualified_name =
                if layout.namespace.is_empty() { layout.name.clone() } else { format!("{}.{}", layout.namespace, layout.name) };
            MsilTypeDef {
                full_name: layout.name.clone(),
                namespace: layout.namespace.clone(),
                fields: layout
                    .fields
                    .iter()
                    .map(|field| MsilField { name: field.name.clone(), ty: nyar_type_to_msil(&field.ty, plan), is_static: false })
                    .collect(),
                methods: if is_value_type { Vec::new() } else { vec![default_reference_ctor(&qualified_name)] },
                is_value_type,
            }
        })
        .collect()
}

/// 将 `core::primitive::*`（或点号形式 `core.primitive.*`）原始类型名映射为对应的 CLR 原始 MSIL 类型。
///
/// Valkyrie 源码中以 `[primitive("core::primitive::usize")] structure usize { }` 形式定义的原始类型，
/// 在 HIR/MIR 中以 `NyarType::Named("usize")`（简单名）或 `NyarType::Named("core.primitive.usize")`（全名）形式出现。
/// 若不在此处映射，它们会被降级为空 `valuetype core.primitive.usize` 结构体，
/// 导致 IL 栈类型不匹配的 `InvalidProgramException`。
///
/// 同时接受三种形式：简单名（`usize`）、点号形式（`core.primitive.usize`）、双冒号形式（`core::primitive::usize`）。
///
/// 返回 `None` 表示该名称不是已知的原始类型名，调用方应继续走 `Named`/`value_type_names` 等后续分支。
fn map_primitive_name_to_msil(name: &str) -> Option<MsilType> {
    let normalized = name.replace("::", ".");
    let stripped = normalized.strip_prefix("core.primitive.").or_else(|| normalized.strip_prefix("primitive.")).unwrap_or(&normalized);
    let mapped = match stripped {
        "bool" => MsilType::Bool,
        "char" => MsilType::Char,
        "i8" => MsilType::Int8 { signed: true },
        "u8" => MsilType::Int8 { signed: false },
        "i16" => MsilType::Int16 { signed: true },
        "u16" => MsilType::Int16 { signed: false },
        "i32" => MsilType::Int32 { signed: true },
        "u32" => MsilType::Int32 { signed: false },
        "i64" => MsilType::Int64 { signed: true },
        "u64" => MsilType::Int64 { signed: false },
        "i128" => MsilType::Int64 { signed: true },
        "u128" => MsilType::Int64 { signed: false },
        "f32" => MsilType::Float32,
        "f64" => MsilType::Float64,
        "f128" => MsilType::Float64,
        "usize" => MsilType::Int32 { signed: false },
        "isize" => MsilType::Int32 { signed: true },
        "unit" => MsilType::Void,
        "void" => MsilType::Void,
        "null" => MsilType::Object,
        "any" => MsilType::Object,
        _ => return None,
    };
    Some(mapped)
}

pub(crate) fn nyar_type_to_msil(ty: &NyarType, plan: &AggregateLayoutPlan) -> MsilType {
    match ty {
        NyarType::Bottom | NyarType::Unit => MsilType::Void,
        NyarType::Boolean => MsilType::Bool,
        NyarType::Character => MsilType::Char,
        NyarType::Integer8 { signed } => MsilType::Int8 { signed: *signed },
        NyarType::Integer16 { signed } => MsilType::Int16 { signed: *signed },
        NyarType::Integer32 { signed } => MsilType::Int32 { signed: *signed },
        NyarType::Integer64 { signed } => MsilType::Int64 { signed: *signed },
        NyarType::Integer128 { .. } => MsilType::Int64 { signed: true },
        NyarType::Float32 => MsilType::Float32,
        NyarType::Float64 => MsilType::Float64,
        // These are distinct language encodings. A CLR preparation pass may
        // choose an explicit adapter, but the generic type mapper must not
        // collapse them into System.String.
        // CLR has one physical `System.String` representation for both text
        // encodings.  The semantic encoding remains the explicit NyarType
        // carried by MIR; lowering must never recover it from this mapping.
        NyarType::Utf8 | NyarType::Utf16 => MsilType::String,
        NyarType::Array(inner) => MsilType::sz_array(nyar_type_to_msil(inner, plan)),
        NyarType::Tuple(_) => layout_key_for_nyar_type(ty)
            .filter(|key| plan.type_name_to_layout.contains_key(key))
            .map(|key| MsilType::Named(key))
            .unwrap_or(MsilType::Object),
        NyarType::FixedArray { element, .. } => layout_key_for_nyar_type(ty)
            .filter(|key| plan.type_name_to_layout.contains_key(key))
            .map(|key| MsilType::Named(key))
            .unwrap_or(MsilType::sz_array(nyar_type_to_msil(element, plan))),
        NyarType::Named(name) => {
            let s = name.as_str();
            if let Some(primitive) = map_primitive_name_to_msil(s) {
                primitive
            }
            // Lossy concretize sentinels / unsubstituted Self — not PE TypeRef owners.
            else if matches!(s, "Self" | "__auto" | "__type_lambda" | "__associated" | "__row" | "__intersection" | "__opaque" | "__generic")
            {
                MsilType::Object
            }
            else if plan.value_type_names.contains(s) || plan.type_name_to_layout.contains_key(s) {
                MsilType::Named(name.to_string())
            }
            else {
                // No aggregate layout: typically an uninstantiated type parameter that
                // lossy concretize left as `Named("T")`, or a nominal erased at this
                // boundary. CLR ABI matches `Apply` / `TraitObject` → `Object`.
                MsilType::Object
            }
        }
        NyarType::TraitObject(_) => MsilType::Object,
        // Platform ABI: function values are `System.Delegate`; calls use `DynamicInvoke`.
        NyarType::Function(_) => MsilType::Named("[mscorlib]System.Delegate".to_string()),
        // CLR ABI erases type arguments: `VonParseResult<T>` shares one TypeDef with the
        // unapplied nominal (`unite VonParseResult<T>` → class `VonParseResult`). Mapping
        // `Apply` to `object` made Call returns unusable for `ldfld VonParseResult::*`.
        NyarType::Apply(base, _) => nyar_type_to_msil(base, plan),
        NyarType::Nullable(_) | NyarType::Union(_) => MsilType::Object,
    }
}
