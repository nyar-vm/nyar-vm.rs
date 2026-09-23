//! Slot / local planning for executable SSA values.

use std::collections::BTreeMap;

use crate::{
    contracts::ValueOrigin,
    executable_provider::{
        ExecutableBlockRef, ExecutableFunction, ExecutableInstruction, ExecutableInstructionKind, ExecutableOperand, ExecutableStorageKind,
        ExecutableValueRef,
    },
    lowering::{
        clr_types::nyar_type_to_msil,
        shared::executable::{ExecutableLoweringContext, collect_reachable_blocks},
    },
    nyar_backend_clr::MsilType,
};
use nyar::NyarType;

pub struct ExecutableSlotPlan {
    pub local_types: Vec<MsilType>,
    pub value_locals: BTreeMap<ExecutableValueRef, u16>,
    pub var_locals: BTreeMap<String, u16>,
    pub block_param_locals: BTreeMap<(ExecutableBlockRef, usize), u16>,
}

impl ExecutableSlotPlan {
    pub fn plan_clr(ctx: &ExecutableLoweringContext<'_>, function: &ExecutableFunction) -> Self {
        let mut plan =
            Self { local_types: Vec::new(), value_locals: BTreeMap::new(), var_locals: BTreeMap::new(), block_param_locals: BTreeMap::new() };
        // Pass 1: every block-param local first so StoreVar can alias loop-carried homes.
        for block in &function.blocks {
            for (index, parameter) in block.parameters.iter().enumerate() {
                let ty = function.value_types.get(parameter).cloned().unwrap_or(NyarType::Unit);
                let local = plan.alloc_local(ctx, &ty, ExecutableStorageKind::Value);
                plan.block_param_locals.insert((block.id, index), local);
                plan.value_locals.insert(*parameter, local);
            }
        }
        // Pass 2: instructions (StoreVar reuses named block-param locals when present).
        for block in &function.blocks {
            for instruction in &block.instructions {
                plan.collect_instruction(ctx, function, instruction);
            }
        }
        plan
    }

    fn collect_instruction(&mut self, ctx: &ExecutableLoweringContext<'_>, function: &ExecutableFunction, instruction: &ExecutableInstruction) {
        match &instruction.kind {
            ExecutableInstructionKind::StoreVar { name, ty, .. } => {
                // Always prefer the loop-carried block-param home when it exists (may overwrite
                // an earlier fresh `var_locals` entry allocated before that param was known).
                let local = if let Some(local) = self.named_block_param_local(function, name) {
                    self.var_locals.insert(name.clone(), local);
                    local
                }
                else if let Some(&local) = self.var_locals.get(name) {
                    local
                }
                else {
                    let storage_ty = ty.clone().or_else(|| {
                        crate::contracts::instruction_primary_result(instruction).and_then(|value| function.value_types.get(&value).cloned())
                    });
                    let local = self.alloc_local(ctx, &storage_ty.unwrap_or(NyarType::Unit), ExecutableStorageKind::Value);
                    self.var_locals.insert(name.clone(), local);
                    local
                };
                // Alias StoreVar SSA output to the same home so Jump/Return args remain
                // loadable even when the consuming block is emitted before the store.
                if let Some(output) = crate::contracts::instruction_primary_result(instruction) {
                    self.value_locals.insert(output, local);
                }
            }
            _ => {
                if let Some(output) = crate::contracts::instruction_primary_result(instruction) {
                    if self.value_locals.contains_key(&output) {
                        return;
                    }
                    let ty = function.value_types.get(&output).cloned().unwrap_or(NyarType::Unit);
                    let storage = storage_for_instruction(&instruction.kind);
                    let local = self.alloc_local(ctx, &ty, storage);
                    self.value_locals.insert(output, local);
                }
            }
        }
    }

    /// Prefer the latest block-parameter local for `name` (inner loops shadow outer).
    fn named_block_param_local(&self, function: &ExecutableFunction, name: &str) -> Option<u16> {
        let mut found = None;
        for value in &function.values {
            if let ValueOrigin::BlockParameter { name: param_name, .. } = &value.origin {
                if param_name == name {
                    if let Some(&local) = self.value_locals.get(&value.id) {
                        found = Some(local);
                    }
                }
            }
        }
        found
    }

    fn alloc_local(&mut self, ctx: &ExecutableLoweringContext<'_>, ty: &NyarType, storage: ExecutableStorageKind) -> u16 {
        let _ = storage;
        let msil_ty = nyar_type_to_msil(ty, ctx.layouts);
        let msil_ty = if matches!(msil_ty, MsilType::Void) { MsilType::Int32 { signed: true } } else { msil_ty };
        let index = self.local_types.len() as u16;
        self.local_types.push(msil_ty);
        index
    }

    pub fn plan_jvm(ctx: &ExecutableLoweringContext<'_>, function: &ExecutableFunction) -> Self {
        let mut plan =
            Self { local_types: Vec::new(), value_locals: BTreeMap::new(), var_locals: BTreeMap::new(), block_param_locals: BTreeMap::new() };
        let reachable = collect_reachable_blocks(function);
        for block_id in &reachable {
            let Some(block) = function.blocks.get(block_id.0 as usize)
            else {
                continue;
            };
            for (index, parameter) in block.parameters.iter().enumerate() {
                let ty = function.value_types.get(parameter).cloned().unwrap_or(NyarType::Unit);
                let field_types = jvm_type_field_types(ctx, &ty);
                let local = plan.alloc_local_span(ctx, &field_types, ExecutableStorageKind::Value);
                plan.block_param_locals.insert((block.id, index), local);
                plan.value_locals.insert(*parameter, local);
            }
            for instruction in &block.instructions {
                plan.collect_jvm_instruction(ctx, function, instruction);
            }
        }
        plan
    }

    fn collect_jvm_instruction(
        &mut self,
        ctx: &ExecutableLoweringContext<'_>,
        function: &ExecutableFunction,
        instruction: &ExecutableInstruction,
    ) {
        if let ExecutableInstructionKind::StoreVar { name, ty, .. } = &instruction.kind {
            // 与 CLR `plan_clr` 对齐：StoreVar 的 SSA output 必须别名到同一 local，
            // 否则 `peek_von_token(tokens, current)` 等调用的第二实参 ValueRef
            // 不在 `value_locals` 中，`emit_operand` 按 Unit 空发射，invoke 缺 int
            // → VerifyError: "Expecting to find integer on stack"。
            let local = if let Some(local) = self.named_block_param_local(function, name) {
                self.var_locals.insert(name.clone(), local);
                local
            }
            else if let Some(&local) = self.var_locals.get(name) {
                local
            }
            else {
                let storage_ty = ty.clone().or_else(|| {
                    crate::contracts::instruction_primary_result(instruction).and_then(|value| function.value_types.get(&value).cloned())
                });
                let storage_ty = storage_ty.unwrap_or(NyarType::Unit);
                // JVM 将多字段值类型装箱为单个堆对象引用。若此处按 flatten 叶子
                // 预留下连续槽，而相邻变量只占 1 槽（或反过来），StoreVar 扁平
                // 拷贝会踩踏 `short[]`/`String[]`，调用时触发
                // VerifyError: Incompatible argument to function。
                let field_types = jvm_boxed_or_flat_field_types(ctx, &storage_ty);
                let local = self.alloc_local_span(ctx, &field_types, ExecutableStorageKind::Value);
                self.var_locals.insert(name.clone(), local);
                local
            };
            if let Some(output) = crate::contracts::instruction_primary_result(instruction) {
                self.value_locals.insert(output, local);
            }
            return;
        }
        let Some(output) = crate::contracts::instruction_primary_result(instruction)
        else {
            return;
        };
        if self.value_locals.contains_key(&output) {
            return;
        }
        let ty = function.value_types.get(&output).cloned().unwrap_or(NyarType::Unit);
        let storage = storage_for_instruction(&instruction.kind);
        let field_types = jvm_field_types(ctx, &instruction.kind, &ty);
        let base = self.alloc_local_span(ctx, &field_types, storage);
        self.value_locals.insert(output, base);
    }

    fn alloc_local_span(&mut self, ctx: &ExecutableLoweringContext<'_>, field_types: &[NyarType], storage: ExecutableStorageKind) -> u16 {
        let base = self.local_types.len() as u16;
        if storage == ExecutableStorageKind::Value {
            for field_ty in field_types {
                for _ in 0..super::jvm_local_slots(field_ty) {
                    let _ = self.alloc_local(ctx, field_ty, storage);
                }
            }
        }
        else {
            // `Reference` here means “non-flattened aggregate home”, not JVM object.
            // Intrinsic/Call results still need JVM slot width: `i64`/`f64` occupy two
            // locals. Allocating a single slot made consecutive longs overlap
            // (`lstore N` then `lstore N+1`) and broke verification downstream.
            let ty = field_types.first().cloned().unwrap_or(NyarType::Unit);
            for _ in 0..super::jvm_local_slots(&ty).max(1) {
                let _ = self.alloc_local(ctx, &ty, storage);
            }
        }
        base
    }
}

fn storage_for_instruction(kind: &ExecutableInstructionKind) -> ExecutableStorageKind {
    match kind {
        // Layout storage lives on AggregateLayout / RepresentationPlan, not InstructionKind.
        ExecutableInstructionKind::StructNew { .. }
        | ExecutableInstructionKind::TupleNew { .. }
        | ExecutableInstructionKind::ArrayFromElements { .. }
        | ExecutableInstructionKind::ArrayNew { .. }
        | ExecutableInstructionKind::FieldGet { .. }
        | ExecutableInstructionKind::FieldSet { .. } => ExecutableStorageKind::Reference,
        _ => ExecutableStorageKind::Reference,
    }
}

fn jvm_field_types(ctx: &ExecutableLoweringContext<'_>, kind: &ExecutableInstructionKind, output_ty: &NyarType) -> Vec<NyarType> {
    match kind {
        ExecutableInstructionKind::StructNew { type_name, .. } => {
            let layout = ctx.layout_by_type_name(type_name);
            match layout {
                Some(l) => l.fields.iter().flat_map(|field| ctx.flatten_value_type_slots(&field.ty)).collect(),
                None => ctx.flatten_value_type_slots(output_ty),
            }
        }
        ExecutableInstructionKind::TupleNew { .. } | ExecutableInstructionKind::ArrayFromElements { .. } => {
            ctx.flatten_value_type_slots(output_ty)
        }
        _ => jvm_type_field_types(ctx, output_ty),
    }
}

pub(crate) fn jvm_type_field_types(ctx: &ExecutableLoweringContext<'_>, ty: &NyarType) -> Vec<NyarType> {
    ctx.flatten_value_type_slots(ty)
}

/// JVM StoreVar 槽位：多字段值类型只占 1 个引用槽（装箱）；其余按 flatten 叶子展开。
fn jvm_boxed_or_flat_field_types(ctx: &ExecutableLoweringContext<'_>, ty: &NyarType) -> Vec<NyarType> {
    if let NyarType::Named(name) = ty {
        let s = name.as_str();
        // 与 jvm `needs_boxing` / `map_primitive_name_to_jvm` 对齐：primitive 别名不装箱。
        let is_primitive_alias = matches!(
            s,
            "bool"
                | "char"
                | "i8"
                | "u8"
                | "i16"
                | "u16"
                | "i32"
                | "u32"
                | "i64"
                | "u64"
                | "i128"
                | "u128"
                | "f32"
                | "f64"
                | "f128"
                | "usize"
                | "isize"
                | "void"
                | "unit"
        ) || s.contains("primitive.");
        if !is_primitive_alias && ctx.is_value_type_name(s) {
            if let Some(layout) = ctx.layout_by_type_name(s) {
                if layout.fields.len() > 1 {
                    return vec![ty.clone()];
                }
            }
        }
    }
    jvm_type_field_types(ctx, ty)
}
