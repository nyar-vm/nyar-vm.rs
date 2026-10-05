//! Shared executable lowering utilities for backend drivers.

#[cfg(feature = "nyar-vm-lane")]
pub mod slots;

use nyar::NyarType;

/// Identity helper: executable payloads already carry platform [`NyarType`].
pub fn platform_type(ty: &NyarType) -> NyarType {
    ty.clone()
}
use nyar_types::{AggregateLayout, AggregateLayoutPlan, FieldId, FieldLayout, LayoutId, NominalInstanceId};

use crate::{
    FragmentSubmission,
    backend_plan_views::{ExecutableBlockRef, ExecutableFunction, ExecutableStorageKind, ExecutableTerminator},
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

    pub fn layout_by_nominal(&self, nominal: NominalInstanceId) -> Option<&AggregateLayout> {
        self.submission.aggregate_layout_by_nominal.get(&nominal).and_then(|id| self.layout_by_id(*id))
    }

    pub fn field_layout_by_id(&self, field: FieldId) -> Option<(&AggregateLayout, &FieldLayout, u32)> {
        let (layout_id, slot) = self.submission.aggregate_layout_by_field.get(&field)?;
        let layout = self.layout_by_id(*layout_id)?;
        let field_layout = layout.fields.get(*slot as usize)?;
        Some((layout, field_layout, *slot))
    }

    pub fn layout_by_type_name(&self, name: &str) -> Option<&AggregateLayout> {
        self.layouts.type_name_to_layout.get(name).and_then(|id| self.layout_by_id(*id))
    }


    pub fn field_layout(&self, layout_id: LayoutId, field: &str) -> Option<&FieldLayout> {
        self.layout_by_id(layout_id).and_then(|layout| layout.fields.iter().find(|item| item.name == field))
    }

    pub fn is_value_type_name(&self, name: &str) -> bool {
        self.layouts.value_type_names.contains(name)
    }

    pub fn layout_for_value_type(&self, ty: &NyarType) -> Option<&AggregateLayout> {
        nyar_types::layout_key_for_nyar_type(ty).and_then(|key| self.layout_by_type_name(&key))
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

