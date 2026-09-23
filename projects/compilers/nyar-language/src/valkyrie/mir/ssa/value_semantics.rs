//! Value vs reference storage classification for MIR aggregates.
//!
//! `structure`, tuple, and fixed array must stay **value** (`MirStorageKind::Value`):
//! inline layout, by-value move/copy, stack or linear-memory slots — never GC heap objects.
//! `class` and heap `[T]` remain **reference** (`MirStorageKind::Reference`).
//!
//! Layout payload types live in `nyar_types`; this module owns Valkyrie HIR → layout planning.

use std::collections::{BTreeMap, BTreeSet};

use nyar_types::StorageKind;

use crate::{
    frontend_contract::concretize_type_lossy,
    types::{
        Identifier,
        hir::{HirModule, HirStruct, ValkyrieType},
    },
};

pub use nyar_types::{
    AggregateLayout, AggregateLayoutPlan, FieldLayout, FlagsLayout, LayoutId, SumTypeLayout, SumVariantLayout, layout_id_for_nyar_type,
    layout_key_for_nyar_type,
};

/// Physical storage class for aggregate values at lowering time.
///
/// Type alias to the platform [`StorageKind`] so MIR and backends share one enum.
pub type MirStorageKind = StorageKind;

pub fn storage_kind_for_named_type(type_name: &str, struct_is_value_type: &BTreeMap<String, bool>) -> MirStorageKind {
    match struct_is_value_type.get(type_name) {
        Some(true) => MirStorageKind::Value,
        Some(false) | None => MirStorageKind::Reference,
    }
}

pub fn storage_kind_for_type(ty: &ValkyrieType, value_type_names: &BTreeSet<Identifier>) -> MirStorageKind {
    match ty {
        ValkyrieType::Tuple(_) | ValkyrieType::FixedArray { .. } => MirStorageKind::Value,
        ValkyrieType::Array(_) => MirStorageKind::Reference,
        ValkyrieType::Named(name) if value_type_names.contains(name) => MirStorageKind::Value,
        _ => MirStorageKind::Reference,
    }
}

pub fn value_type_names_from_module(structs: &[HirStruct]) -> BTreeSet<Identifier> {
    structs.iter().filter(|item| item.is_value_type).map(|item| item.name.clone()).collect()
}

pub fn layout_key_for_type(ty: &ValkyrieType) -> Option<String> {
    match ty {
        ValkyrieType::Named(name) => Some(name.to_string()),
        ValkyrieType::Tuple(types) => Some(format!("__tuple_{}", types.iter().map(type_layout_key_component).collect::<Vec<_>>().join("_"))),
        ValkyrieType::FixedArray { element, length } => Some(format!("__fixedarray_{length}_{}", type_layout_key_component(element))),
        _ => None,
    }
}

pub fn layout_id_for_type(ty: &ValkyrieType, plan: &AggregateLayoutPlan) -> Option<LayoutId> {
    // The HIR normalizer may represent a nominal aggregate as a one-element
    // tuple. Preserve the nominal layout identity when it is registered; using
    // the synthetic tuple layout would expose fields `0`, `1`, ... to semantic
    // FieldGet and lose the aggregate's declared field metadata.
    if let ValkyrieType::Tuple(elements) = ty {
        // Only the HIR normalizer's one-element wrapper can reuse a nominal
        // layout.  A real tuple keeps tuple identity even when all elements
        // happen to share the same nominal layout; otherwise `(T, T)` is
        // incorrectly resolved as `T` and tuple_get loses its field metadata.
        if elements.len() == 1 {
            let mut nominal_id = None;
            let mut consistent = true;
            for element in elements {
                let Some(id) = layout_id_for_type(element, plan)
                else {
                    consistent = false;
                    break;
                };
                if let Some(existing) = nominal_id {
                    if existing != id {
                        consistent = false;
                        break;
                    }
                }
                else {
                    nominal_id = Some(id);
                }
            }
            if consistent && nominal_id.is_some() {
                return nominal_id;
            }
        }
    }
    // Only layout-transparent wrappers may reuse the argument's aggregate layout.
    // Blindly unwrapping every one-arg Apply made `VonParseResult<Plan>` /
    // `Result<Plan>` resolve to `Plan`, so Fine/Fail `FieldGet payload` failed SMIR010
    // against the payload struct (no `payload` field). Match Future/Promise only —
    // same rule as validation / expr_helpers.
    if let ValkyrieType::Apply(base, arguments) = ty {
        if arguments.len() == 1
            && matches!(
                base.as_ref(),
                ValkyrieType::Named(name) if matches!(name.as_str(), "Future" | "Promise")
            )
        {
            if let Some(id) = layout_id_for_type(&arguments[0], plan) {
                return Some(id);
            }
        }
    }
    let key = layout_key_for_type(ty)?;
    plan.type_name_to_layout.get(&key).copied().or_else(|| {
        let suffix = format!(".{key}");
        let mut candidate = None;
        for (qualified, id) in &plan.type_name_to_layout {
            if !qualified.ends_with(&suffix) {
                continue;
            }
            if let Some(existing) = candidate {
                if existing != *id {
                    return None;
                }
            }
            candidate = Some(*id);
        }
        candidate
    })
}

pub fn compute_aggregate_layout_plan(module: &HirModule) -> AggregateLayoutPlan {
    let mut plan = AggregateLayoutPlan::default();
    register_builtin_value_types(&mut plan);
    let mut structs = Vec::new();
    collect_module_structs(module, &mut structs);
    for hir_struct in structs {
        let storage = if hir_struct.is_value_type { MirStorageKind::Value } else { MirStorageKind::Reference };
        let layout = layout_for_struct(hir_struct, &mut plan, storage);
        register_layout(&mut plan, layout);
    }
    // Semantic-group builds keep only consumer HIR; imported structures (e.g.
    // VonDiagnostic) live on imported_semantic_exports and must still provide
    // FieldGet layout ids — otherwise SMIR010 "no layout id" on error.message.
    // Skip empty / erased names. Always register distinct (namespace, name) shapes
    // so TextSpan{start,stop} is not erased by core.text TextSpan{offset,length}.
    for export in &module.imported_semantic_exports {
        for hir_struct in &export.structs {
            if hir_struct.fields.is_empty() {
                continue;
            }
            let simple = hir_struct.name.as_str();
            if matches!(simple, "any" | "null" | "object" | "Self" | "__auto" | "__opaque") {
                continue;
            }
            let storage = if hir_struct.is_value_type { MirStorageKind::Value } else { MirStorageKind::Reference };
            let layout = layout_for_struct(hir_struct, &mut plan, storage);
            register_layout(&mut plan, layout);
        }
    }
    for singleton in &module.singletons {
        let hir_struct = crate::valkyrie::mir::singleton::singleton_as_struct(singleton);
        let layout = layout_for_struct(&hir_struct, &mut plan, MirStorageKind::Reference);
        register_layout(&mut plan, layout);
    }
    plan
}

fn collect_module_structs<'a>(module: &'a HirModule, output: &mut Vec<&'a HirStruct>) {
    output.extend(module.structs.iter());
    for child in &module.submodules {
        collect_module_structs(child, output);
    }
}

/// 注册语言内置的值类型，使后端将其按值（inline）而非引用（heap）处理。
///
/// 这些类型在 HIR 中没有对应的 `struct` 声明，但运行时表现为标量包装：
/// - `ExitCode`：进程退出码，等价于 `i32`，layout 与 `i32` 一致。
fn register_builtin_value_types(plan: &mut AggregateLayoutPlan) {
    let exit_code_fields = vec![FieldSpec { name: "value".to_string(), ty: ValkyrieType::Integer32 { signed: true }, size: 4, align: 4 }];
    let exit_code_layout = build_aggregate_layout(next_layout_id(plan), "ExitCode", String::new(), MirStorageKind::Value, exit_code_fields);
    register_layout(plan, exit_code_layout);
}

pub fn ensure_layout_for_type(plan: &mut AggregateLayoutPlan, ty: &ValkyrieType) -> Option<LayoutId> {
    if let Some(id) = layout_id_for_type(ty, plan) {
        return Some(id);
    }
    let layout = match ty {
        ValkyrieType::Tuple(types) => layout_for_tuple(types, plan),
        ValkyrieType::FixedArray { element, length } => layout_for_fixed_array(element, *length, plan),
        _ => return None,
    };
    let id = layout.id;
    register_layout(plan, layout);
    Some(id)
}

/// Register a named aggregate layout discovered during StructNew when the
/// declaring HIR struct was not present in this module's layout plan (imports).
pub fn ensure_named_aggregate_layout(
    plan: &mut AggregateLayoutPlan,
    name: &str,
    storage: MirStorageKind,
    fields: &[(String, ValkyrieType)],
) -> LayoutId {
    // Prefer an existing layout with the same simple name AND compatible fields.
    // `TextSpan` exists as both core.text{offset,length} and von{start,stop}; returning
    // the first simple-name hit made StructNew disagree with layout metadata (SMIR010).
    if let Some(id) = plan.layouts.iter().find_map(|layout| {
        if layout.name != name {
            return None;
        }
        let compatible = fields.len() == layout.fields.len()
            && fields.iter().all(|(field_name, _)| layout.fields.iter().any(|field| field.name == *field_name));
        compatible.then_some(layout.id)
    }) {
        return id;
    }
    if fields.is_empty() {
        if let Some(id) = plan.type_name_to_layout.get(name).copied() {
            return id;
        }
    }
    let specs = fields
        .iter()
        .map(|(field_name, ty)| {
            let (size, align) = scalar_layout(ty, plan);
            FieldSpec { name: field_name.clone(), ty: ty.clone(), size, align }
        })
        .collect::<Vec<_>>();
    let layout = build_aggregate_layout(next_layout_id(plan), name, String::new(), storage, specs);
    let id = layout.id;
    register_layout(plan, layout);
    id
}

/// Unite/Result/Option runtime shape for MIR `FieldGet tag` / `FieldGet payload`.
///
/// Matches CLR/Wasm GC convention: reference aggregate with `tag: i32` + opaque payload.
/// Payload uses utf8 as a stand-in ABI carrier (reference-sized); typed Fine/Fail
/// extraction goes through `SumPayloadGet`, not this field's declared type.
pub fn ensure_unite_tagged_layout(plan: &mut AggregateLayoutPlan, sum_name: &str) -> LayoutId {
    if let Some(id) = plan.type_name_to_layout.get(sum_name).copied() {
        if plan.layouts.iter().find(|layout| layout.id == id).is_some_and(|layout| {
            layout.fields.iter().any(|field| field.name == "tag") && layout.fields.iter().any(|field| field.name == "payload")
        }) {
            return id;
        }
    }
    ensure_named_aggregate_layout(
        plan,
        sum_name,
        MirStorageKind::Reference,
        &[("tag".to_string(), ValkyrieType::Integer32 { signed: true }), ("payload".to_string(), ValkyrieType::Utf8)],
    )
}

/// Ensure every unite in `sum_types` has a tagged aggregate layout for FieldGet contracts.
pub fn ensure_unite_layouts_for_sums(plan: &mut AggregateLayoutPlan, sum_types: &[SumTypeLayout]) {
    for sum in sum_types {
        if sum.is_unite {
            ensure_unite_tagged_layout(plan, &sum.name);
        }
    }
}

pub fn layout_for_tuple(element_types: &[ValkyrieType], plan: &AggregateLayoutPlan) -> AggregateLayout {
    let name = layout_key_for_type(&ValkyrieType::Tuple(element_types.to_vec())).unwrap_or_else(|| "__tuple".to_string());
    build_aggregate_layout(next_layout_id(plan), &name, String::new(), MirStorageKind::Value, element_fields(element_types, plan))
}

pub fn layout_for_fixed_array(element_type: &ValkyrieType, length: usize, plan: &AggregateLayoutPlan) -> AggregateLayout {
    let name = layout_key_for_type(&ValkyrieType::FixedArray { element: Box::new(element_type.clone()), length })
        .unwrap_or_else(|| "__fixedarray".to_string());
    let fields = (0..length)
        .map(|index| {
            let (size, align) = scalar_layout(element_type, plan);
            FieldSpec { name: index.to_string(), ty: element_type.clone(), size, align }
        })
        .collect::<Vec<_>>();
    build_aggregate_layout(next_layout_id(plan), &name, String::new(), MirStorageKind::Value, fields)
}

fn layout_for_struct(hir_struct: &HirStruct, plan: &AggregateLayoutPlan, storage: MirStorageKind) -> AggregateLayout {
    let fields = hir_struct
        .fields
        .iter()
        .map(|field| {
            let (size, align) = scalar_layout(&field.ty, plan);
            FieldSpec { name: field.name.to_string(), ty: field.ty.clone(), size, align }
        })
        .collect();
    let namespace = hir_struct.namespace.iter().map(|id| id.as_str()).collect::<Vec<_>>().join(".");
    build_aggregate_layout(next_layout_id(plan), &hir_struct.name.to_string(), namespace, storage, fields)
}

fn layout_qualified_key(layout: &AggregateLayout) -> String {
    if layout.namespace.is_empty() { layout.name.clone() } else { format!("{}.{}", layout.namespace, layout.name) }
}

fn register_layout(plan: &mut AggregateLayoutPlan, layout: AggregateLayout) {
    // Same simple name may appear in multiple namespaces with different shapes
    // (e.g. `core.text.TextSpan`={offset,length} vs `std.data.text.von.TextSpan`={start,stop}).
    // Dropping the second entry made StructNew field_slot_index miss `start`/`stop` (both
    // wrote local+0) and JVM VerifyError: "Register N contains wrong type".
    // Keep every distinct (namespace, name) in `layouts`; map qualified key always; map
    // simple name only when free (first wins for unqualified lookup / CLR TypeDef alias).
    let qualified = layout_qualified_key(&layout);
    if plan.layouts.iter().any(|existing| existing.name == layout.name && existing.namespace == layout.namespace) {
        return;
    }
    if layout.storage == MirStorageKind::Value {
        plan.value_type_names.insert(layout.name.clone());
        plan.value_type_names.insert(qualified.clone());
    }
    plan.type_name_to_layout.insert(qualified, layout.id);
    plan.type_name_to_layout.entry(layout.name.clone()).or_insert(layout.id);
    plan.layouts.push(layout);
}

/// Merge dynamically discovered layouts from MIR lowering (or dependency MIR) into `dst`.
///
/// Layout ids are **module-local**. Crossing a module boundary (dependency link)
/// must reassign colliding ids and return `old_id → new_id` so instruction
/// `layout_id` fields can be rewritten. Same `(namespace, name)` reuses the
/// existing destination id (no duplicate layout row).
pub fn merge_aggregate_layout_plan(dst: &mut AggregateLayoutPlan, src: &AggregateLayoutPlan) -> BTreeMap<LayoutId, LayoutId> {
    let mut remap = BTreeMap::new();
    for layout in &src.layouts {
        let old_id = layout.id;
        if let Some(existing) = dst.layouts.iter().find(|candidate| candidate.name == layout.name && candidate.namespace == layout.namespace) {
            remap.insert(old_id, existing.id);
            continue;
        }
        let new_id = next_layout_id(dst);
        remap.insert(old_id, new_id);
        let mut layout = layout.clone();
        layout.id = new_id;
        register_layout(dst, layout);
    }
    remap
}

fn next_layout_id(plan: &AggregateLayoutPlan) -> LayoutId {
    plan.layouts.iter().map(|layout| layout.id).max().unwrap_or(0) + 1
}

struct FieldSpec {
    name: String,
    ty: ValkyrieType,
    size: u32,
    align: u32,
}

fn element_fields(element_types: &[ValkyrieType], plan: &AggregateLayoutPlan) -> Vec<FieldSpec> {
    element_types
        .iter()
        .enumerate()
        .map(|(index, ty)| {
            let (size, align) = scalar_layout(ty, plan);
            FieldSpec { name: index.to_string(), ty: ty.clone(), size, align }
        })
        .collect()
}

fn build_aggregate_layout(id: LayoutId, name: &str, namespace: String, storage: MirStorageKind, specs: Vec<FieldSpec>) -> AggregateLayout {
    let mut offset = 0u32;
    let mut max_align = 1u32;
    let mut fields = Vec::with_capacity(specs.len());
    for spec in specs {
        offset = align_offset(offset, spec.align);
        fields.push(FieldLayout { name: spec.name, ty: concretize_type_lossy(&spec.ty), offset, size: spec.size, align: spec.align });
        offset += spec.size;
        max_align = max_align.max(spec.align);
    }
    // Empty aggregates would otherwise get size=0 and fail SMIR010; language-level
    // empty structs still need a non-zero carrier for layout contracts.
    let size = align_offset(offset, max_align).max(if fields.is_empty() { max_align.max(1) } else { 0 });
    AggregateLayout { id, name: name.to_string(), namespace, storage, size, align: max_align.max(1), fields }
}

fn scalar_layout(ty: &ValkyrieType, plan: &AggregateLayoutPlan) -> (u32, u32) {
    match ty {
        ValkyrieType::Boolean | ValkyrieType::Character => (4, 4),
        ValkyrieType::Integer8 { .. } | ValkyrieType::Integer16 { .. } | ValkyrieType::Integer32 { .. } | ValkyrieType::Float32 => (4, 4),
        ValkyrieType::Integer64 { .. } | ValkyrieType::Float64 => (8, 8),
        ValkyrieType::Named(name) if plan.value_type_names.contains(name.as_str()) => plan
            .type_name_to_layout
            .get(name.as_str())
            .and_then(|id| plan.layouts.iter().find(|layout| layout.id == *id))
            .map(|layout| (layout.size, layout.align))
            .unwrap_or((8, 8)),
        ValkyrieType::Tuple(types) => inline_aggregate_size(&ValkyrieType::Tuple(types.clone()), plan),
        ValkyrieType::FixedArray { element, length } => {
            inline_aggregate_size(&ValkyrieType::FixedArray { element: element.clone(), length: *length }, plan)
        }
        _ => (8, 8),
    }
}

fn inline_aggregate_size(ty: &ValkyrieType, plan: &AggregateLayoutPlan) -> (u32, u32) {
    layout_id_for_type(ty, plan)
        .and_then(|id| plan.layouts.iter().find(|layout| layout.id == id))
        .map(|layout| (layout.size, layout.align))
        .unwrap_or_else(|| {
            let fields = match ty {
                ValkyrieType::Tuple(types) => element_fields(types, plan),
                ValkyrieType::FixedArray { element, length } => (0..*length)
                    .map(|index| {
                        let (size, align) = scalar_layout(element, plan);
                        FieldSpec { name: index.to_string(), ty: element.as_ref().clone(), size, align }
                    })
                    .collect(),
                _ => Vec::new(),
            };
            let layout = build_aggregate_layout(0, "__tmp", String::new(), MirStorageKind::Value, fields);
            (layout.size, layout.align)
        })
}

fn align_offset(offset: u32, align: u32) -> u32 {
    if align <= 1 {
        return offset;
    }
    ((offset + align - 1) / align) * align
}

fn type_layout_key_component(ty: &ValkyrieType) -> String {
    match ty {
        ValkyrieType::Named(name) => name.to_string(),
        ValkyrieType::Boolean => "bool".to_string(),
        ValkyrieType::Character => "char".to_string(),
        ValkyrieType::Integer8 { signed } => format!("i8_{signed}"),
        ValkyrieType::Integer16 { signed } => format!("i16_{signed}"),
        ValkyrieType::Integer32 { signed } => format!("i32_{signed}"),
        ValkyrieType::Integer64 { signed } => format!("i64_{signed}"),
        ValkyrieType::Integer128 { signed } => format!("i128_{signed}"),
        ValkyrieType::Float32 => "f32".to_string(),
        ValkyrieType::Float64 => "f64".to_string(),
        ValkyrieType::Utf8 => "utf8".to_string(),
        ValkyrieType::Utf16 => "utf16".to_string(),
        ValkyrieType::Tuple(types) => format!("tuple_{}", types.iter().map(type_layout_key_component).collect::<Vec<_>>().join("_")),
        ValkyrieType::FixedArray { element, length } => format!("arr{length}_{}", type_layout_key_component(element)),
        ValkyrieType::Array(inner) => format!("heaparr_{}", type_layout_key_component(inner)),
        _ => "ref".to_string(),
    }
}
