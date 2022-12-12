//! Shared executable lowering utilities for backend drivers.

#[cfg(feature = "legacy-lanes")]
pub mod slots;

use nyar::NyarType;

/// Identity helper: executable payloads already carry platform [`NyarType`].
pub fn platform_type(ty: &NyarType) -> NyarType {
    ty.clone()
}
use nyar_types::{AggregateLayout, AggregateLayoutPlan, FieldLayout, LayoutId};

use crate::{
    FragmentSubmission,
    executable_provider::{ExecutableBlockRef, ExecutableFunction, ExecutableStorageKind, ExecutableTerminator},
};

/// Cross-backend lowering context attached to a fragment submission.
pub struct ExecutableLoweringContext<'a> {
    pub submission: &'a FragmentSubmission,
    pub layouts: &'a AggregateLayoutPlan,
}

impl<'a> ExecutableLoweringContext<'a> {
    pub fn new(submission: &'a FragmentSubmission) -> Self {
        Self { submission, layouts: &submission.aggregate_layouts }
    }

    pub fn require_layout_id(layout_id: Option<LayoutId>, context: &str) -> LayoutId {
        layout_id.unwrap_or_else(|| panic!("missing layout_id for {context}; executable instructions must carry layout metadata"))
    }

    pub fn layout_by_id(&self, id: LayoutId) -> Option<&AggregateLayout> {
        self.layouts.layouts.iter().find(|layout| layout.id == id)
    }

    pub fn layout_by_type_name(&self, name: &str) -> Option<&AggregateLayout> {
        self.layouts.type_name_to_layout.get(name).and_then(|id| self.layout_by_id(*id))
    }

    /// Resolve aggregate layout for a field access/store.
    ///
    /// Prefer `layout_id` when it actually contains `field`. Otherwise search all
    /// layouts whose simple/qualified name matches `type_name` and that declare
    /// `field` — required when multiple namespaces share a simple name
    /// (`TextSpan` offset/length vs start/stop) and the simple-name map kept the
    /// wrong first registration.
    pub fn layout_for_named_field(&self, layout_id: Option<LayoutId>, type_name: &str, field: &str) -> Option<&AggregateLayout> {
        if let Some(id) = layout_id {
            if let Some(layout) = self.layout_by_id(id) {
                if layout.fields.iter().any(|entry| entry.name == field) {
                    return Some(layout);
                }
            }
        }
        let simple = type_name.rsplit('.').next().unwrap_or(type_name);
        if let Some(layout) = self.layouts.layouts.iter().find(|layout| {
            let qualified = if layout.namespace.is_empty() { layout.name.clone() } else { format!("{}.{}", layout.namespace, layout.name) };
            (layout.name == simple || layout.name == type_name || qualified == type_name)
                && layout.fields.iter().any(|entry| entry.name == field)
        }) {
            return Some(layout);
        }
        self.layout_by_type_name(type_name)
    }

    pub fn field_layout(&self, layout_id: LayoutId, field: &str) -> Option<&FieldLayout> {
        self.layout_by_id(layout_id).and_then(|layout| layout.fields.iter().find(|item| item.name == field))
    }

    pub fn field_slot_index(&self, layout_id: Option<LayoutId>, type_name: &str, field: &str) -> u16 {
        let layout = self.layout_for_named_field(layout_id, type_name, field);
        let Some(layout) = layout
        else {
            return 0;
        };
        let mut offset: u16 = 0;
        for entry in &layout.fields {
            if entry.name == field {
                return offset;
            }
            offset = offset.saturating_add(self.jvm_flattened_slot_width(&entry.ty));
        }
        0
    }

    /// 递归扁平化值类型为 JVM local/参数的逻辑字段类型列表。
    ///
    /// 对于注册为值类型的 Named 类型，递归展开其所有字段；
    /// 对于其他类型（含 `i64`/`f64`），返回单元素向量。
    ///
    /// **宽类型槽位宽度**由 [`jvm_local_slots`] 单独计算（`i64`/`f64` → 2），
    /// 不得在此把 `Integer64`/`Float64` 复制成两个条目——否则
    /// [`effective_param_descriptors`] 会把 `render_i64_text(i64)` 编成
    /// `(JJ)Ljava/lang/String;`，调用方只压一个 long/int 时触发
    /// `VerifyError: Expecting to find long on stack`。
    pub fn flatten_value_type_slots(&self, ty: &NyarType) -> Vec<NyarType> {
        match ty {
            NyarType::Union(_) => vec![NyarType::Integer32 { signed: true }],
            NyarType::Named(name)
                if self.find_sum_type(name.as_str()).is_some()
                    || name.as_str().rsplit("::").next().unwrap_or(name.as_str()).rsplit('.').next().unwrap_or(name.as_str())
                        == "ExecutableInstructionKind" =>
            {
                // JVM enum/unite values are represented by the generated int
                // handle ABI. Do not flatten variant fields into mixed
                // reference/scalar locals; that makes one parameter slot change
                // category across arms and produces verifier-invalid
                // AStore/IStore sequences in compiler self-hosting helpers.
                vec![NyarType::Integer32 { signed: true }]
            }
            NyarType::Named(name) if self.is_value_type_name(name.as_str()) => match self.layout_by_type_name(name.as_str()) {
                Some(layout) => {
                    let mut result = Vec::new();
                    for field in &layout.fields {
                        result.extend(self.flatten_value_type_slots(&field.ty));
                    }
                    if result.is_empty() { vec![ty.clone()] } else { result }
                }
                None => vec![ty.clone()],
            },
            _ => vec![ty.clone()],
        }
    }

    /// JVM local 槽位宽度累加：叶子类型按 [`jvm_local_slots`]，嵌套值类型先 flatten。
    pub fn jvm_flattened_slot_width(&self, ty: &NyarType) -> u16 {
        self.flatten_value_type_slots(ty).iter().map(|leaf| jvm_local_slots(leaf)).sum::<u16>().max(1)
    }

    pub fn field_count(&self, layout_id: Option<LayoutId>, type_name: &str) -> u16 {
        let layout = layout_id.and_then(|id| self.layout_by_id(id)).or_else(|| self.layout_by_type_name(type_name));
        layout.map(|item| item.fields.len()).unwrap_or(1).max(1) as u16
    }

    pub fn field_type(&self, layout_id: Option<LayoutId>, type_name: &str, field: &str) -> Option<NyarType> {
        self.layout_for_named_field(layout_id, type_name, field)
            .and_then(|layout| layout.fields.iter().find(|entry| entry.name == field).map(|entry| entry.ty.clone()))
    }

    pub fn is_value_type_name(&self, name: &str) -> bool {
        self.layouts.value_type_names.contains(name)
    }

    pub fn find_type_name_by_field_with_hint(&self, field_name: &str, owner_hint: Option<&str>) -> Option<String> {
        let candidates: Vec<_> =
            self.layouts.layouts.iter().filter(|layout| layout.fields.iter().any(|field| field.name == field_name)).collect();
        match candidates.len() {
            0 => None,
            1 => Some(candidates[0].name.clone()),
            _ => owner_hint
                .and_then(|hint| candidates.iter().find(|layout| layout.name == hint).map(|layout| layout.name.clone()))
                .or_else(|| Self::disambiguate_field_owner_candidates(field_name, &candidates, &self.layouts.value_type_names)),
        }
    }

    /// 按字段名在所有布局中搜索拥有该字段的类型简单名。
    /// 当 `infer_aggregate_name` 无法从 SSA 值类型推断 owner 时，
    /// 可用本方法从字段名反查所属类型（如 `project_dir` -> `BuildContext`）。
    /// 多个布局都含同名字段时优先选择值类型布局，避免 CLR 回退到 `Object.<field>`。
    pub fn find_type_name_by_field(&self, field_name: &str) -> Option<String> {
        let candidates: Vec<_> =
            self.layouts.layouts.iter().filter(|layout| layout.fields.iter().any(|field| field.name == field_name)).collect();
        match candidates.len() {
            0 => None,
            1 => Some(candidates[0].name.clone()),
            _ => Self::disambiguate_field_owner_candidates(field_name, &candidates, &self.layouts.value_type_names),
        }
    }

    fn disambiguate_field_owner_candidates(
        field_name: &str,
        candidates: &[&AggregateLayout],
        value_type_names: &std::collections::BTreeSet<String>,
    ) -> Option<String> {
        // Unite/enum `payload`/`tag` are shared across many layouts. Prefer the tagged sum
        // shape (`tag` + `payload`) and reference storage so `AlgebraicTerm.payload` (utf8
        // valuetype field) cannot win over `Result`.
        if matches!(field_name, "payload" | "tag") {
            let tagged: Vec<_> = candidates
                .iter()
                .copied()
                .filter(|layout| {
                    layout.fields.iter().any(|field| field.name == "tag") && layout.fields.iter().any(|field| field.name == "payload")
                })
                .collect();
            if !tagged.is_empty() {
                if let Some(named) = tagged.iter().find(|layout| matches!(layout.name.as_str(), "Result" | "Option")) {
                    return Some(named.name.clone());
                }
                if let Some(reference) = tagged.iter().find(|layout| !value_type_names.contains(&layout.name)) {
                    return Some(reference.name.clone());
                }
                return Some(tagged[0].name.clone());
            }
        }
        candidates
            .iter()
            .find(|layout| value_type_names.contains(&layout.name))
            .or_else(|| candidates.first())
            .map(|layout| layout.name.clone())
    }

    pub fn clr_qualified_type_name(&self, simple_name: &str) -> String {
        self.layout_by_type_name(simple_name).map_or_else(
            || simple_name.to_string(),
            |layout| {
                if layout.namespace.is_empty() { layout.name.clone() } else { format!("{}.{}", layout.namespace, layout.name) }
            },
        )
    }

    pub fn clr_qualified_type_for_value_type(&self, ty: &NyarType) -> Option<String> {
        nyar_types::layout_key_for_nyar_type(ty).map(|key| self.clr_qualified_type_name(&key))
    }

    pub fn layout_for_value_type(&self, ty: &NyarType) -> Option<&AggregateLayout> {
        nyar_types::layout_key_for_nyar_type(ty).and_then(|key| self.layout_by_type_name(&key))
    }

    /// 按简单名查找任意 sum type（`enums` 或 `unite`）。
    ///
    /// JVM 后端对无 payload 的 `enums` 与带 payload 的 `unite` 均用 int 句柄表示变体
    /// （构造为 `iconst` tag）。描述符与 return/`checkcast`/`areturn` 必须走 `I`，
    /// 不能落成 `LName;`，否则会出现
    /// `VerifyError: Expecting to find object/array on stack`（`iconst` + `areturn`）。
    pub fn find_sum_type(&self, type_name: &str) -> Option<&nyar_types::layout::SumTypeLayout> {
        fn simple_name(name: &str) -> &str {
            name.rsplit("::").next().unwrap_or(name).rsplit('.').next().unwrap_or(name)
        }
        let want = simple_name(type_name);
        self.submission
            .sum_types
            .iter()
            .find(|sum| sum.name == type_name || simple_name(&sum.name) == want || simple_name(&sum.name) == type_name || sum.name == want)
    }

    /// 按简单名查找 unite 风格的 sum type。
    ///
    /// JVM 后端用 int 句柄表示 unite sum type（如 `VonParseResult`/`Option`/`Result`），
    /// 其 `payload` 字段访问不能用 `getfield`（栈上是 int 而非对象引用）。
    /// 此方法用于在 `FieldGet { field: "payload" }` 时判断 object 是否是 unite，
    /// 若是则改走 `tuple_get_0` runtime stub 提取 payload。
    pub fn find_unite_sum_type(&self, type_name: &str) -> Option<&nyar_types::layout::SumTypeLayout> {
        self.find_sum_type(type_name).filter(|sum| sum.is_unite)
    }

    pub fn witness_impl_symbol(&self, trait_name: &str, method_name: &str) -> Option<String> {
        self.submission
            .witness_tables
            .iter()
            .find(|table| table.trait_name == trait_name)
            .and_then(|table| table.methods.iter().find(|method| method.method_name == method_name))
            .map(|method| method.impl_symbol.clone())
    }

    pub fn storage_for_type(&self, ty: &NyarType) -> ExecutableStorageKind {
        match ty {
            NyarType::Bottom
            | NyarType::Unit
            | NyarType::Boolean
            | NyarType::Integer8 { .. }
            | NyarType::Integer16 { .. }
            | NyarType::Integer32 { .. }
            | NyarType::Integer64 { .. }
            | NyarType::Integer128 { .. }
            | NyarType::Float32
            | NyarType::Float64
            | NyarType::Character
            | NyarType::Utf8
            | NyarType::Utf16 => ExecutableStorageKind::Value,
            NyarType::Named(name) if self.is_value_type_name(name.as_str()) => ExecutableStorageKind::Value,
            NyarType::Tuple(_) | NyarType::FixedArray { .. } => ExecutableStorageKind::Value,
            _ => ExecutableStorageKind::Reference,
        }
    }

    /// Resolve a typed intrinsic opcode for a call callee.
    ///
    /// IntrinsicOpcode registries were removed from Semantic MIR (ADR 0010/0011).
    /// Always returns `None`; backends must lower ArrayGet/Set/Length and Invoke.
    pub fn resolve_intrinsic_opcode(&self, _callee: &crate::executable_provider::ExecutableOperand) -> Option<()> {
        None
    }
}

pub fn block_label(id: ExecutableBlockRef) -> String {
    format!("block_{}", id.0)
}

pub fn collect_reachable_blocks(function: &ExecutableFunction) -> Vec<ExecutableBlockRef> {
    let mut order = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut done = std::collections::BTreeSet::new();
    let mut stack = vec![function.entry];
    while let Some(block_id) = stack.pop() {
        if done.contains(&block_id) {
            continue;
        }
        if seen.contains(&block_id) {
            order.push(block_id);
            done.insert(block_id);
            continue;
        }
        seen.insert(block_id);
        stack.push(block_id);
        let Some(block) = function.blocks.get(block_id.0 as usize)
        else {
            continue;
        };
        match &block.terminator {
            ExecutableTerminator::Return { .. } => {}
            ExecutableTerminator::Jump { target, .. } => stack.push(*target),
            ExecutableTerminator::Branch { then_target, else_target, .. } => {
                stack.push(*else_target);
                stack.push(*then_target);
            }
            ExecutableTerminator::PerformEffect { resume_target, .. } => stack.push(*resume_target),
            ExecutableTerminator::YieldToRuntime { .. } => {}
            ExecutableTerminator::StateDispatch { cases, default_target, .. } => {
                stack.push(*default_target);
                for (_, target) in cases {
                    stack.push(*target);
                }
            }
            ExecutableTerminator::Unreachable => {}
        }
    }
    order.reverse();
    order
}

pub fn executable_has_state_machine(function: &ExecutableFunction) -> bool {
    function.blocks.iter().any(|block| {
        matches!(
            block.terminator,
            ExecutableTerminator::StateDispatch { .. }
                | ExecutableTerminator::YieldToRuntime { .. }
                | ExecutableTerminator::PerformEffect { .. }
        )
    })
}

pub fn jvm_local_slots(ty: &NyarType) -> u16 {
    match ty {
        NyarType::Float64 | NyarType::Integer64 { .. } => 2,
        _ => 1,
    }
}
