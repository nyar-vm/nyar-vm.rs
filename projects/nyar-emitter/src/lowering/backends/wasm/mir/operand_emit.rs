//! Split from former monolithic wasm mir lowerer (ADR 0008).
#![allow(deprecated)]

#[allow(deprecated)]
use super::*;

#[allow(deprecated)]
impl<'a> WasmMirLowerer<'a> {
    pub(crate) fn wasm_local_is_typed_ref(&self, local_index: u32) -> bool {
        if local_index < self.stack_ptr_local {
            return false;
        }
        matches!(
            self.local_valtypes.get((local_index - self.stack_ptr_local) as usize),
            Some(values) if values.first().copied() == Some(VALTYPE_REF)
        )
    }

    /// 发射 f64 操作数：对有 local ?Value，先 local.get ?promote ?f64（若 local ?i32）?
    /// 对无 local ?Value/Constant 发射 f64.const 0 占位?
    pub(crate) fn emit_f64_operand(&mut self, operand: &MirOperand) {
        match operand {
            MirOperand::Value(value) => {
                if let Some(local) = self.value_locals.get(value).copied() {
                    self.emit_local_get(local);
                    // 所?local 当前都是 i32 类型（见 alloc_i32_local）?
                    // f64 运算前需?promote：i32→f64 ?f64.convert (0xB7)?
                    if self.wasm_local_value_type(local) != VALTYPE_F64 {
                        WasmOpcode::F64ConvertI32S.encode(&mut self.code);
                    }
                }
                else if let Some(local) = self.scalar_locals.get(value).copied() {
                    self.emit_local_get(local);
                    if self.wasm_local_value_type(local) != VALTYPE_F64 {
                        WasmOpcode::F64ConvertI32S.encode(&mut self.code);
                    }
                }
                else {
                    panic!(
                        "WASM emit fail-closed: missing f64 operand producer in `{}`; refuse f64.const 0 (ADR 0008)",
                        self.mir_fn.symbol
                    );
                }
            }
            MirOperand::Constant(constant) => match constant {
                MirConstant::Float64(value) => self.emit_f64_const(value.into_inner()),
                MirConstant::Int(value) => self.emit_f64_const(*value as f64),
                MirConstant::Bool(value) => self.emit_f64_const(if *value { 1.0 } else { 0.0 }),
                MirConstant::Utf8(_) | MirConstant::Unit => panic!(
                    "WASM emit fail-closed: Utf8/Unit cannot be f64 operand in `{}` (ADR 0008)",
                    self.mir_fn.symbol
                ),
                MirConstant::Utf16(_) => panic!("WASM lowering requires an explicit UTF-16 ABI contract"),
            },
            MirOperand::Symbol(path) => {
                if let Some(local) = self.var_locals.get(&path.to_string()).copied() {
                    self.emit_local_get(local);
                    if self.wasm_local_value_type(local) != VALTYPE_F64 {
                        WasmOpcode::F64ConvertI32S.encode(&mut self.code);
                    }
                }
                else {
                    panic!(
                        "WASM emit fail-closed: unresolved Symbol `{}` as f64 operand in `{}` (ADR 0008)",
                        path, self.mir_fn.symbol
                    );
                }
            }
        }
    }

    /// 发射 i32 操作数：对有 local 的 Value，直接 local.get。
    pub(crate) fn emit_i32_operand(&mut self, operand: &MirOperand) {
        match operand {
            MirOperand::Value(value) => {
                // 优先 reference_locals；value/scalar 槽也可能误挂 anyref 入口参数?
                if let Some(local) = self.reference_locals.get(value).copied() {
                    self.emit_anyref_local_as_i32_bool(local);
                }
                else if let Some(local) = self.value_locals.get(value).copied().or_else(|| self.scalar_locals.get(value).copied()) {
                    if self.wasm_local_value_type(local) == WASM_GC_ANYREF || self.wasm_local_value_type(local) == WASM_GC_EXTERNREF {
                        self.emit_anyref_local_as_i32_bool(local);
                    }
                    else {
                        self.emit_local_get(local);
                        // i64/f64 槽进?i32 原语（eq/add/…）前必须截断，否则 `expected i32, found i64`?
                        let ty = self.wasm_local_value_type(local);
                        if ty == VALTYPE_I64 {
                            WasmOpcode::I32WrapI64.encode(&mut self.code);
                        }
                        else if ty == VALTYPE_F64 {
                            WasmOpcode::I32TruncF64S.encode(&mut self.code);
                        }
                    }
                }
                else {
                    panic!(
                        "WASM emit fail-closed: missing i32 operand producer for %{} in `{}` (ADR 0008)",
                        value.0, self.mir_fn.symbol
                    );
                }
            }
            MirOperand::Constant(constant) => self.emit_load_constant(constant),
            MirOperand::Symbol(path) => {
                if let Some(local) = self.var_locals.get(&path.to_string()).copied() {
                    if self.wasm_local_value_type(local) == WASM_GC_ANYREF || self.wasm_local_value_type(local) == WASM_GC_EXTERNREF {
                        self.emit_anyref_local_as_i32_bool(local);
                    }
                    else {
                        self.emit_local_get(local);
                        let ty = self.wasm_local_value_type(local);
                        if ty == VALTYPE_I64 {
                            WasmOpcode::I32WrapI64.encode(&mut self.code);
                        }
                        else if ty == VALTYPE_F64 {
                            WasmOpcode::I32TruncF64S.encode(&mut self.code);
                        }
                    }
                }
                else {
                    panic!(
                        "WASM emit fail-closed: unresolved Symbol `{}` as i32 operand in `{}` (ADR 0008)",
                        path, self.mir_fn.symbol
                    );
                }
            }
        }
    }

    /// 引用 local →?i32 布尔：`ref.is_null` ?`i32.eqz`?=非空, 0=null）?
    pub(crate) fn emit_anyref_local_as_i32_bool(&mut self, local: u32) {
        self.emit_local_get(local);
        WasmOpcode::RefIsNull.encode(&mut self.code);
        WasmOpcode::I32Eqz.encode(&mut self.code);
    }

    pub(crate) fn emit_operand(&mut self, operand: &MirOperand) {
        match operand {
            MirOperand::Value(value) => {
                if let Some(local) = self.reference_locals.get(value).copied() {
                    self.emit_local_get(local);
                }
                else if let Some(local) = self.value_locals.get(value).copied() {
                    self.emit_local_get(local);
                }
                else if let Some(local) = self.scalar_locals.get(value).copied() {
                    self.emit_local_get(local);
                }
                else {
                    panic!(
                        "WASM emit fail-closed: missing producer local for MirValue {:?} in `{}`; refuse typed placeholder (ADR 0008)",
                        value, self.mir_fn.symbol
                    );
                }
            }
            MirOperand::Constant(constant) => self.emit_load_constant(constant),
            MirOperand::Symbol(path) => {
                if let Some(local) = self.var_locals.get(&path.to_string()).copied() {
                    self.emit_local_get(local);
                }
                else {
                    panic!(
                        "WASM emit fail-closed: unresolved Symbol `{}` in `{}`; refuse ref.null placeholder (ADR 0008)",
                        path, self.mir_fn.symbol
                    );
                }
            }
        }
    }

    /// Removed from formal emit path: inventing 0/null for missing locals
    /// is forbidden upward inference (ADR 0008). Kept only as documentation
    /// of the old anti-pattern; callers must panic instead.
    #[allow(dead_code)]
    pub(crate) fn emit_typed_placeholder_for_value(&mut self, value: &MirValueRef) {
        panic!(
            "WASM emit fail-closed: emit_typed_placeholder_for_value called for {:?} in `{}` (ADR 0008)",
            value, self.mir_fn.symbol
        );
    }

    /// 发射 `MirConstant` 对应的栈值?
    ///
    /// 标量常量（Int/Bool/Float64）发射对?i32/f64 指令?
    /// 引用常量（String/Unit）发?`ref.null anyref` 占位?
    ///
    /// String/Unit ?`storage_for_type` 中判?`Reference`?
    /// `output_storage_kind` 据此?output 分配?`reference_locals`（anyref）?
    /// 若此处仍发射 `i32.const 0`，随后的 `store_scalar` 会执?
    /// `local.set expected anyref, found i32`，导?v1→v2 自举阻断?
    /// `ref.null anyref` (0xD0 VALTYPE_ANYREF) 产生 null 引用，语义上也是 String/Unit
    /// 占位值的正确表达。`emit_operand` 中的 `MirOperand::Constant` 路径
    /// 同样受益：引用类型操作数?null anyref，与接收方期望的 anyref 类型匹配?
    /// 查询 output 标量槽的真实 wasm valtype（无槽时回退 value_types / i32）?
    pub(crate) fn output_scalar_slot_type(&self, output: MirValueRef) -> u8 {
        if let Some(local) = self.value_locals.get(&output).copied().or_else(|| self.scalar_locals.get(&output).copied()) {
            return self.wasm_local_value_type(local);
        }
        self.mir_fn
            .value_types
            .get(&output)
            .map(|ty| wasm_param_value_type_for(self.ctx, ty, self.gc_struct_type_indices, self.js_glue_utf8_as_anyref))
            .unwrap_or_else(|| {
                panic!(
                    "WASM emit fail-closed: missing value_types for scalar slot %{} in `{}`; refuse I32 default (ADR 0008)",
                    output.0, self.mir_fn.symbol
                )
            })
    }

    /// 按目标槽 valtype 发射常量（Int→i64/i32/f64；Unit→anyref；String 仍按 wasi/js 路径）?
    pub(crate) fn emit_load_constant_for_slot(&mut self, constant: &MirConstant, slot_ty: u8) {
        match constant {
            MirConstant::Int(value) => match slot_ty {
                VALTYPE_I64 => self.emit_i64_const(*value),
                VALTYPE_F64 => self.emit_f64_const(*value as f64),
                _ => self.emit_i32_const(*value as i32),
            },
            MirConstant::Bool(value) => match slot_ty {
                VALTYPE_I64 => self.emit_i64_const(if *value { 1 } else { 0 }),
                VALTYPE_F64 => self.emit_f64_const(if *value { 1.0 } else { 0.0 }),
                _ => self.emit_i32_const(if *value { 1 } else { 0 }),
            },
            MirConstant::Float64(value) => match slot_ty {
                VALTYPE_I64 => self.emit_i64_const(value.into_inner() as i64),
                VALTYPE_I32 => self.emit_i32_const(value.into_inner() as i32),
                _ => self.emit_f64_const(value.into_inner()),
            },
            MirConstant::Utf8(_) | MirConstant::Unit => self.emit_load_constant(constant),
            MirConstant::Utf16(_) => panic!("WASM lowering requires an explicit UTF-16 ABI contract"),
        }
    }

    pub(crate) fn emit_load_constant(&mut self, constant: &MirConstant) {
        match constant {
            MirConstant::Int(value) => self.emit_i32_const(*value as i32),
            MirConstant::Float64(value) => self.emit_f64_const(value.into_inner()),
            MirConstant::Bool(value) => self.emit_i32_const(if *value { 1 } else { 0 }),
            MirConstant::Utf8(text) => {
                // WASI 轨：字符串字面量作为线性内存偏移量（i32）传递，
                // 字符串内容已通过 data 段在模块初始化时写入线性内存?
                // Canonical ABI 的「字符串」对?**utf8 字节序列**（非语言?string）?
                if self.wasi_mode {
                    if let Some(&offset) = self.string_literal_offset.get(text) {
                        self.emit_i32_const(offset as i32);
                        return;
                    }
                    self.emit_i32_const(0);
                    return;
                }
                if let Some(import_index) = self.const_utf8_import {
                    if let Some(&literal_index) = self.string_literal_index.get(text) {
                        self.emit_i32_const(literal_index as i32);
                        self.emit_call(import_index);
                        return;
                    }
                }
                self.emit_i32_const(0);
            }
            MirConstant::Utf16(_) => panic!("WASM lowering requires an explicit UTF-16 ABI contract"),
            // Unit ADT=1（恰有一个值）；用 ref.null any ?GC 占位，勿?void(ADT=0) 混淆?
            MirConstant::Unit => self.emit_ref_null_anyref(),
        }
    }

    /// 将栈顶值存?output 对应?local?
    ///
    /// 按与 `Call` 输出存储相同的优先级查找 local?
    /// `reference_locals`（anyref）→ `scalar_locals`（i32）→ `value_locals`（i32）?
    ///
    /// 旧实现仅检?`scalar_locals`，当 `plan_instruction` ?output 分配?
    /// `reference_locals`（引用类型）时找不到 local，导致栈顶值未存储?
    /// 或被其他指令错误消费。修复后 `Copy`/`Identity` 等调?`store_scalar` 的指?
    /// 能正确将 anyref 结果存入 anyref local，避?`local.set expected i32, found anyref`?
    pub(crate) fn store_scalar(&mut self, value: MirValueRef) {
        if let Some(local) = self.reference_locals.get(&value).copied() {
            self.emit_local_set(local);
        }
        else if let Some(local) = self.scalar_locals.get(&value).copied() {
            if self.wasm_local_value_type(local) == WASM_GC_ANYREF || self.wasm_local_value_type(local) == WASM_GC_EXTERNREF {
                self.scalar_locals.remove(&value);
                self.value_locals.remove(&value);
                self.reference_locals.insert(value, local);
                self.emit_local_set(local);
                return;
            }
            self.emit_local_set(local);
        }
        else if let Some(local) = self.value_locals.get(&value).copied() {
            // value_locals 可能误挂 anyref 入口参数槽：按真?valtype 分流?
            if self.wasm_local_value_type(local) == WASM_GC_ANYREF || self.wasm_local_value_type(local) == WASM_GC_EXTERNREF {
                self.value_locals.remove(&value);
                self.reference_locals.insert(value, local);
                self.emit_local_set(local);
                return;
            }
            self.emit_local_set(local);
        }
    }

    pub(crate) fn operand_address_local(&self, operand: &MirOperand) -> Option<u32> {
        match operand {
            MirOperand::Value(value) => self.value_locals.get(value).copied(),
            MirOperand::Symbol(path) => self.var_locals.get(&path.to_string()).copied(),
            _ => None,
        }
    }

    pub(crate) fn field_store_stack_type(&self, field: &FieldLayout) -> u8 {
        if self.storage_for_type(&field.ty) == StorageKind::Reference {
            // 值类型聚合在线性内存中不存?GC 引用,?i32 占位?
            VALTYPE_I32
        }
        else if self.storage_for_type(&field.ty) == StorageKind::Value {
            VALTYPE_I32
        }
        else {
            wasm_param_value_type(self.ctx, &field.ty, self.js_glue_utf8_as_anyref)
        }
    }

    /// wasm-gc `struct.set` 字段值栈类型（与 `register_gc_struct_types` / `wasm_gc_field_type_byte` 一致）?
    pub(crate) fn gc_struct_field_stack_type(&self, field: &FieldLayout) -> u8 {
        wasm_gc_field_type_byte_for_glue(&field.ty, self.js_glue_utf8_as_anyref)
    }

    /// Emits a store for a single field.
    ///
    /// For scalar fields, emits the matching `i32.store` / `i64.store` / `f64.store`.
    /// For nested value-type fields, emits `memory.copy` to copy the inline contents
    /// from the source address (already on the stack) into the parent's field region.
    /// Stack at entry: `[dest_addr, source_value_or_addr]`.
    pub(crate) fn emit_store_at_field(&mut self, field: &FieldLayout) {
        let is_value_type = self.storage_for_type(&field.ty) == StorageKind::Value;
        if is_value_type {
            // Stack: [dest_addr, source_addr] →?memory.copy expects [dest, source, len]
            self.emit_i32_const(field.size as i32);
            self.emit_memory_copy();
            return;
        }
        match field.ty {
            NyarType::Float64 => encode_f64_store(3, 0, &mut self.code),
            NyarType::Integer64 { .. } => encode_i64_store(3, 0, &mut self.code),
            _ => encode_i32_store(2, 0, &mut self.code),
        }
    }

    pub(crate) fn emit_load_at_field(&mut self, field: &FieldLayout) {
        match field.ty {
            NyarType::Float64 => encode_f64_load(3, 0, &mut self.code),
            NyarType::Integer64 { .. } => encode_i64_load(3, 0, &mut self.code),
            _ => encode_i32_load(2, 0, &mut self.code),
        }
    }

    pub(crate) fn infer_array_element_type(&self, operand: &MirOperand) -> Option<NyarType> {
        match operand {
            MirOperand::Value(value) => self.mir_fn.value_types.get(value).and_then(|ty| match ty {
                NyarType::Array(item) => Some(item.as_ref().clone()),
                NyarType::FixedArray { element, .. } => Some(element.as_ref().clone()),
                NyarType::Apply(base, args)
                    if matches!(base.as_ref(), NyarType::Named(name) if {
                        let text = name.as_str();
                        text == "Array" || text == "array" || text.ends_with("Array") || text == "List" || text == "list"
                    }) =>
                {
                    args.first().cloned()
                }
                ty if is_generic_array_element_type(ty) => Some(ty.clone()),
                _ => None,
            }),
            _ => None,
        }
    }

    /// ?`MirOperand` 解析引用语义?local index?
    /// ?`Value` 操作数查 `reference_locals`?
    /// ?`Symbol` 操作数查 `var_locals` 并校?local 类型?anyref?
    pub(crate) fn operand_reference_local(&self, operand: &MirOperand) -> Option<u32> {
        match operand {
            MirOperand::Value(value) => {
                if let Some(local) = self.reference_locals.get(value).copied() {
                    return Some(local);
                }
                if let Some(local) = self.value_locals.get(value).copied().or_else(|| self.scalar_locals.get(value).copied()) {
                    let ty = self.wasm_local_value_type(local);
                    if ty == WASM_GC_ANYREF || ty == WASM_GC_EXTERNREF {
                        return Some(local);
                    }
                }
                None
            }
            MirOperand::Symbol(path) => {
                let key = path.to_string();
                if let Some(&local) = self.var_locals.get(&key) {
                    if self.wasm_local_value_type(local) == WASM_GC_ANYREF {
                        return Some(local);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// 确保 `output` 的 local valtype 与即将 `local.set` 的栈顶类型一致。
    ///
    /// 解决 plan 阶段按 `value_types` 误分到 i32、emit 却压 anyref（或相反）时
    /// `local.set expected i32, found anyref` / 反向错误。
    pub(crate) fn force_output_local_for_stack_type(&mut self, output: MirValueRef, stack_ty: u8) {
        let want_ref = stack_ty == WASM_GC_ANYREF || stack_ty == WASM_GC_EXTERNREF;
        if want_ref {
            if let Some(local) = self.reference_locals.get(&output).copied() {
                let ty = self.wasm_local_value_type(local);
                if (ty == WASM_GC_ANYREF || ty == WASM_GC_EXTERNREF) && !self.wasm_local_is_typed_ref(local) {
                    return;
                }
            }
            if let Some(local) = self.value_locals.get(&output).copied().or_else(|| self.scalar_locals.get(&output).copied()) {
                if self.wasm_local_value_type(local) == WASM_GC_ANYREF || self.wasm_local_value_type(local) == WASM_GC_EXTERNREF {
                    self.value_locals.remove(&output);
                    self.scalar_locals.remove(&output);
                    self.reference_locals.insert(output, local);
                    return;
                }
            }
            self.value_locals.remove(&output);
            self.scalar_locals.remove(&output);
            self.reference_locals.remove(&output);
            let local = self.alloc_anyref_local();
            self.reference_locals.insert(output, local);
        }
        else {
            // 标量栈：?valtype 必须与栈顶精确一致（i32≠i64≠f64）?
            // 旧逻辑把任意非 ref 标量都当匹配，导?Copy/jump?
            // `local.get`(i64 param) →?`local.set`(i32) →?expected i32, found i64?
            let want_scalar = match stack_ty {
                VALTYPE_I64 | VALTYPE_F64 => stack_ty,
                _ => VALTYPE_I32,
            };
            if let Some(local) = self.value_locals.get(&output).copied().or_else(|| self.scalar_locals.get(&output).copied()) {
                let ty = self.wasm_local_value_type(local);
                if ty == want_scalar {
                    self.reference_locals.remove(&output);
                    return;
                }
                self.value_locals.remove(&output);
                self.scalar_locals.remove(&output);
                self.reference_locals.remove(&output);
                let new_local = self.alloc_scalar_local_for_stack_type(want_scalar);
                self.value_locals.insert(output, new_local);
                return;
            }
            if self.reference_locals.contains_key(&output) {
                self.reference_locals.remove(&output);
                self.value_locals.remove(&output);
                self.scalar_locals.remove(&output);
                let local = self.alloc_scalar_local_for_stack_type(want_scalar);
                self.value_locals.insert(output, local);
            }
            else if !self.value_locals.contains_key(&output) && !self.scalar_locals.contains_key(&output) {
                let local = self.alloc_scalar_local_for_stack_type(want_scalar);
                self.value_locals.insert(output, local);
            }
        }
    }

    pub(crate) fn assign_output_local(&mut self, output: MirValueRef) {
        if let Some(local) = self.reference_locals.get(&output).copied() {
            if self.wasm_local_is_typed_ref(local) {
                eprintln!(
                    "[wasm::ref-local] symbol={} value={} local={} local_ty={} planned_ty={:?}",
                    self.mir_fn.symbol,
                    output.0,
                    local,
                    self.wasm_local_value_type(local),
                    self.mir_fn.value_types.get(&output),
                );
            }
            self.emit_local_set(local);
        }
        else if let Some(local) = self.scalar_locals.get(&output).copied() {
            // 误挂 anyref ?scalar 槽：按真?valtype 分流，禁?i32.set →?anyref?
            if self.wasm_local_value_type(local) == WASM_GC_ANYREF || self.wasm_local_value_type(local) == WASM_GC_EXTERNREF {
                self.scalar_locals.remove(&output);
                self.value_locals.remove(&output);
                self.reference_locals.insert(output, local);
                self.emit_local_set(local);
                return;
            }
            self.emit_local_set(local);
        }
        else if let Some(local) = self.value_locals.get(&output).copied() {
            if self.wasm_local_value_type(local) == WASM_GC_ANYREF || self.wasm_local_value_type(local) == WASM_GC_EXTERNREF {
                panic!(
                    "WASM emit fail-closed: assign_output_local type mismatch (scalar on stack, anyref local) for %{} in `{}` (ADR 0008)",
                    output.0, self.mir_fn.symbol
                );
            }
            self.emit_local_set(local);
        }
        else {
            let ty = self.require_value_type(output);
            let is_anyref = wasm_gc_field_type_byte_for_glue(ty, self.js_glue_utf8_as_anyref) == WASM_GC_ANYREF;
            if is_anyref {
                let local = self.alloc_anyref_local();
                self.reference_locals.insert(output, local);
                self.emit_local_set(local);
            }
            else {
                let stack_ty = wasm_param_value_type(self.ctx, ty, self.js_glue_utf8_as_anyref);
                let local = self.alloc_scalar_local_for_stack_type(stack_ty);
                self.scalar_locals.insert(output, local);
                self.emit_local_set(local);
            }
        }
    }

    pub(crate) fn emit_placeholder_for_output(&mut self, output: MirValueRef) {
        panic!(
            "WASM emit fail-closed: emit_placeholder_for_output(%{}) in `{}`; refuse invent 0/ref.null (ADR 0008)",
            output.0, self.mir_fn.symbol
        );
    }

    pub(crate) fn operand_wasm_stack_type(&self, operand: &MirOperand) -> u8 {
        match operand {
            MirOperand::Value(vref) => {
                // 已分?local 时一律以槽位真实 valtype 为准?
                // ArrayGet 等会?Named 元素落到 i32 槽（`array (mut i32)` / utf8 句柄），
                // 若仍?value_types ?Reference/Named→anyref 分类，jump/Copy ?
                // `local.get`(i32) →?`local.set`(anyref)（func29：expected anyref, found i32）?
                if let Some(local) = self.reference_locals.get(vref).copied() {
                    return self.wasm_local_value_type(local);
                }
                if let Some(local) = self.value_locals.get(vref).copied().or_else(|| self.scalar_locals.get(vref).copied()) {
                    // 入口 anyref 参数若被误写?value_locals：wasm_local_value_type 仍返?anyref?
                    return self.wasm_local_value_type(local);
                }
                if let Some(ty) = self.mir_fn.value_types.get(vref) {
                    if type_is_wasm_gc_heap_reference(ty) {
                        return WASM_GC_ANYREF;
                    }
                    if self.js_glue_utf8_as_anyref && is_js_glue_host_string_type(ty) {
                        return WASM_GC_ANYREF;
                    }
                    if self.storage_for_type(ty) == StorageKind::Reference {
                        return WASM_GC_ANYREF;
                    }
                    return wasm_param_value_type(self.ctx, ty, self.js_glue_utf8_as_anyref);
                }
                panic!(
                    "WASM emit fail-closed: missing value_types for %{} in `{}`; refuse ANYREF default (ADR 0008)",
                    vref.0, self.mir_fn.symbol
                )
            }
            MirOperand::Constant(constant) => match constant {
                MirConstant::Utf8(_) => VALTYPE_I32,
                MirConstant::Utf16(_) => panic!("WASM lowering requires an explicit UTF-16 ABI contract"),
                MirConstant::Unit => WASM_GC_ANYREF,
                MirConstant::Float64(_) => VALTYPE_F64,
                MirConstant::Int(_) | MirConstant::Bool(_) => VALTYPE_I32,
            },
            MirOperand::Symbol(path) => {
                self.var_locals.get(&path.to_string()).copied().map(|local| self.wasm_local_value_type(local)).unwrap_or_else(|| {
                    panic!(
                        "WASM emit fail-closed: unresolved Symbol `{}` stack type in `{}`; refuse ANYREF default (ADR 0008)",
                        path, self.mir_fn.symbol
                    )
                })
            }
        }
    }

    pub(crate) fn emit_operand_coerced(&mut self, operand: &MirOperand, expected: u8) {
        let actual = self.operand_wasm_stack_type(operand);
        if actual == expected {
            self.emit_operand(operand);
            return;
        }
        // Reference-local metadata is authoritative for GC aggregates.
        if expected == WASM_GC_ANYREF && self.operand_reference_local(operand).is_some() {
            self.emit_operand(operand);
            return;
        }
        if expected == VALTYPE_I64 && actual == VALTYPE_I32 {
            self.emit_operand(operand);
            self.code.push(0xAC); // i64.extend_i32_s
            return;
        }
        if expected == VALTYPE_I32 && actual == VALTYPE_I64 {
            self.emit_operand(operand);
            WasmOpcode::I32WrapI64.encode(&mut self.code);
            return;
        }
        // Host import may declare externref while MIR carries anyref — pass through.
        if expected == WASM_GC_EXTERNREF && actual == WASM_GC_ANYREF {
            self.emit_operand(operand);
            return;
        }
        panic!(
            "WASM emit fail-closed: operand coerce expected {expected:#x} got {actual:#x} in `{}`; refuse invent 0/null (ADR 0008)",
            self.mir_fn.symbol
        );
    }
}
