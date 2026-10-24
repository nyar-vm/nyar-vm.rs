//! Shared executable lowering utilities for backend drivers.

#[cfg(feature = "ovm-lane")]
pub mod slots;

use nyar::NyarType;

/// Identity helper: executable payloads already carry platform [`NyarType`].
pub fn platform_type(ty: &NyarType) -> NyarType {
    ty.clone()
}
use nyar_types::{AggregateLayout, AggregateLayoutPlan, FieldId, FieldLayout, LayoutId, NominalInstanceId, TypeId};

use crate::{
    FragmentSubmission,
    backend_plan_views::{ExecutableBlockRef, ExecutableFunction, ExecutableStorageKind, ExecutableTerminator},
    contracts::InstructionKind,
};

/// Cross-backend lowering context attached to a fragment submission.
pub struct ExecutableLoweringContext<'a> {
    pub submission: &'a FragmentSubmission,
    pub layouts: &'a AggregateLayoutPlan,
}

impl<'a> ExecutableLoweringContext<'a> {
    pub fn new(submission: &'a FragmentSubmission) -> Self {
        Self { submission, layouts: submission.backend_plan.aggregate_layouts() }
    }

    pub fn require_layout_id(layout_id: Option<LayoutId>, context: &str) -> LayoutId {
        layout_id.unwrap_or_else(|| panic!("missing layout_id for {context}; executable instructions must carry layout metadata"))
    }

    pub fn layout_by_id(&self, id: LayoutId) -> Option<&AggregateLayout> {
        self.layouts.layouts.iter().find(|layout| layout.id == id)
    }

    pub fn layout_by_nominal(&self, nominal: NominalInstanceId) -> Option<&AggregateLayout> {
        self.submission.backend_plan.aggregate_layout_by_nominal().get(&nominal).and_then(|id| self.layout_by_id(*id))
    }

    pub fn layout_by_type_id(&self, ty: TypeId) -> Option<&AggregateLayout> {
        self.submission.backend_plan.aggregate_layout_by_type().get(&ty).and_then(|id| self.layout_by_id(*id))
    }

    pub fn field_layout_by_id(&self, field: FieldId) -> Option<(&AggregateLayout, &FieldLayout, u32)> {
        let (layout_id, slot) = self.submission.backend_plan.aggregate_layout_by_field().get(&field)?;
        let layout = self.layout_by_id(*layout_id)?;
        let field_layout = layout.fields.get(*slot as usize)?;
        Some((layout, field_layout, *slot))
    }

    pub fn field_layout(&self, layout_id: LayoutId, field: &str) -> Option<&FieldLayout> {
        self.layout_by_id(layout_id).and_then(|layout| layout.fields.iter().find(|item| item.name == field))
    }

    /// 类型的存储类；每一条分支都来自类型自身的结构形状或已绑定身份。
    ///
    /// 这里既不做 layout 名称回查，也没有 `Reference` 默认值：`Named` 残差
    /// 不携带 declaration / `TypeId`，不存在可按身份解析的名义 layout。
    /// [`validate_storage_identities`] 已在 lowering 前拒绝该形状。
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
            NyarType::Tuple(_) | NyarType::FixedArray { .. } => ExecutableStorageKind::Value,
            NyarType::Array(_)
            | NyarType::TraitObject(_)
            | NyarType::Union(_)
            | NyarType::Nullable(_)
            | NyarType::Apply(..)
            | NyarType::Function(_) => ExecutableStorageKind::Reference,
            NyarType::Named(name) => panic!(
                "executable lowering contract violation: `Named({name})` 无 declaration / `TypeId` 身份，\
禁止按短名解析名义 layout 或回落 `Reference`（预编码校验见 `validate_storage_identities`）"
            ),
        }
    }
}

/// 可执行合同里无法按身份解析存储类的名义类型残差。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedStorageIdentity {
    /// 出现该残差的函数符号；layout 字段来源时为空。
    pub function: String,
    /// 出现位置（参数 / 返回值 / SSA 值 / 指令 / layout 字段）。
    pub site: String,
    /// 残差类型。
    pub ty: NyarType,
}

/// 预编码校验：可执行合同不得携带无身份的名义类型残差。
///
/// `NyarType::Named` 只有短名，没有 declaration / `TypeId`。一旦它进入可执行合同，
/// 存储类、参数 ABI 与 GC 字段字节都只能靠名称回查或默认值猜实现；
/// 该形状必须在生成任何字节之前结构化失败，而不是由 backend 认亲。
pub fn validate_storage_identities(submission: &FragmentSubmission) -> Result<(), UnresolvedStorageIdentity> {
    let plan = &submission.backend_plan;
    for instance in plan.instances() {
        let Some(view) = plan.get_function(&instance)
        else {
            continue;
        };
        let function = &view.function;
        let mut sites = vec![("return type".to_string(), &function.return_type)];
        sites.extend(function.param_types.iter().enumerate().map(|(index, ty)| (format!("parameter {index}"), ty)));
        sites.extend(function.value_types.iter().map(|(value, ty)| (format!("SSA value {}", value.0), ty)));
        for block in &function.blocks {
            for (index, instruction) in block.instructions.iter().enumerate() {
                let location = format!("block {} instruction {index}", block.id.0);
                match &instruction.kind {
                    InstructionKind::LoadConstant { ty: Some(ty), .. } => sites.push((format!("{location} LoadConstant"), ty)),
                    InstructionKind::StoreVar { ty: Some(ty), .. } => sites.push((format!("{location} StoreVar"), ty)),
                    InstructionKind::ArrayNew { array_type, .. } => sites.push((format!("{location} ArrayNew"), array_type)),
                    InstructionKind::ArrayFromElements { array_type, .. } => sites.push((format!("{location} ArrayFromElements"), array_type)),
                    _ => {}
                }
            }
        }
        for (site, ty) in sites {
            if let Some(ty) = named_residual(ty) {
                return Err(UnresolvedStorageIdentity { function: function.symbol.clone(), site, ty });
            }
        }
    }
    for layout in plan.aggregate_layouts().layouts.iter() {
        for field in &layout.fields {
            if let Some(ty) = named_residual(&field.ty) {
                return Err(UnresolvedStorageIdentity {
                    function: String::new(),
                    site: format!("aggregate layout `{}::{}` field `{}`", layout.namespace, layout.name, field.name),
                    ty,
                });
            }
        }
    }
    Ok(())
}

/// 递归查找无身份的名义短名残差（`NyarType::Named`）。
fn named_residual(ty: &NyarType) -> Option<NyarType> {
    match ty {
        NyarType::Named(_) => Some(ty.clone()),
        NyarType::Apply(base, arguments) => named_residual(base).or_else(|| arguments.iter().find_map(named_residual)),
        NyarType::Function(function) => named_residual(&function.return_type).or_else(|| function.params.iter().find_map(named_residual)),
        NyarType::Tuple(elements) | NyarType::Union(elements) => elements.iter().find_map(named_residual),
        NyarType::Array(element) | NyarType::FixedArray { element, .. } | NyarType::Nullable(element) => named_residual(element),
        NyarType::TraitObject(object) => object.type_arguments.iter().find_map(named_residual),
        _ => None,
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
