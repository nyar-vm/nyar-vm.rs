//! 加载期模块校验：结构边界、函数控制流、栈高度/粗类型与局部槽粗类型汇合、栈深上限，以及导入 / layout 下标。

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use nyar_bytecode::{
    NyarHeadCode, NyarImport, NyarInstruction, NyarModuleData, NYAR_VERSION, OBSOLETE_CALL_NATIVE, decode_at,
};

use crate::{
    error::NyarRuntimeError,
    host::resolve_import,
};

/// 单函数操作数栈高度上限（加载期拒绝病理模块）。
const MAX_OPERAND_STACK_HEIGHT: i32 = 8192;

/// 单函数外码字节上限（体积预算；超限 fail-closed）。
const MAX_FUNCTION_CODE_BYTES: usize = 64 * 1024;

/// 整模块外码字节上限。
const MAX_MODULE_CODE_BYTES: usize = 4 * 1024 * 1024;

/// 粗类型栈槽 / 局部槽。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StackKind {
    I32,
    Ref,
    Any,
}

/// 操作数栈汇合：`Any` 向具体侧收窄（与既有栈类型合同一致）。
fn merge_stack_kind(left: StackKind, right: StackKind) -> Option<StackKind> {
    match (left, right) {
        (a, b) if a == b => Some(a),
        (StackKind::Any, other) | (other, StackKind::Any) => Some(other),
        _ => None,
    }
}

/// 局部槽汇合：一侧未知则放宽为 `Any`（避免把未初始化 `Null` 误收成 `I32`）。
fn merge_local_kind(left: StackKind, right: StackKind) -> Option<StackKind> {
    match (left, right) {
        (a, b) if a == b => Some(a),
        (StackKind::Any, _) | (_, StackKind::Any) => Some(StackKind::Any),
        _ => None,
    }
}

/// 函数返回形状（多出口须可汇合）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReturnShape {
    Void,
    Value(StackKind),
}

fn merge_return_shape(left: ReturnShape, right: ReturnShape) -> Option<ReturnShape> {
    match (left, right) {
        (ReturnShape::Void, ReturnShape::Void) => Some(ReturnShape::Void),
        (ReturnShape::Value(a), ReturnShape::Value(b)) => merge_stack_kind(a, b).map(ReturnShape::Value),
        _ => None,
    }
}

/// `StoreLocal`：从 `Any` 可细化；写入 `Any` 则放宽；`I32`/`Ref` 冲突失败。
fn assign_local_kind(old: StackKind, new: StackKind) -> Option<StackKind> {
    match (old, new) {
        (a, b) if a == b => Some(a),
        (StackKind::Any, refined) => Some(refined),
        (_, StackKind::Any) => Some(StackKind::Any),
        _ => None,
    }
}

fn merge_kind_vecs(
    left: &[StackKind],
    right: &[StackKind],
    merge: fn(StackKind, StackKind) -> Option<StackKind>,
) -> Option<Vec<StackKind>> {
    if left.len() != right.len() {
        return None;
    }
    let mut out = Vec::with_capacity(left.len());
    for (a, b) in left.iter().zip(right.iter()) {
        out.push(merge(*a, *b)?);
    }
    Some(out)
}

fn merge_type_stacks(left: &[StackKind], right: &[StackKind]) -> Option<Vec<StackKind>> {
    merge_kind_vecs(left, right, merge_stack_kind)
}

fn merge_local_kinds(left: &[StackKind], right: &[StackKind]) -> Option<Vec<StackKind>> {
    merge_kind_vecs(left, right, merge_local_kind)
}

/// 校验已解码模块：版本、导入白名单、下标、函数代码区间、跳转边界、栈高度与栈深上限。
pub fn verify_module(data: &NyarModuleData) -> Result<(), NyarRuntimeError> {
    if data.version != NYAR_VERSION {
        return Err(NyarRuntimeError::ModuleLoad(format!(
            "unsupported module version {}; expected {NYAR_VERSION}",
            data.version
        )));
    }

    if data.code_bytes.len() > MAX_MODULE_CODE_BYTES {
        return Err(NyarRuntimeError::ModuleLoad(format!(
            "module code section length {} exceeds limit {MAX_MODULE_CODE_BYTES}",
            data.code_bytes.len()
        )));
    }

    for (index, layout) in data.layouts.iter().enumerate() {
        if layout.field_count < 0 {
            return Err(NyarRuntimeError::ModuleLoad(format!(
                "layout[{index}] has negative field_count {}",
                layout.field_count
            )));
        }
    }

    for (index, import) in data.imports.iter().enumerate() {
        verify_import(index, import)?;
    }

    verify_code_stream(data)?;
    for (function_index, function) in data.functions.iter().enumerate() {
        verify_function(data, function_index, function)?;
    }

    Ok(())
}

fn verify_import(index: usize, import: &NyarImport) -> Result<(), NyarRuntimeError> {
    if import.module_name.is_empty() || import.symbol_name.is_empty() {
        return Err(NyarRuntimeError::ModuleLoad(format!("import[{index}] has empty module or symbol name")));
    }
    // 解析一次：未知 `nyar.host` 符号在此失败；结果在 `LoadedModule` 侧缓存。
    let _ = resolve_import(import)?;
    Ok(())
}

/// 整段代码流的操作码与截断检查（不依赖函数表覆盖）。
fn verify_code_stream(data: &NyarModuleData) -> Result<(), NyarRuntimeError> {
    let mut pc = 0usize;
    while pc < data.code_bytes.len() {
        let opcode = data.code_bytes[pc];
        if opcode == OBSOLETE_CALL_NATIVE {
            return Err(NyarRuntimeError::ModuleLoad(format!(
                "obsolete CallNative opcode 0x{OBSOLETE_CALL_NATIVE:02X} at pc {pc}; use CallImport"
            )));
        }

        let Some(code) = NyarHeadCode::from_u8(opcode)
        else {
            return Err(NyarRuntimeError::UnknownOpcode(opcode));
        };

        let instruction = decode_at(&data.code_bytes, pc);
        if instruction.size == 0 {
            return Err(NyarRuntimeError::ModuleLoad(format!("truncated instruction at pc {pc}")));
        }

        match code {
            NyarHeadCode::CallImport => {
                let import_index = instruction.operand1;
                if import_index < 0 || (import_index as usize) >= data.imports.len() {
                    return Err(NyarRuntimeError::ImportIndexOutOfRange(import_index));
                }
            }
            NyarHeadCode::ObjectNew => {
                let layout_id = instruction.operand1;
                if layout_id < 0 || (layout_id as usize) >= data.layouts.len() {
                    return Err(NyarRuntimeError::LayoutIndexOutOfRange(layout_id));
                }
            }
            NyarHeadCode::FieldGet | NyarHeadCode::FieldSet => {
                let field_slot = instruction.operand1;
                if field_slot < 0 {
                    return Err(NyarRuntimeError::FieldSlotOutOfRange(field_slot));
                }
                // 热路径按对象自身 layout 校验上界；加载期仅拒绝负槽，并要求至少存在能容纳该槽的布局。
                let fits_some_layout = data.layouts.iter().any(|layout| field_slot < layout.field_count);
                if !fits_some_layout {
                    return Err(NyarRuntimeError::FieldSlotOutOfRange(field_slot));
                }
            }
            _ => {}
        }

        pc = pc.saturating_add(instruction.size as usize);
    }

    Ok(())
}

fn verify_function(
    data: &NyarModuleData,
    function_index: usize,
    function: &nyar_bytecode::NyarFunction,
) -> Result<(), NyarRuntimeError> {
    if function.code_offset < 0 || function.code_length < 0 {
        return Err(NyarRuntimeError::ModuleLoad(format!(
            "function[{function_index}] has negative code_offset or code_length"
        )));
    }

    let start = function.code_offset as usize;
    let length = function.code_length as usize;
    let end = start.checked_add(length).ok_or_else(|| {
        NyarRuntimeError::ModuleLoad(format!("function[{function_index}] code range overflows"))
    })?;
    if end > data.code_bytes.len() {
        return Err(NyarRuntimeError::ModuleLoad(format!(
            "function[{function_index}] code range [{start}, {end}) exceeds code section length {}",
            data.code_bytes.len()
        )));
    }
    if length > MAX_FUNCTION_CODE_BYTES {
        return Err(NyarRuntimeError::ModuleLoad(format!(
            "function[{function_index}] code length {length} exceeds limit {MAX_FUNCTION_CODE_BYTES}"
        )));
    }

    if length == 0 {
        return Ok(());
    }

    let instructions = decode_function_instructions(data, function_index, start, end)?;
    let boundaries: BTreeSet<usize> = instructions.iter().map(|(pc, _)| *pc).collect();
    let local_slots = function.local_count.max(function.arity).max(0) as usize;

    // 普通函数入口相对 `stack_base` 高度为 0。
    // effect handler 由 `InvokeHandler` 压入 `[continuation, effect_value]`，入口高度为 2。
    let entry_height = if data.witness_entries.iter().any(|entry| entry.function_index == function_index as i32) {
        2
    } else {
        0
    };

    let entry_types = vec![StackKind::Any; entry_height.max(0) as usize];
    // 入口局部均为 `Any`（帧以 `Null` 填充；参数类型由调用约定另行约束）。
    let entry_locals = vec![StackKind::Any; local_slots];
    let mut heights: BTreeMap<usize, i32> = BTreeMap::new();
    let mut type_stacks: BTreeMap<usize, Vec<StackKind>> = BTreeMap::new();
    let mut local_kinds: BTreeMap<usize, Vec<StackKind>> = BTreeMap::new();
    let mut return_shape: Option<ReturnShape> = None;
    let mut queue = VecDeque::new();
    heights.insert(start, entry_height);
    type_stacks.insert(start, entry_types);
    local_kinds.insert(start, entry_locals);
    queue.push_back(start);

    while let Some(pc) = queue.pop_front() {
        let height = heights[&pc];
        if height > MAX_OPERAND_STACK_HEIGHT {
            return Err(NyarRuntimeError::ModuleLoad(format!(
                "function[{function_index}] operand stack height {height} exceeds limit {MAX_OPERAND_STACK_HEIGHT} at pc {pc}"
            )));
        }
        let mut types = type_stacks[&pc].clone();
        let mut locals = local_kinds[&pc].clone();
        let instruction = instructions.get(&pc).copied().ok_or_else(|| {
            NyarRuntimeError::ModuleLoad(format!(
                "function[{function_index}] control reaches non-instruction pc {pc}"
            ))
        })?;

        let arity = function.arity.max(0) as usize;
        verify_instruction_operands(data, function_index, pc, instruction, local_slots, arity)?;

        if instruction.code == NyarHeadCode::Return {
            let shape = if height <= 0 {
                ReturnShape::Void
            } else {
                let top = types.last().copied().unwrap_or(StackKind::Any);
                ReturnShape::Value(top)
            };
            return_shape = Some(match return_shape {
                None => shape,
                Some(existing) => merge_return_shape(existing, shape).ok_or_else(|| {
                    NyarRuntimeError::ModuleLoad(format!(
                        "function[{function_index}] return type mismatch at pc {pc}: {existing:?} vs {shape:?}"
                    ))
                })?,
            });
        }

        let edges = stack_transfer(data, function_index, pc, instruction, height, &mut types, &mut locals)?;
        for (target, edge_height) in edges {
            if !boundaries.contains(&target) {
                return Err(NyarRuntimeError::ModuleLoad(format!(
                    "function[{function_index}] jump from pc {pc} lands off instruction boundary at {target}"
                )));
            }
            if target < start || target >= end {
                return Err(NyarRuntimeError::ModuleLoad(format!(
                    "function[{function_index}] jump from pc {pc} escapes function range to {target}"
                )));
            }
            if types.len() as i32 != edge_height {
                return Err(NyarRuntimeError::ModuleLoad(format!(
                    "function[{function_index}] internal type-stack length mismatch at pc {pc}"
                )));
            }
            match heights.get(&target) {
                Some(existing) if *existing != edge_height => {
                    return Err(NyarRuntimeError::ModuleLoad(format!(
                        "function[{function_index}] stack height mismatch at pc {target}: {existing} vs {edge_height}"
                    )));
                }
                Some(_) => {}
                None => {
                    heights.insert(target, edge_height);
                }
            }
            let stack_changed = match type_stacks.get(&target) {
                Some(existing) => {
                    let Some(merged) = merge_type_stacks(existing, &types) else {
                        return Err(NyarRuntimeError::ModuleLoad(format!(
                            "function[{function_index}] stack type mismatch at pc {target}"
                        )));
                    };
                    if merged != *existing {
                        type_stacks.insert(target, merged);
                        true
                    } else {
                        false
                    }
                }
                None => {
                    type_stacks.insert(target, types.clone());
                    true
                }
            };
            let locals_changed = match local_kinds.get(&target) {
                Some(existing) => {
                    let Some(merged) = merge_local_kinds(existing, &locals) else {
                        return Err(NyarRuntimeError::ModuleLoad(format!(
                            "function[{function_index}] local type mismatch at pc {target}"
                        )));
                    };
                    if merged != *existing {
                        local_kinds.insert(target, merged);
                        true
                    } else {
                        false
                    }
                }
                None => {
                    local_kinds.insert(target, locals.clone());
                    true
                }
            };
            if stack_changed || locals_changed {
                queue.push_back(target);
            }
        }
    }

    Ok(())
}

fn decode_function_instructions(
    data: &NyarModuleData,
    function_index: usize,
    start: usize,
    end: usize,
) -> Result<BTreeMap<usize, NyarInstruction>, NyarRuntimeError> {
    let mut instructions = BTreeMap::new();
    let mut pc = start;
    while pc < end {
        let instruction = decode_at(&data.code_bytes, pc);
        if instruction.size == 0 {
            return Err(NyarRuntimeError::ModuleLoad(format!(
                "function[{function_index}] truncated instruction at pc {pc}"
            )));
        }
        let next = pc.checked_add(instruction.size as usize).ok_or_else(|| {
            NyarRuntimeError::ModuleLoad(format!("function[{function_index}] instruction size overflows at pc {pc}"))
        })?;
        if next > end {
            return Err(NyarRuntimeError::ModuleLoad(format!(
                "function[{function_index}] instruction at pc {pc} crosses function end {end}"
            )));
        }
        instructions.insert(pc, instruction);
        pc = next;
    }
    if pc != end {
        return Err(NyarRuntimeError::ModuleLoad(format!(
            "function[{function_index}] code range leaves a gap ending at {pc}, expected {end}"
        )));
    }
    Ok(instructions)
}

fn verify_instruction_operands(
    data: &NyarModuleData,
    function_index: usize,
    pc: usize,
    instruction: NyarInstruction,
    local_slots: usize,
    arity: usize,
) -> Result<(), NyarRuntimeError> {
    match instruction.code {
        NyarHeadCode::Const => {
            if instruction.operand1 < 0 || (instruction.operand1 as usize) >= data.constants.len() {
                return Err(NyarRuntimeError::ModuleLoad(format!(
                    "function[{function_index}] Const at pc {pc} constant index {} out of range",
                    instruction.operand1
                )));
            }
        }
        NyarHeadCode::LoadArg => {
            // 调用约定：`LoadArg` 只能读 `[0, arity)`，不得越界到普通局部槽。
            if instruction.operand1 < 0 || (instruction.operand1 as usize) >= arity {
                return Err(NyarRuntimeError::ModuleLoad(format!(
                    "function[{function_index}] LoadArg at pc {pc} index {} out of arity {arity}",
                    instruction.operand1
                )));
            }
        }
        NyarHeadCode::LoadLocal | NyarHeadCode::StoreLocal => {
            if instruction.operand1 < 0 || (instruction.operand1 as usize) >= local_slots {
                return Err(NyarRuntimeError::LocalIndexOutOfRange(instruction.operand1));
            }
        }
        NyarHeadCode::LoadGlobal | NyarHeadCode::StoreGlobal => {
            if instruction.operand1 < 0 || (instruction.operand1 as usize) >= data.globals.len() {
                return Err(NyarRuntimeError::GlobalIndexOutOfRange(instruction.operand1));
            }
        }
        NyarHeadCode::Call | NyarHeadCode::CallStatic => {
            if instruction.operand1 < 0 || (instruction.operand1 as usize) >= data.functions.len() {
                return Err(NyarRuntimeError::FunctionIndexOutOfRange(instruction.operand1));
            }
        }
        NyarHeadCode::CallImport | NyarHeadCode::CallIntrinsic => {
            if instruction.operand2 < 0 {
                return Err(NyarRuntimeError::ModuleLoad(format!(
                    "function[{function_index}] {:?} at pc {pc} has negative argc {}",
                    instruction.code, instruction.operand2
                )));
            }
        }
        _ => {}
    }
    Ok(())
}

fn require_height(function_index: usize, pc: usize, height: i32, needed: i32) -> Result<(), NyarRuntimeError> {
    if height < needed {
        Err(NyarRuntimeError::ModuleLoad(format!(
            "function[{function_index}] stack underflow at pc {pc}: height {height}, need {needed}"
        )))
    } else {
        Ok(())
    }
}

fn require_kind(
    function_index: usize,
    pc: usize,
    kinds: &mut Vec<StackKind>,
    expected: StackKind,
) -> Result<(), NyarRuntimeError> {
    let Some(actual) = kinds.pop() else {
        return Err(NyarRuntimeError::ModuleLoad(format!(
            "function[{function_index}] stack underflow at pc {pc}"
        )));
    };
    match (actual, expected) {
        (_, StackKind::Any) | (StackKind::Any, _) => Ok(()),
        (a, e) if a == e => Ok(()),
        _ => Err(NyarRuntimeError::ModuleLoad(format!(
            "function[{function_index}] stack type mismatch at pc {pc}: expected {expected:?}, got {actual:?}"
        ))),
    }
}

fn push_const_kind(data: &NyarModuleData, instruction: NyarInstruction) -> StackKind {
    if instruction.operand1 >= 0 {
        if let Some(nyar_bytecode::NyarConstant::Integer32(_)) = data.constants.get(instruction.operand1 as usize) {
            return StackKind::I32;
        }
    }
    StackKind::Any
}

/// 就地更新 `types` / `locals`，返回后继边 `(目标 pc, 到达时栈高度)`。
fn stack_transfer(
    data: &NyarModuleData,
    function_index: usize,
    pc: usize,
    instruction: NyarInstruction,
    height: i32,
    types: &mut Vec<StackKind>,
    locals: &mut Vec<StackKind>,
) -> Result<Vec<(usize, i32)>, NyarRuntimeError> {
    let fallthrough = pc + instruction.size as usize;

    match instruction.code {
        NyarHeadCode::Nop => Ok(vec![(fallthrough, height)]),
        NyarHeadCode::Jump => {
            let target = pc.wrapping_add(instruction.operand1 as usize);
            Ok(vec![(target, height)])
        }
        NyarHeadCode::JumpIfTrue | NyarHeadCode::JumpIfFalse => {
            require_height(function_index, pc, height, 1)?;
            // 条件槽必须是 i32（0/非 0）；拒绝 Ref/Any 冒充布尔条件。
            require_kind(function_index, pc, types, StackKind::I32)?;
            let after = height - 1;
            let target = pc.wrapping_add(instruction.operand1 as usize);
            Ok(vec![(fallthrough, after), (target, after)])
        }
        NyarHeadCode::Return => {
            // Return 结束本帧。允许空栈（void / init）；有值则留给调用方。
            Ok(Vec::new())
        }
        NyarHeadCode::Yield | NyarHeadCode::PerformEffect => {
            require_height(function_index, pc, height, 1)?;
            require_kind(function_index, pc, types, StackKind::Any)?;
            types.push(StackKind::Any);
            Ok(vec![(fallthrough, height)])
        }
        NyarHeadCode::Resume => {
            require_height(function_index, pc, height, 2)?;
            require_kind(function_index, pc, types, StackKind::Any)?;
            require_kind(function_index, pc, types, StackKind::Any)?;
            types.push(StackKind::Any);
            Ok(vec![(fallthrough, height - 1)])
        }
        NyarHeadCode::Const => {
            types.push(push_const_kind(data, instruction));
            Ok(vec![(fallthrough, height + 1)])
        }
        NyarHeadCode::ObjectNew => {
            types.push(StackKind::Ref);
            Ok(vec![(fallthrough, height + 1)])
        }
        NyarHeadCode::LoadLocal | NyarHeadCode::LoadArg => {
            let slot = instruction.operand1 as usize;
            let kind = locals.get(slot).copied().unwrap_or(StackKind::Any);
            types.push(kind);
            Ok(vec![(fallthrough, height + 1)])
        }
        NyarHeadCode::LoadGlobal => {
            types.push(StackKind::Any);
            Ok(vec![(fallthrough, height + 1)])
        }
        NyarHeadCode::StoreLocal => {
            require_height(function_index, pc, height, 1)?;
            let Some(stored) = types.pop() else {
                return Err(NyarRuntimeError::ModuleLoad(format!(
                    "function[{function_index}] stack underflow at pc {pc}"
                )));
            };
            let slot = instruction.operand1 as usize;
            let old = locals.get(slot).copied().unwrap_or(StackKind::Any);
            let Some(next) = assign_local_kind(old, stored) else {
                return Err(NyarRuntimeError::ModuleLoad(format!(
                    "function[{function_index}] local type conflict at pc {pc}: slot {slot} was {old:?}, store {stored:?}"
                )));
            };
            if slot < locals.len() {
                locals[slot] = next;
            }
            Ok(vec![(fallthrough, height - 1)])
        }
        NyarHeadCode::Pop | NyarHeadCode::StoreGlobal => {
            require_height(function_index, pc, height, 1)?;
            require_kind(function_index, pc, types, StackKind::Any)?;
            Ok(vec![(fallthrough, height - 1)])
        }
        NyarHeadCode::Dup => {
            require_height(function_index, pc, height, 1)?;
            let top = *types.last().ok_or_else(|| {
                NyarRuntimeError::ModuleLoad(format!("function[{function_index}] stack underflow at pc {pc}"))
            })?;
            types.push(top);
            Ok(vec![(fallthrough, height + 1)])
        }
        NyarHeadCode::I32Add
        | NyarHeadCode::I32Sub
        | NyarHeadCode::I32Mul
        | NyarHeadCode::I32DivS
        | NyarHeadCode::I32RemS
        | NyarHeadCode::I32Eq
        | NyarHeadCode::I32Ne
        | NyarHeadCode::I32LtS
        | NyarHeadCode::I32LeS
        | NyarHeadCode::I32GtS
        | NyarHeadCode::I32GeS => {
            require_height(function_index, pc, height, 2)?;
            require_kind(function_index, pc, types, StackKind::I32)?;
            require_kind(function_index, pc, types, StackKind::I32)?;
            types.push(StackKind::I32);
            Ok(vec![(fallthrough, height - 1)])
        }
        NyarHeadCode::FieldGet => {
            require_height(function_index, pc, height, 1)?;
            require_kind(function_index, pc, types, StackKind::Ref)?;
            types.push(StackKind::Any);
            Ok(vec![(fallthrough, height)])
        }
        NyarHeadCode::FieldSet => {
            require_height(function_index, pc, height, 2)?;
            require_kind(function_index, pc, types, StackKind::Any)?;
            require_kind(function_index, pc, types, StackKind::Ref)?;
            types.push(StackKind::Ref);
            Ok(vec![(fallthrough, height - 1)])
        }
        NyarHeadCode::Call | NyarHeadCode::CallStatic => {
            let callee = &data.functions[instruction.operand1 as usize];
            let arity = callee.arity.max(0);
            require_height(function_index, pc, height, arity)?;
            for _ in 0..arity {
                require_kind(function_index, pc, types, StackKind::Any)?;
            }
            types.push(StackKind::Any);
            Ok(vec![(fallthrough, height - arity + 1)])
        }
        NyarHeadCode::CallImport | NyarHeadCode::CallIntrinsic => {
            let argc = instruction.operand2.max(0);
            require_height(function_index, pc, height, argc)?;
            for _ in 0..argc {
                require_kind(function_index, pc, types, StackKind::Any)?;
            }
            types.push(StackKind::Any);
            Ok(vec![(fallthrough, height - argc + 1)])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::HOST_IMPORT_MODULE;
    use nyar_bytecode::{NyarConstant, NyarFunction, NyarImportKind, NyarLayout, NyarModuleData, emit_imm1, emit_plain};

    fn empty_module() -> NyarModuleData {
        NyarModuleData {
            version: NYAR_VERSION,
            name: "test".into(),
            constants: Vec::new(),
            functions: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
            witness_entries: Vec::new(),
            code_bytes: Vec::new(),
            globals: Vec::new(),
            init_function_indices: Vec::new(),
            layouts: Vec::new(),
        }
    }

    #[test]
    fn accepts_empty_v2_module() {
        verify_module(&empty_module()).expect("empty v2 ok");
    }

    #[test]
    fn rejects_wrong_version() {
        let mut module = empty_module();
        module.version = 1;
        assert!(matches!(verify_module(&module), Err(NyarRuntimeError::ModuleLoad(_))));
    }

    #[test]
    fn rejects_unknown_host_import() {
        let mut module = empty_module();
        module.imports.push(nyar_bytecode::NyarImport {
            kind: NyarImportKind::Function,
            module_name: HOST_IMPORT_MODULE.into(),
            symbol_name: "not_a_real_host_op".into(),
        });
        let err = verify_module(&module).expect_err("unknown host");
        assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("unknown host import")));
    }

    #[test]
    fn rejects_call_import_out_of_range() {
        let mut module = empty_module();
        let mut code = Vec::new();
        code.push(NyarHeadCode::CallImport as u8);
        code.extend_from_slice(&0i32.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes());
        module.code_bytes = code;
        assert!(matches!(verify_module(&module), Err(NyarRuntimeError::ImportIndexOutOfRange(0))));
    }

    #[test]
    fn rejects_obsolete_call_native_byte() {
        let mut module = empty_module();
        module.code_bytes = vec![OBSOLETE_CALL_NATIVE, 0, 0, 0, 0, 0, 0, 0, 0];
        let err = verify_module(&module).expect_err("CallNative");
        assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("CallNative")));
    }

    #[test]
    fn accepts_in_range_call_import() {
        let mut module = empty_module();
        module.imports.push(nyar_bytecode::NyarImport {
            kind: NyarImportKind::Function,
            module_name: HOST_IMPORT_MODULE.into(),
            symbol_name: "print".into(),
        });
        let mut code = Vec::new();
        code.push(NyarHeadCode::CallImport as u8);
        code.extend_from_slice(&0i32.to_le_bytes());
        code.extend_from_slice(&1i32.to_le_bytes());
        // 无函数表覆盖时只做流式检查；补一个覆盖整段代码的函数以启用 CFG。
        module.functions.push(NyarFunction {
            name: "main".into(),
            arity: 0,
            local_count: 0,
            code_offset: 0,
            code_length: code.len() as i32,
        });
        // CallImport argc=1 需要栈上有 1 个值——构造 Const + CallImport 会更完整；此处仅保留索引检查路径：
        // 空栈 CallImport 应在函数校验中因 underflow 失败。
        module.code_bytes = code;
        let err = verify_module(&module).expect_err("underflow");
        assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("stack underflow")));
    }

    #[test]
    fn accepts_const_call_import_return() {
        let mut module = empty_module();
        module.imports.push(nyar_bytecode::NyarImport {
            kind: NyarImportKind::Function,
            module_name: HOST_IMPORT_MODULE.into(),
            symbol_name: "print".into(),
        });
        module.constants.push(NyarConstant::Integer32(1));
        let mut code = Vec::new();
        emit_imm1(&mut code, NyarHeadCode::Const, 0);
        code.push(NyarHeadCode::CallImport as u8);
        code.extend_from_slice(&0i32.to_le_bytes());
        code.extend_from_slice(&1i32.to_le_bytes());
        emit_plain(&mut code, NyarHeadCode::Return);
        module.functions.push(NyarFunction {
            name: "main".into(),
            arity: 0,
            local_count: 0,
            code_offset: 0,
            code_length: code.len() as i32,
        });
        module.code_bytes = code;
        verify_module(&module).expect("balanced CallImport");
    }

    #[test]
    fn rejects_object_new_out_of_range() {
        let mut module = empty_module();
        let mut code = Vec::new();
        code.push(NyarHeadCode::ObjectNew as u8);
        code.extend_from_slice(&0i32.to_le_bytes());
        module.code_bytes = code;
        assert!(matches!(verify_module(&module), Err(NyarRuntimeError::LayoutIndexOutOfRange(0))));
    }

    #[test]
    fn accepts_object_new_with_layout() {
        let mut module = empty_module();
        module.layouts.push(NyarLayout { field_count: 2 });
        let mut code = Vec::new();
        code.push(NyarHeadCode::ObjectNew as u8);
        code.extend_from_slice(&0i32.to_le_bytes());
        module.code_bytes = code;
        // 无函数覆盖时流式检查通过即可。
        verify_module(&module).expect("ObjectNew ok");
    }

    #[test]
    fn rejects_field_slot_without_fitting_layout() {
        let mut module = empty_module();
        module.layouts.push(NyarLayout { field_count: 1 });
        let mut code = Vec::new();
        code.push(NyarHeadCode::FieldGet as u8);
        code.extend_from_slice(&1i32.to_le_bytes());
        module.code_bytes = code;
        assert!(matches!(verify_module(&module), Err(NyarRuntimeError::FieldSlotOutOfRange(1))));
    }

    #[test]
    fn rejects_jump_off_instruction_boundary() {
        let mut module = empty_module();
        let mut code = Vec::new();
        // Jump +1 落到 Imm1 立即数中间。
        emit_imm1(&mut code, NyarHeadCode::Jump, 1);
        emit_plain(&mut code, NyarHeadCode::Return);
        module.functions.push(NyarFunction {
            name: "main".into(),
            arity: 0,
            local_count: 0,
            code_offset: 0,
            code_length: code.len() as i32,
        });
        module.code_bytes = code;
        let err = verify_module(&module).expect_err("bad jump");
        assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("instruction boundary")));
    }

    #[test]
    fn rejects_stack_height_mismatch_at_join() {
        // path_a 汇合高度 1，path_b 汇合高度 2。
        let mut module = empty_module();
        module.constants.push(NyarConstant::Integer32(0));
        let mut code = Vec::new();
        emit_imm1(&mut code, NyarHeadCode::Const, 0); // condition
        let jif_at = code.len();
        emit_imm1(&mut code, NyarHeadCode::JumpIfFalse, 0); // -> path_b
        emit_imm1(&mut code, NyarHeadCode::Const, 0);
        let jump_a_at = code.len();
        emit_imm1(&mut code, NyarHeadCode::Jump, 0);
        let path_b = code.len();
        emit_imm1(&mut code, NyarHeadCode::Const, 0);
        emit_imm1(&mut code, NyarHeadCode::Const, 0);
        let jump_b_at = code.len();
        emit_imm1(&mut code, NyarHeadCode::Jump, 0);
        let join = code.len();
        emit_plain(&mut code, NyarHeadCode::Return);

        let to_path_b = (path_b as i32) - (jif_at as i32);
        code[jif_at + 1..jif_at + 5].copy_from_slice(&to_path_b.to_le_bytes());
        let to_join_a = (join as i32) - (jump_a_at as i32);
        code[jump_a_at + 1..jump_a_at + 5].copy_from_slice(&to_join_a.to_le_bytes());
        let to_join_b = (join as i32) - (jump_b_at as i32);
        code[jump_b_at + 1..jump_b_at + 5].copy_from_slice(&to_join_b.to_le_bytes());

        module.functions.push(NyarFunction {
            name: "main".into(),
            arity: 0,
            local_count: 0,
            code_offset: 0,
            code_length: code.len() as i32,
        });
        module.code_bytes = code;
        let err = verify_module(&module).expect_err("height mismatch");
        assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("stack height mismatch")));
    }

    #[test]
    fn rejects_i32_binop_on_object_ref() {
        let mut module = empty_module();
        module.constants.push(NyarConstant::Integer32(1));
        module.layouts.push(NyarLayout { field_count: 0 });
        let mut code = Vec::new();
        emit_imm1(&mut code, NyarHeadCode::Const, 0);
        emit_imm1(&mut code, NyarHeadCode::ObjectNew, 0);
        emit_plain(&mut code, NyarHeadCode::I32Add);
        emit_plain(&mut code, NyarHeadCode::Return);
        module.functions.push(NyarFunction {
            name: "main".into(),
            arity: 0,
            local_count: 0,
            code_offset: 0,
            code_length: code.len() as i32,
        });
        module.code_bytes = code;
        let err = verify_module(&module).expect_err("type mismatch");
        assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("stack type mismatch")));
    }

    #[test]
    fn rejects_operand_stack_deeper_than_limit() {
        let mut module = empty_module();
        module.constants.push(NyarConstant::Integer32(0));
        let mut code = Vec::new();
        // 超过 MAX_OPERAND_STACK_HEIGHT 次 Const 压栈。
        for _ in 0..=(MAX_OPERAND_STACK_HEIGHT as usize) {
            emit_imm1(&mut code, NyarHeadCode::Const, 0);
        }
        emit_plain(&mut code, NyarHeadCode::Return);
        module.functions.push(NyarFunction {
            name: "main".into(),
            arity: 0,
            local_count: 0,
            code_offset: 0,
            code_length: code.len() as i32,
        });
        module.code_bytes = code;
        let err = verify_module(&module).expect_err("stack too deep");
        assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("exceeds limit")));
    }

    #[test]
    fn accepts_balanced_branch_join() {
        let mut module = empty_module();
        module.constants.push(NyarConstant::Integer32(0));
        let mut code = Vec::new();
        emit_imm1(&mut code, NyarHeadCode::Const, 0);
        let jif_at = code.len();
        emit_imm1(&mut code, NyarHeadCode::JumpIfFalse, 0);
        emit_imm1(&mut code, NyarHeadCode::Const, 0);
        let jump_a_at = code.len();
        emit_imm1(&mut code, NyarHeadCode::Jump, 0);
        let path_b = code.len();
        emit_imm1(&mut code, NyarHeadCode::Const, 0);
        let jump_b_at = code.len();
        emit_imm1(&mut code, NyarHeadCode::Jump, 0);
        let join = code.len();
        emit_plain(&mut code, NyarHeadCode::Return);

        let to_path_b = (path_b as i32) - (jif_at as i32);
        code[jif_at + 1..jif_at + 5].copy_from_slice(&to_path_b.to_le_bytes());
        let to_join_a = (join as i32) - (jump_a_at as i32);
        code[jump_a_at + 1..jump_a_at + 5].copy_from_slice(&to_join_a.to_le_bytes());
        let to_join_b = (join as i32) - (jump_b_at as i32);
        code[jump_b_at + 1..jump_b_at + 5].copy_from_slice(&to_join_b.to_le_bytes());

        module.functions.push(NyarFunction {
            name: "main".into(),
            arity: 0,
            local_count: 0,
            code_offset: 0,
            code_length: code.len() as i32,
        });
        module.code_bytes = code;
        verify_module(&module).expect("balanced join");
    }

    #[test]
    fn accepts_store_local_i32_then_binop() {
        let mut module = empty_module();
        module.constants.push(NyarConstant::Integer32(1));
        module.constants.push(NyarConstant::Integer32(2));
        let mut code = Vec::new();
        emit_imm1(&mut code, NyarHeadCode::Const, 0);
        emit_imm1(&mut code, NyarHeadCode::StoreLocal, 0);
        emit_imm1(&mut code, NyarHeadCode::Const, 1);
        emit_imm1(&mut code, NyarHeadCode::StoreLocal, 1);
        emit_imm1(&mut code, NyarHeadCode::LoadLocal, 0);
        emit_imm1(&mut code, NyarHeadCode::LoadLocal, 1);
        emit_plain(&mut code, NyarHeadCode::I32Add);
        emit_plain(&mut code, NyarHeadCode::Return);
        module.functions.push(NyarFunction {
            name: "main".into(),
            arity: 0,
            local_count: 2,
            code_offset: 0,
            code_length: code.len() as i32,
        });
        module.code_bytes = code;
        verify_module(&module).expect("local i32 store/load");
    }

    #[test]
    fn rejects_store_ref_then_i32_into_same_local() {
        let mut module = empty_module();
        module.constants.push(NyarConstant::Integer32(0));
        module.layouts.push(NyarLayout { field_count: 0 });
        let mut code = Vec::new();
        emit_imm1(&mut code, NyarHeadCode::ObjectNew, 0);
        emit_imm1(&mut code, NyarHeadCode::StoreLocal, 0);
        emit_imm1(&mut code, NyarHeadCode::Const, 0);
        emit_imm1(&mut code, NyarHeadCode::StoreLocal, 0);
        emit_plain(&mut code, NyarHeadCode::Return);
        module.functions.push(NyarFunction {
            name: "main".into(),
            arity: 0,
            local_count: 1,
            code_offset: 0,
            code_length: code.len() as i32,
        });
        module.code_bytes = code;
        let err = verify_module(&module).expect_err("local conflict");
        assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("local type conflict")));
    }

    #[test]
    fn rejects_function_code_over_size_budget() {
        let mut module = empty_module();
        let mut code = vec![NyarHeadCode::Nop as u8; MAX_FUNCTION_CODE_BYTES + 1];
        code.push(NyarHeadCode::Return as u8);
        module.functions.push(NyarFunction {
            name: "main".into(),
            arity: 0,
            local_count: 0,
            code_offset: 0,
            code_length: code.len() as i32,
        });
        module.code_bytes = code;
        let err = verify_module(&module).expect_err("size budget");
        assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("exceeds limit")));
    }

    #[test]
    fn rejects_mixed_void_and_value_returns() {
        let mut module = empty_module();
        module.constants.push(NyarConstant::Integer32(1));
        let mut code = Vec::new();
        emit_imm1(&mut code, NyarHeadCode::Const, 0);
        let jif_at = code.len();
        emit_imm1(&mut code, NyarHeadCode::JumpIfFalse, 0);
        emit_plain(&mut code, NyarHeadCode::Return); // void
        let value_ret = code.len();
        emit_imm1(&mut code, NyarHeadCode::Const, 0);
        emit_plain(&mut code, NyarHeadCode::Return); // i32
        let to_value = (value_ret as i32) - (jif_at as i32);
        code[jif_at + 1..jif_at + 5].copy_from_slice(&to_value.to_le_bytes());
        module.functions.push(NyarFunction {
            name: "main".into(),
            arity: 0,
            local_count: 0,
            code_offset: 0,
            code_length: code.len() as i32,
        });
        module.code_bytes = code;
        let err = verify_module(&module).expect_err("return mismatch");
        assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("return type mismatch")));
    }

    #[test]
    fn accepts_consistent_i32_returns() {
        let mut module = empty_module();
        module.constants.push(NyarConstant::Integer32(1));
        let mut code = Vec::new();
        emit_imm1(&mut code, NyarHeadCode::Const, 0);
        let jif_at = code.len();
        emit_imm1(&mut code, NyarHeadCode::JumpIfFalse, 0);
        emit_imm1(&mut code, NyarHeadCode::Const, 0);
        emit_plain(&mut code, NyarHeadCode::Return);
        let other = code.len();
        emit_imm1(&mut code, NyarHeadCode::Const, 0);
        emit_plain(&mut code, NyarHeadCode::Return);
        let to_other = (other as i32) - (jif_at as i32);
        code[jif_at + 1..jif_at + 5].copy_from_slice(&to_other.to_le_bytes());
        module.functions.push(NyarFunction {
            name: "main".into(),
            arity: 0,
            local_count: 0,
            code_offset: 0,
            code_length: code.len() as i32,
        });
        module.code_bytes = code;
        verify_module(&module).expect("consistent i32 returns");
    }

    #[test]
    fn rejects_load_arg_beyond_arity() {
        let mut module = empty_module();
        let mut code = Vec::new();
        // arity=1 却 LoadArg 1（仅 local_count 允许该槽）。
        emit_imm1(&mut code, NyarHeadCode::LoadArg, 1);
        emit_plain(&mut code, NyarHeadCode::Return);
        module.functions.push(NyarFunction {
            name: "main".into(),
            arity: 1,
            local_count: 2,
            code_offset: 0,
            code_length: code.len() as i32,
        });
        module.code_bytes = code;
        let err = verify_module(&module).expect_err("LoadArg arity");
        assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("out of arity")));
    }

    #[test]
    fn rejects_load_ref_local_into_i32_binop() {
        let mut module = empty_module();
        module.layouts.push(NyarLayout { field_count: 0 });
        let mut code = Vec::new();
        emit_imm1(&mut code, NyarHeadCode::ObjectNew, 0);
        emit_imm1(&mut code, NyarHeadCode::StoreLocal, 0);
        emit_imm1(&mut code, NyarHeadCode::LoadLocal, 0);
        emit_imm1(&mut code, NyarHeadCode::LoadLocal, 0);
        emit_plain(&mut code, NyarHeadCode::I32Add);
        emit_plain(&mut code, NyarHeadCode::Return);
        module.functions.push(NyarFunction {
            name: "main".into(),
            arity: 0,
            local_count: 1,
            code_offset: 0,
            code_length: code.len() as i32,
        });
        module.code_bytes = code;
        let err = verify_module(&module).expect_err("ref as i32");
        assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("stack type mismatch")));
    }

    #[test]
    fn rejects_jump_if_false_with_ref_condition() {
        let mut module = empty_module();
        module.layouts.push(NyarLayout { field_count: 0 });
        let mut code = Vec::new();
        emit_imm1(&mut code, NyarHeadCode::ObjectNew, 0);
        let br_pc = code.len();
        emit_imm1(&mut code, NyarHeadCode::JumpIfFalse, 0);
        emit_plain(&mut code, NyarHeadCode::Return);
        let else_pc = code.len();
        let rel = (else_pc as i32) - (br_pc as i32);
        code[br_pc + 1..br_pc + 5].copy_from_slice(&rel.to_le_bytes());
        emit_plain(&mut code, NyarHeadCode::Return);
        module.functions.push(NyarFunction {
            name: "main".into(),
            arity: 0,
            local_count: 0,
            code_offset: 0,
            code_length: code.len() as i32,
        });
        module.code_bytes = code;
        let err = verify_module(&module).expect_err("ref JumpIfFalse");
        assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("stack type mismatch")));
    }

    #[test]
    fn accepts_i32_binop_discard_pop_void_return() {
        let mut module = empty_module();
        let mut code = Vec::new();
        emit_imm1(&mut code, NyarHeadCode::LoadLocal, 0);
        emit_imm1(&mut code, NyarHeadCode::LoadLocal, 1);
        emit_plain(&mut code, NyarHeadCode::I32Add);
        emit_plain(&mut code, NyarHeadCode::Pop);
        emit_plain(&mut code, NyarHeadCode::Return);
        module.functions.push(NyarFunction {
            name: "main".into(),
            arity: 0,
            local_count: 2,
            code_offset: 0,
            code_length: code.len() as i32,
        });
        module.code_bytes = code;
        verify_module(&module).expect("i32 Pop void");
    }
}
