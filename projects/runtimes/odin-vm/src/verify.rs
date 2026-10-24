//! 加载期模块校验：结构边界、函数控制流、栈高度/粗类型与局部槽粗类型汇合、栈深上限，以及导入 / layout 下标。

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use odin_bytecode::{NYAR_VERSION, NyarHeadCode, NyarImport, NyarInstruction, NyarModuleData, OBSOLETE_CALL_NATIVE, decode_at};

use crate::{error::NyarRuntimeError, host::resolve_import};

/// 单函数操作数栈高度上限（加载期拒绝病理模块）。
pub const MAX_OPERAND_STACK_HEIGHT: i32 = 8192;

/// 单函数外码字节上限（体积预算；超限 fail-closed）。
pub const MAX_FUNCTION_CODE_BYTES: usize = 64 * 1024;

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

fn merge_kind_vecs(left: &[StackKind], right: &[StackKind], merge: fn(StackKind, StackKind) -> Option<StackKind>) -> Option<Vec<StackKind>> {
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

/// `Yield` / `Resume` 路径在操作数栈上的协程元数据。
///
/// - `operand_base`：`stack_base` 之上、yielded 值之下仍保留的操作数片段深度。
/// - `unconsumed_resume`：resume 注入值是否仍占一个槽（尚未被后续指令消费）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CoroutineMeta {
    operand_base: i32,
    unconsumed_resume: bool,
}

impl CoroutineMeta {
    const ENTRY: Self = Self { operand_base: 0, unconsumed_resume: false };

    fn after_yield(height: i32) -> Self {
        Self { operand_base: height.saturating_sub(1), unconsumed_resume: true }
    }

    fn pop_slot(self, height: i32) -> (Self, i32) {
        let new_height = height - 1;
        let mut meta = self;
        if meta.unconsumed_resume && new_height == meta.operand_base {
            meta.unconsumed_resume = false;
        }
        else if new_height < meta.operand_base {
            meta.operand_base -= 1;
        }
        (meta, new_height)
    }

    fn allows_return(self, height: i32) -> bool {
        let above_preserved = height - self.operand_base;
        if self.unconsumed_resume { above_preserved <= 2 } else { above_preserved <= 1 }
    }
}

type FlowEdge = (usize, i32, CoroutineMeta);

fn flow_edge(target: usize, height: i32, coroutine: CoroutineMeta) -> FlowEdge {
    (target, height, coroutine)
}

fn edge(target: usize, height: i32, coroutine: CoroutineMeta) -> FlowEdge {
    flow_edge(target, height, coroutine)
}

/// 校验已解码模块：版本、导入白名单、下标、函数代码区间、跳转边界、栈高度与栈深上限。
pub fn verify_module(data: &NyarModuleData) -> Result<(), NyarRuntimeError> {
    if data.version != NYAR_VERSION {
        return Err(NyarRuntimeError::ModuleLoad(format!("unsupported module version {}; expected {NYAR_VERSION}", data.version)));
    }

    if data.code_bytes.len() > MAX_MODULE_CODE_BYTES {
        return Err(NyarRuntimeError::ModuleLoad(format!(
            "module code section length {} exceeds limit {MAX_MODULE_CODE_BYTES}",
            data.code_bytes.len()
        )));
    }

    for (index, layout) in data.layouts.iter().enumerate() {
        if layout.field_count < 0 {
            return Err(NyarRuntimeError::ModuleLoad(format!("layout[{index}] has negative field_count {}", layout.field_count)));
        }
    }

    for (index, import) in data.imports.iter().enumerate() {
        verify_import(index, import)?;
    }

    let mut export_names = BTreeSet::new();
    for export in &data.exports {
        if export.symbol_name.is_empty() || !export_names.insert(&export.symbol_name) {
            return Err(NyarRuntimeError::ModuleLoad("empty or duplicate export identity".to_owned()));
        }
        let count = match export.kind {
            odin_bytecode::NyarExportKind::Function => data.functions.len(),
            odin_bytecode::NyarExportKind::Global => data.globals.len(),
        };
        if export.function_index < 0 || export.function_index as usize >= count {
            return Err(NyarRuntimeError::ModuleLoad(format!("export `{}` index out of range", export.symbol_name)));
        }
    }
    for (index, function) in data.functions.iter().enumerate() {
        if function.arity < 0 || function.local_count < 0 {
            return Err(NyarRuntimeError::ModuleLoad(format!("function[{index}] has negative arity or local count")));
        }
    }
    for &index in &data.init_function_indices {
        let function =
            usize::try_from(index).ok().and_then(|index| data.functions.get(index)).ok_or(NyarRuntimeError::FunctionIndexOutOfRange(index))?;
        if function.arity != 0 {
            return Err(NyarRuntimeError::ModuleLoad(format!("init function[{index}] must have zero arguments")));
        }
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

fn verify_function(data: &NyarModuleData, function_index: usize, function: &odin_bytecode::NyarFunction) -> Result<(), NyarRuntimeError> {
    if function.code_offset < 0 || function.code_length < 0 {
        return Err(NyarRuntimeError::ModuleLoad(format!("function[{function_index}] has negative code_offset or code_length")));
    }

    let start = function.code_offset as usize;
    let length = function.code_length as usize;
    let end =
        start.checked_add(length).ok_or_else(|| NyarRuntimeError::ModuleLoad(format!("function[{function_index}] code range overflows")))?;
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
    let entry_height = if data.witness_entries.iter().any(|entry| entry.function_index == function_index as i32) { 2 } else { 0 };

    let entry_types = vec![StackKind::Any; entry_height.max(0) as usize];
    // 入口局部均为 `Any`（帧以 `Null` 填充；参数类型由调用约定另行约束）。
    let entry_locals = vec![StackKind::Any; local_slots];
    let mut heights: BTreeMap<usize, i32> = BTreeMap::new();
    let mut type_stacks: BTreeMap<usize, Vec<StackKind>> = BTreeMap::new();
    let mut local_kinds: BTreeMap<usize, Vec<StackKind>> = BTreeMap::new();
    let mut coroutine_metas: BTreeMap<usize, CoroutineMeta> = BTreeMap::new();
    let mut return_shape: Option<ReturnShape> = None;
    let mut queue = VecDeque::new();
    heights.insert(start, entry_height);
    type_stacks.insert(start, entry_types);
    local_kinds.insert(start, entry_locals);
    coroutine_metas.insert(start, CoroutineMeta::ENTRY);
    queue.push_back(start);

    while let Some(pc) = queue.pop_front() {
        let height = heights[&pc];
        let mut coroutine = coroutine_metas[&pc];
        if height > MAX_OPERAND_STACK_HEIGHT {
            return Err(NyarRuntimeError::ModuleLoad(format!(
                "function[{function_index}] operand stack height {height} exceeds limit {MAX_OPERAND_STACK_HEIGHT} at pc {pc}"
            )));
        }
        let mut types = type_stacks[&pc].clone();
        let mut locals = local_kinds[&pc].clone();
        let instruction = instructions
            .get(&pc)
            .copied()
            .ok_or_else(|| NyarRuntimeError::ModuleLoad(format!("function[{function_index}] control reaches non-instruction pc {pc}")))?;

        let arity = function.arity.max(0) as usize;
        verify_instruction_operands(data, function_index, pc, instruction, local_slots, arity)?;

        if instruction.code == NyarHeadCode::Return {
            if !coroutine.allows_return(height) {
                return Err(NyarRuntimeError::ModuleLoad(format!(
                    "function[{function_index}] return at pc {pc} leaves {height} values on stack; at most one allowed"
                )));
            }
            let above_preserved = height - coroutine.operand_base;
            let shape = if above_preserved <= 0 || (above_preserved == 1 && coroutine.unconsumed_resume) {
                ReturnShape::Void
            }
            else {
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

        let edges = stack_transfer(data, function_index, pc, instruction, height, &mut types, &mut locals, &mut coroutine)?;
        for (target, edge_height, edge_coroutine) in edges {
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
                return Err(NyarRuntimeError::ModuleLoad(format!("function[{function_index}] internal type-stack length mismatch at pc {pc}")));
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
                    let Some(merged) = merge_type_stacks(existing, &types)
                    else {
                        return Err(NyarRuntimeError::ModuleLoad(format!("function[{function_index}] stack type mismatch at pc {target}")));
                    };
                    if merged != *existing {
                        type_stacks.insert(target, merged);
                        true
                    }
                    else {
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
                    let Some(merged) = merge_local_kinds(existing, &locals)
                    else {
                        return Err(NyarRuntimeError::ModuleLoad(format!("function[{function_index}] local type mismatch at pc {target}")));
                    };
                    if merged != *existing {
                        local_kinds.insert(target, merged);
                        true
                    }
                    else {
                        false
                    }
                }
                None => {
                    local_kinds.insert(target, locals.clone());
                    true
                }
            };
            let coroutine_changed = match coroutine_metas.get(&target) {
                Some(existing) if *existing == edge_coroutine => false,
                Some(_) => {
                    return Err(NyarRuntimeError::ModuleLoad(format!(
                        "function[{function_index}] coroutine stack metadata mismatch at pc {target}"
                    )));
                }
                None => {
                    coroutine_metas.insert(target, edge_coroutine);
                    true
                }
            };
            if stack_changed || locals_changed || coroutine_changed {
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
            return Err(NyarRuntimeError::ModuleLoad(format!("function[{function_index}] truncated instruction at pc {pc}")));
        }
        let next = pc
            .checked_add(instruction.size as usize)
            .ok_or_else(|| NyarRuntimeError::ModuleLoad(format!("function[{function_index}] instruction size overflows at pc {pc}")))?;
        if next > end {
            return Err(NyarRuntimeError::ModuleLoad(format!("function[{function_index}] instruction at pc {pc} crosses function end {end}")));
        }
        instructions.insert(pc, instruction);
        pc = next;
    }
    if pc != end {
        return Err(NyarRuntimeError::ModuleLoad(format!("function[{function_index}] code range leaves a gap ending at {pc}, expected {end}")));
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
        NyarHeadCode::Constant => {
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
        Err(NyarRuntimeError::ModuleLoad(format!("function[{function_index}] stack underflow at pc {pc}: height {height}, need {needed}")))
    }
    else {
        Ok(())
    }
}

fn require_kind(function_index: usize, pc: usize, kinds: &mut Vec<StackKind>, expected: StackKind) -> Result<(), NyarRuntimeError> {
    let Some(actual) = kinds.pop()
    else {
        return Err(NyarRuntimeError::ModuleLoad(format!("function[{function_index}] stack underflow at pc {pc}")));
    };
    match (actual, expected) {
        (_, StackKind::Any) | (StackKind::Any, _) => Ok(()),
        (a, e) if a == e => Ok(()),
        _ => Err(NyarRuntimeError::ModuleLoad(format!(
            "function[{function_index}] stack type mismatch at pc {pc}: expected {expected:?}, got {actual:?}"
        ))),
    }
}

fn pop_typed_stack(
    function_index: usize,
    pc: usize,
    types: &mut Vec<StackKind>,
    expected: StackKind,
    height: i32,
    coroutine: &mut CoroutineMeta,
) -> Result<i32, NyarRuntimeError> {
    require_kind(function_index, pc, types, expected)?;
    let (next, new_height) = coroutine.pop_slot(height);
    *coroutine = next;
    Ok(new_height)
}

fn pop_any_stack(
    function_index: usize,
    pc: usize,
    types: &mut Vec<StackKind>,
    height: i32,
    coroutine: &mut CoroutineMeta,
) -> Result<i32, NyarRuntimeError> {
    pop_typed_stack(function_index, pc, types, StackKind::Any, height, coroutine)
}

fn push_const_kind(data: &NyarModuleData, instruction: NyarInstruction) -> StackKind {
    if instruction.operand1 >= 0 {
        match data.constants.get(instruction.operand1 as usize) {
            Some(odin_bytecode::NyarConstant::Integer32(_)) | Some(odin_bytecode::NyarConstant::Boolean(_)) => {
                return StackKind::I32;
            }
            _ => {}
        }
    }
    StackKind::Any
}

/// 就地更新 `types` / `locals` / 协程元数据，返回后继边 `(目标 pc, 到达时栈高度, 协程元数据)`。
fn stack_transfer(
    data: &NyarModuleData,
    function_index: usize,
    pc: usize,
    instruction: NyarInstruction,
    height: i32,
    types: &mut Vec<StackKind>,
    locals: &mut Vec<StackKind>,
    coroutine: &mut CoroutineMeta,
) -> Result<Vec<(usize, i32, CoroutineMeta)>, NyarRuntimeError> {
    let fallthrough = pc + instruction.size as usize;

    match instruction.code {
        NyarHeadCode::Nop => Ok(vec![flow_edge(fallthrough, height, *coroutine)]),
        NyarHeadCode::Jump => {
            let target = pc.wrapping_add(instruction.operand1 as usize);
            Ok(vec![flow_edge(target, height, *coroutine)])
        }
        NyarHeadCode::JumpIfTrue | NyarHeadCode::JumpIfFalse => {
            require_height(function_index, pc, height, 1)?;
            // 条件槽必须是 i32（0/非 0）；拒绝 Ref/Any 冒充布尔条件。
            let after = pop_typed_stack(function_index, pc, types, StackKind::I32, height, coroutine)?;
            let target = pc.wrapping_add(instruction.operand1 as usize);
            Ok(vec![flow_edge(fallthrough, after, *coroutine), flow_edge(target, after, *coroutine)])
        }
        NyarHeadCode::Return => {
            // Return 结束本帧。允许空栈（void / init）；有值则留给调用方。
            Ok(Vec::new())
        }
        NyarHeadCode::Yield | NyarHeadCode::PerformEffect => {
            require_height(function_index, pc, height, 1)?;
            require_kind(function_index, pc, types, StackKind::Any)?;
            types.push(StackKind::Any);
            let resume_meta = CoroutineMeta::after_yield(height);
            Ok(vec![(fallthrough, height, resume_meta)])
        }
        NyarHeadCode::Resume => {
            require_height(function_index, pc, height, 2)?;
            require_kind(function_index, pc, types, StackKind::Any)?;
            require_kind(function_index, pc, types, StackKind::Any)?;
            types.push(StackKind::Any);
            Ok(vec![edge(fallthrough, height - 1, *coroutine)])
        }
        NyarHeadCode::Constant => {
            types.push(push_const_kind(data, instruction));
            Ok(vec![edge(fallthrough, height + 1, *coroutine)])
        }
        NyarHeadCode::ObjectNew => {
            types.push(StackKind::Ref);
            Ok(vec![edge(fallthrough, height + 1, *coroutine)])
        }
        NyarHeadCode::LoadLocal | NyarHeadCode::LoadArg => {
            let slot = instruction.operand1 as usize;
            let kind = locals.get(slot).copied().unwrap_or(StackKind::Any);
            types.push(kind);
            Ok(vec![edge(fallthrough, height + 1, *coroutine)])
        }
        NyarHeadCode::LoadGlobal => {
            types.push(StackKind::Any);
            Ok(vec![edge(fallthrough, height + 1, *coroutine)])
        }
        NyarHeadCode::StoreLocal => {
            require_height(function_index, pc, height, 1)?;
            let Some(stored) = types.pop()
            else {
                return Err(NyarRuntimeError::ModuleLoad(format!("function[{function_index}] stack underflow at pc {pc}")));
            };
            let (next_meta, after) = coroutine.pop_slot(height);
            *coroutine = next_meta;
            let slot = instruction.operand1 as usize;
            let old = locals.get(slot).copied().unwrap_or(StackKind::Any);
            let Some(next) = assign_local_kind(old, stored)
            else {
                return Err(NyarRuntimeError::ModuleLoad(format!(
                    "function[{function_index}] local type conflict at pc {pc}: slot {slot} was {old:?}, store {stored:?}"
                )));
            };
            if slot < locals.len() {
                locals[slot] = next;
            }
            Ok(vec![edge(fallthrough, after, *coroutine)])
        }
        NyarHeadCode::Pop | NyarHeadCode::StoreGlobal => {
            require_height(function_index, pc, height, 1)?;
            let after = pop_any_stack(function_index, pc, types, height, coroutine)?;
            Ok(vec![edge(fallthrough, after, *coroutine)])
        }
        NyarHeadCode::Dup => {
            require_height(function_index, pc, height, 1)?;
            let top =
                *types.last().ok_or_else(|| NyarRuntimeError::ModuleLoad(format!("function[{function_index}] stack underflow at pc {pc}")))?;
            types.push(top);
            Ok(vec![edge(fallthrough, height + 1, *coroutine)])
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
            let after_rhs = pop_typed_stack(function_index, pc, types, StackKind::I32, height, coroutine)?;
            let after_lhs = pop_typed_stack(function_index, pc, types, StackKind::I32, after_rhs, coroutine)?;
            types.push(StackKind::I32);
            Ok(vec![edge(fallthrough, after_lhs + 1, *coroutine)])
        }
        NyarHeadCode::FieldGet => {
            require_height(function_index, pc, height, 1)?;
            let after = pop_typed_stack(function_index, pc, types, StackKind::Ref, height, coroutine)?;
            types.push(StackKind::Any);
            Ok(vec![edge(fallthrough, after + 1, *coroutine)])
        }
        NyarHeadCode::FieldSet => {
            require_height(function_index, pc, height, 2)?;
            let after_value = pop_any_stack(function_index, pc, types, height, coroutine)?;
            let after_ref = pop_typed_stack(function_index, pc, types, StackKind::Ref, after_value, coroutine)?;
            types.push(StackKind::Ref);
            Ok(vec![edge(fallthrough, after_ref + 1, *coroutine)])
        }
        NyarHeadCode::Call | NyarHeadCode::CallStatic => {
            let callee = &data.functions[instruction.operand1 as usize];
            let arity = callee.arity.max(0);
            require_height(function_index, pc, height, arity)?;
            let mut after = height;
            for _ in 0..arity {
                after = pop_any_stack(function_index, pc, types, after, coroutine)?;
            }
            types.push(StackKind::Any);
            Ok(vec![edge(fallthrough, after + 1, *coroutine)])
        }
        NyarHeadCode::CallImport | NyarHeadCode::CallIntrinsic => {
            let argc = instruction.operand2.max(0);
            require_height(function_index, pc, height, argc)?;
            let mut after = height;
            for _ in 0..argc {
                after = pop_any_stack(function_index, pc, types, after, coroutine)?;
            }
            types.push(StackKind::Any);
            Ok(vec![edge(fallthrough, after + 1, *coroutine)])
        }
    }
}
