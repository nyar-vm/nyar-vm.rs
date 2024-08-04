//! Value vs reference storage classification for MIR aggregates.
//!
//! `structure`, tuple, and fixed array must stay **value** (`MirStorageKind::Value`):
//! inline layout, by-value move/copy, stack or linear-memory slots — never GC heap objects.
//! `class` and heap `[T]` remain **reference** (`MirStorageKind::Reference`).
//!
//! Layout payload types live in `nyar_types`; this module owns Valkyrie HIR → layout planning.

use std::collections::BTreeSet;

use nyar_types::StorageKind;

use crate::{
    frontend_contract::concretize_type,
    types::hir::{HirModule, HirStruct, ValkyrieType},
};

pub use nyar_types::{AggregateLayout, AggregateLayoutPlan, FieldLayout, FlagsLayout, LayoutId, LayoutKey, SumTypeLayout, SumVariantLayout};

/// Physical storage class for aggregate values at lowering time.
///
/// Type alias to the platform [`StorageKind`] so MIR and backends share one enum.
pub type MirStorageKind = StorageKind;

/// 判定类型的物理存储类别：值语义或引用语义。
///
/// 仅已冻结的 `Nominal` declaration 可判为值类型；短名不认亲。
pub fn storage_kind_for_type(ty: &ValkyrieType, value_type_declarations: &BTreeSet<nyar_types::ItemId>) -> MirStorageKind {
    match ty {
        ValkyrieType::Tuple(_) | ValkyrieType::FixedArray { .. } => MirStorageKind::Value,
        ValkyrieType::Array(_) => MirStorageKind::Reference,
        // 只认已冻结 Nominal declaration；禁止 Named 短名认亲 value 存储。
        ValkyrieType::Nominal { declaration, .. } if value_type_declarations.contains(declaration) => MirStorageKind::Value,
        _ => MirStorageKind::Reference,
    }
}

/// 为合成 tuple / fixed array 类型生成结构化布局键；名义类型返回 `None`。
///
/// 键只由平台类型身份构成；元素类型无法严格 concretize 时不产生键，
/// 由后续布局构造失败关闭，禁止回落到名称。
fn synthetic_layout_key(ty: &ValkyrieType) -> Option<LayoutKey> {
    match ty {
        ValkyrieType::Tuple(types) => {
            let elements = types.iter().map(concretize_type).collect::<Result<Vec<_>, _>>().ok()?;
            Some(LayoutKey::Tuple(elements))
        }
        ValkyrieType::FixedArray { element, length } => {
            Some(LayoutKey::FixedArray { element: Box::new(concretize_type(element).ok()?), length: *length })
        }
        _ => None,
    }
}

/// 查询类型在计划中登记的布局 id：优先名义 declaration，其次合成键。
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
    // 生产查询只认 declaration → layout；名义缺布局不得回落 Named/限定名字符串。
    if let Some(declaration) = ty.nominal_declaration() {
        return plan.declaration_to_layout.get(&declaration).copied();
    }
    // 合成 tuple / fixedarray 结构键；禁止 Future/Promise 等短名透明解包。
    let key = synthetic_layout_key(ty)?;
    plan.synthetic_layouts.get(&key).copied()
}

/// 从 HIR 收集聚合布局计划：结构体、导入结构体与 singleton 载体。
pub fn compute_aggregate_layout_plan(module: &HirModule) -> AggregateLayoutPlan {
    let mut plan = AggregateLayoutPlan::default();
    let mut structs = Vec::new();
    collect_module_structs(module, &mut structs);
    for hir_struct in structs {
        // 开放泛型声明不得生成平台布局（字段仍是 binder）；与 SumTypeLayout 同口径。
        if !hir_struct.generics.is_empty() {
            continue;
        }
        let storage = if hir_struct.is_value_type { MirStorageKind::Value } else { MirStorageKind::Reference };
        let layout = layout_for_struct(hir_struct, &mut plan, storage);
        register_declared_layout(&mut plan, hir_struct, layout);
    }
    // Semantic-group builds keep only consumer HIR; imported structures (e.g.
    // VonDiagnostic) live on imported_semantic_exports and must still provide
    // FieldGet layout ids — otherwise SMIR010 "no layout id" on error.message.
    // Skip empty / erased names. Always register distinct (namespace, name) shapes
    // so TextSpan{start,stop} is not erased by core.text TextSpan{offset,length}.
    for export in &module.imported_semantic_exports {
        for hir_struct in &export.structs {
            if hir_struct.fields.is_empty() || !hir_struct.generics.is_empty() {
                continue;
            }
            let simple = hir_struct.name.as_str();
            if matches!(simple, "any" | "null" | "object" | "Self" | "__auto" | "__opaque") {
                continue;
            }
            let storage = if hir_struct.is_value_type { MirStorageKind::Value } else { MirStorageKind::Reference };
            let layout = layout_for_struct(hir_struct, &mut plan, storage);
            register_declared_layout(&mut plan, hir_struct, layout);
        }
    }
    for singleton in &module.singletons {
        let hir_struct = crate::valkyrie::mir::singleton::singleton_as_struct(singleton);
        if !hir_struct.generics.is_empty() {
            continue;
        }
        let layout = layout_for_struct(&hir_struct, &mut plan, MirStorageKind::Reference);
        register_declared_layout(&mut plan, &hir_struct, layout);
    }
    plan
}

fn collect_module_structs<'a>(module: &'a HirModule, output: &mut Vec<&'a HirStruct>) {
    output.extend(module.structs.iter());
    for child in &module.submodules {
        collect_module_structs(child, output);
    }
}

/// 确保类型已在计划中有布局：缺失时为 tuple / fixed array 合成并登记新布局。
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

/// Unite/Result/Option 运行时 `tag`/`payload` 载体布局，按 sum declaration ItemId 登记。
///
/// Matches CLR/Wasm GC convention: reference aggregate with `tag: i32` + opaque payload.
/// Payload uses utf8 as a stand-in ABI carrier (reference-sized); typed Fine/Fail
/// extraction goes through `SumPayloadGet`, not this field's declared type.
/// `display_name` 仅诊断/调试；禁止用名称合并或反查布局。
pub fn ensure_unite_tagged_layout(plan: &mut AggregateLayoutPlan, declaration: nyar_types::ItemId, display_name: &str) -> LayoutId {
    if let Some(id) = plan.declaration_to_layout.get(&declaration).copied() {
        if plan.layouts.iter().find(|layout| layout.id == id).is_some_and(|layout| {
            layout.fields.iter().any(|field| field.name == "tag") && layout.fields.iter().any(|field| field.name == "payload")
        }) {
            return id;
        }
    }
    let layout = build_aggregate_layout(
        next_layout_id(plan),
        display_name,
        String::new(),
        MirStorageKind::Reference,
        vec![
            FieldSpec { name: "tag".to_string(), ty: ValkyrieType::Integer32 { signed: true }, size: 4, align: 4 },
            FieldSpec { name: "payload".to_string(), ty: ValkyrieType::Utf8, size: 8, align: 8 },
        ],
    );
    let id = layout.id;
    plan.declaration_to_layout.insert(declaration, id);
    plan.layouts.push(layout);
    id
}

/// Ensure every unite in `sum_types` has a tagged aggregate layout for FieldGet contracts.
pub fn ensure_unite_layouts_for_sums(plan: &mut AggregateLayoutPlan, sum_types: &[super::MirSumDeclaration]) {
    for sum in sum_types {
        if sum.is_unite {
            let Some(declaration) = sum.declaration
            else {
                // 缺 declaration 不得按 sum.name 伪登记布局。
                continue;
            };
            ensure_unite_tagged_layout(plan, declaration, &sum.name);
        }
    }
}

pub fn layout_for_tuple(element_types: &[ValkyrieType], plan: &AggregateLayoutPlan) -> AggregateLayout {
    let mut layout =
        build_aggregate_layout(next_layout_id(plan), "tuple", String::new(), MirStorageKind::Value, element_fields(element_types, plan));
    layout.key = synthetic_layout_key(&ValkyrieType::Tuple(element_types.to_vec()));
    layout
}

pub fn layout_for_fixed_array(element_type: &ValkyrieType, length: usize, plan: &AggregateLayoutPlan) -> AggregateLayout {
    let fields = (0..length)
        .map(|index| {
            let (size, align) = scalar_layout(element_type, plan);
            FieldSpec { name: index.to_string(), ty: element_type.clone(), size, align }
        })
        .collect::<Vec<_>>();
    let mut layout = build_aggregate_layout(next_layout_id(plan), "fixedarray", String::new(), MirStorageKind::Value, fields);
    layout.key = synthetic_layout_key(&ValkyrieType::FixedArray { element: Box::new(element_type.clone()), length });
    layout
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

fn register_declared_layout(plan: &mut AggregateLayoutPlan, declaration: &HirStruct, layout: AggregateLayout) {
    let Some(item) = declaration.constructor_declaration
    else {
        // 缺 declaration 不得按 struct 名伪登记布局旁路。
        return;
    };
    if plan.declaration_to_layout.contains_key(&item) {
        return;
    }
    plan.declaration_to_layout.insert(item, layout.id);
    if layout.storage == MirStorageKind::Value {
        plan.value_type_declarations.insert(item);
    }
    // 名义结构体不得写入合成布局表；后端须读 layout.storage / value_type_declarations。
    plan.layouts.push(layout);
}

/// 仅登记合成 tuple / fixedarray 布局；键为结构化 [`LayoutKey`]。
fn register_layout(plan: &mut AggregateLayoutPlan, layout: AggregateLayout) {
    let Some(key) = layout.key.clone()
    else {
        // 无结构键的布局不能进入合成表。
        return;
    };
    if plan.synthetic_layouts.contains_key(&key) {
        return;
    }
    plan.synthetic_layouts.insert(key, layout.id);
    plan.layouts.push(layout);
}

fn next_layout_id(plan: &AggregateLayoutPlan) -> LayoutId {
    plan.layouts.iter().map(|layout| layout.id).max().unwrap_or(0) + 1
}

/// 聚合布局字段的规格描述：未 concretize 前的字段名 / 类型 / 字节大小 / 字节对齐。
#[doc(hidden)]
pub struct FieldSpec {
    /// 字段名；tuple / fixed array 用序号字符串。
    pub name: String,
    /// 未 concretize 的字段类型。
    pub ty: ValkyrieType,
    /// 字段字节大小。
    pub size: u32,
    /// 字段字节对齐。
    pub align: u32,
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

/// 按字段规格计算偏移与对齐并构造聚合布局；字段类型必须严格 concretize。
#[doc(hidden)]
pub fn build_aggregate_layout(id: LayoutId, name: &str, namespace: String, storage: MirStorageKind, specs: Vec<FieldSpec>) -> AggregateLayout {
    let mut offset = 0u32;
    let mut max_align = 1u32;
    let mut fields = Vec::with_capacity(specs.len());
    for spec in specs {
        offset = align_offset(offset, spec.align);
        // 布局字段类型必须严格 concretize；禁止 lossy 把 Self/Generic/Nominal 擦成伪 Named。
        let ty = concretize_type(&spec.ty)
            .unwrap_or_else(|err| panic!("aggregate layout `{namespace}::{name}` field `{}` 无法严格 concretize: {err}", spec.name));
        fields.push(FieldLayout { name: spec.name, ty, offset, size: spec.size, align: spec.align });
        offset += spec.size;
        max_align = max_align.max(spec.align);
    }
    // Empty aggregates would otherwise get size=0 and fail SMIR010; language-level
    // empty structs still need a non-zero carrier for layout contracts.
    let size = align_offset(offset, max_align).max(if fields.is_empty() { max_align.max(1) } else { 0 });
    AggregateLayout { id, name: name.to_string(), namespace, key: None, storage, size, align: max_align.max(1), fields }
}

fn scalar_layout(ty: &ValkyrieType, plan: &AggregateLayoutPlan) -> (u32, u32) {
    match ty {
        ValkyrieType::Boolean | ValkyrieType::Character => (4, 4),
        ValkyrieType::Integer8 { .. } | ValkyrieType::Integer16 { .. } | ValkyrieType::Integer32 { .. } | ValkyrieType::Float32 => (4, 4),
        ValkyrieType::Integer64 { .. } | ValkyrieType::Float64 => (8, 8),
        ValkyrieType::Nominal { declaration, .. } => plan
            .declaration_to_layout
            .get(declaration)
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
