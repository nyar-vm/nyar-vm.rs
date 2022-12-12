//! Resolved Semantic MIR calls -> Wasm call forms.
//!
//! Owns direct / import / intrinsic / witness / indirect call emission and the
//! call-shaped helpers currently on the same dispatch path. This is Wasm
//! **call encoding**, not a second MIR. Semantic MIR remains call-resolution
//! authority; this module only maps already-resolved calls to Wasm opcodes.

use super::*;

impl<'a> WasmMirLowerer<'a> {
    /// Lowers a non-builtin MIR call to WASM bytecode.
    ///
    /// - Static dispatch emits `call` (0x10) with the resolved function index.
    /// - Witness dispatch emits `call_indirect` (0x11) using the witness operand as the table index.
    /// - `receiver_kind: Some(ByAddress)` means the first argument is a value-type receiver
    ///   whose linear-memory address is already an `i32` local in `value_locals`.
    ///   Because `emit_operand` emits `local.get` for value locals, the receiver's
    ///   linear-memory address is passed directly as the first WASM parameter -?no
    ///   special codegen is needed beyond emitting the argument normally.
    pub(super) fn emit_call_lowering(
        &mut self,
        callee: &MirOperand,
        arguments: &[MirOperand],
        dispatch: MirDispatchKind,
        witness: Option<&MirOperand>,
        receiver_kind: Option<ReceiverPassingKind>,
        output: Option<MirValueRef>,
    ) {
        if let Some(ReceiverPassingKind::ByAddress) = receiver_kind {
            // Confirm the receiver exists as the first argument; it is emitted below.
        }
        let mut callee_return = None;
        let mut value_already_on_stack = false;
        match dispatch {
            MirDispatchKind::Static => {
                if self.try_emit_format_runtime_stub(callee, arguments, output) {
                    return;
                }
                if let Some(import_index) = self.resolve_callee_import_index(callee, arguments) {
                    if self.try_emit_wasi_cli_write_via_stream(callee, arguments, import_index, output) {
                        return;
                    }
                    callee_return = self.resolve_callee_return_type(callee, Some(import_index));
                    let param_types = self.resolve_callee_param_types(callee, Some(import_index));
                    self.emit_call_arguments(arguments, &param_types);
                    self.emit_call(import_index);
                    value_already_on_stack = callee_return.is_some();
                }
                else if let Some(function_index) = self.resolve_callee_function_index(callee) {
                    callee_return = self
                        .return_types_by_function_index
                        .get(&function_index)
                        .copied()
                        .flatten()
                        .or_else(|| self.resolve_callee_return_type(callee, None));
                    let param_types = self
                        .param_types_by_function_index
                        .get(&function_index)
                        .cloned()
                        .unwrap_or_else(|| self.resolve_callee_param_types(callee, None));
                    self.emit_call_arguments(arguments, &param_types);
                    self.emit_call(function_index);
                    value_already_on_stack = callee_return.is_some();
                }
                else if self.try_emit_sum_variant_ctor(callee, arguments, output) {
                    return;
                }
                else if self.try_emit_nullable_option_ctor(callee, arguments, output) {
                    return;
                }
                else if self.try_emit_tuple_get(callee, arguments, output) {
                    return;
                }
                else if self.try_emit_is_null(callee, arguments, output) {
                    return;
                }
                else {
                    eprintln!("[wasm::mir] unresolved static call in `{}`: callee={:?}", self.mir_fn.symbol, callee);
                    self.emit_unresolved_call_placeholder(arguments, output);
                    value_already_on_stack = output.is_some();
                }
            }
            MirDispatchKind::Witness => {
                callee_return = self.resolve_callee_return_type(callee, None);
                for argument in arguments {
                    self.emit_operand_coerced(argument, self.operand_wasm_stack_type(argument));
                }
                if let Some(witness_operand) = witness {
                    self.emit_operand(witness_operand);
                }
                else {
                    eprintln!("[wasm::mir] missing witness operand in `{}`: callee={:?}", self.mir_fn.symbol, callee);
                    self.emit_unresolved_call_placeholder(arguments, output);
                    value_already_on_stack = output.is_some();
                    self.emit_store_call_output(output, callee_return, value_already_on_stack);
                    return;
                }
                let type_index = self.resolve_callee_type_index(callee);
                self.emit_call_indirect(type_index, 0);
                value_already_on_stack = callee_return.is_some();
            }
            MirDispatchKind::Indirect => {
                eprintln!("[wasm::mir] unsupported Indirect dispatch in `{}`: callee={:?}", self.mir_fn.symbol, callee);
                self.emit_unresolved_call_placeholder(arguments, output);
                value_already_on_stack = output.is_some();
            }
        }
        self.emit_store_call_output(output, callee_return, value_already_on_stack);
    }

    fn emit_store_call_output(&mut self, output: Option<MirValueRef>, callee_return: Option<u8>, value_already_on_stack: bool) {
        match output {
            Some(output) => {
                // Type-driven aggregate ABI trace; keep parser/library names
                // out of the diagnostic so this remains a reusable contract.
                if let Some(semantic_ty) = self.mir_fn.value_types.get(&output) {
                    if matches!(semantic_ty, NyarType::Array(_) | NyarType::FixedArray { .. } | NyarType::Named(_) | NyarType::Union(_))
                        && callee_return.is_some()
                    {
                        eprintln!(
                            "[wasm::aggregate-call-output] fn={} output={} semantic={semantic_ty:?} callee_return={callee_return:?} value_already_on_stack={} refs={} values={} scalars={}",
                            self.mir_fn.symbol,
                            output.0,
                            value_already_on_stack,
                            self.reference_locals.contains_key(&output),
                            self.value_locals.contains_key(&output),
                            self.scalar_locals.contains_key(&output),
                        );
                    }
                }
                // 根据 callee 的真?wasm 返回类型预分?output local?
                // 防止 MIR 类型推断（storage_for_type / wasm_gc_field_type_byte?
                // ?wasm 类型段中 callee 的返回类型不一致导?local.set 类型不匹配?
                self.ensure_call_output_local(output, callee_return);
                if value_already_on_stack {
                    self.coerce_call_result_for_output(output, callee_return);
                }
                else if callee_return.is_none() {
                    self.emit_placeholder_for_output(output);
                }
                self.assign_output_local(output);
            }
            None => {
                if callee_return.is_some() {
                    WasmOpcode::Drop.encode(&mut self.code);
                }
            }
        }
    }

    /// 根据 callee 的真?wasm 返回类型确保 output local 已分配到正确?map?
    ///
    /// `callee_return` 来自 wasm 类型段，?MIR 类型推断更可靠：
    /// - 返回 anyref/externref 时，output 应在 `reference_locals`
    /// - 返回 i32/i64/f64 时，output 应在 `scalar_locals`
    /// 若已分配到错误的 map，清理旧条目后重新分配?
    fn ensure_call_output_local(&mut self, output: MirValueRef, callee_return: Option<u8>) {
        let Some(return_type) = callee_return
        else {
            return;
        };
        let needs_anyref = return_type == WASM_GC_ANYREF || return_type == WASM_GC_EXTERNREF;
        let needs_scalar = return_type == VALTYPE_I32 || return_type == VALTYPE_I64 || return_type == VALTYPE_F64;
        if needs_anyref && !self.reference_locals.contains_key(&output) {
            self.value_locals.remove(&output);
            self.scalar_locals.remove(&output);
            let local = self.alloc_anyref_local();
            self.reference_locals.insert(output, local);
        }
        else if needs_scalar && !self.value_locals.contains_key(&output) && !self.scalar_locals.contains_key(&output) {
            self.reference_locals.remove(&output);
            let local = self.alloc_scalar_local_for_stack_type(return_type);
            self.scalar_locals.insert(output, local);
        }
        else if needs_scalar {
            // 已有标量槽但 valtype ?callee 返回不一致（常见：plan ?i32，call 返回 i64）?
            if let Some(local) = self.value_locals.get(&output).copied().or_else(|| self.scalar_locals.get(&output).copied()) {
                let ty = self.wasm_local_value_type(local);
                if ty != return_type && matches!(return_type, VALTYPE_I32 | VALTYPE_I64 | VALTYPE_F64) {
                    self.value_locals.remove(&output);
                    self.scalar_locals.remove(&output);
                    self.reference_locals.remove(&output);
                    let new_local = self.alloc_scalar_local_for_stack_type(return_type);
                    self.scalar_locals.insert(output, new_local);
                }
            }
        }
    }

    /// ?call 返回值类型与 output local 类型不一致时，在栈顶做强制转换?
    fn coerce_call_result_for_output(&mut self, output: MirValueRef, callee_return: Option<u8>) {
        let Some(return_type) = callee_return
        else {
            return;
        };
        let output_is_anyref = self.reference_locals.contains_key(&output)
            || (!self.value_locals.contains_key(&output)
                && !self.scalar_locals.contains_key(&output)
                && self
                    .mir_fn
                    .value_types
                    .get(&output)
                    .map(|ty| wasm_gc_field_type_byte_for_glue(ty, self.js_glue_utf8_as_anyref) == WASM_GC_ANYREF)
                    .unwrap_or(false));
        if return_type == VALTYPE_I32 && output_is_anyref {
            WasmOpcode::Drop.encode(&mut self.code);
            self.emit_ref_null_anyref();
        }
        else if return_type == VALTYPE_I64 && (self.value_locals.contains_key(&output) || self.scalar_locals.contains_key(&output)) {
            // 仅当输出槽仍?i32 ?wrap；i64 槽保持原值（ensure_call_output_local 应已对齐）?
            if let Some(local) = self.value_locals.get(&output).copied().or_else(|| self.scalar_locals.get(&output).copied()) {
                if self.wasm_local_value_type(local) == VALTYPE_I32 {
                    WasmOpcode::I32WrapI64.encode(&mut self.code);
                }
            }
        }
        else if return_type == VALTYPE_F64 && (self.value_locals.contains_key(&output) || self.scalar_locals.contains_key(&output)) {
            WasmOpcode::I32TruncF64S.encode(&mut self.code);
        }
        else if (return_type == WASM_GC_ANYREF || return_type == WASM_GC_EXTERNREF)
            && (self.value_locals.contains_key(&output) || self.scalar_locals.contains_key(&output))
        {
            WasmOpcode::Drop.encode(&mut self.code);
            self.emit_i32_const(0);
        }
        else if return_type == WASM_GC_EXTERNREF && output_is_anyref {
            WasmOpcode::Drop.encode(&mut self.code);
            self.emit_ref_null_anyref();
        }
        else if return_type == WASM_GC_EXTERNREF && (self.value_locals.contains_key(&output) || self.scalar_locals.contains_key(&output)) {
            WasmOpcode::Drop.encode(&mut self.code);
            self.emit_i32_const(0);
        }
        else if (return_type == WASM_GC_ANYREF || return_type == WASM_GC_EXTERNREF)
            && (self.value_locals.contains_key(&output) || self.scalar_locals.contains_key(&output))
        {
            // 计划阶段误分配到 i32 local，但 callee 实际返回 anyref：迁?output local?
            // 保留栈顶宿主字符串，避免 drop + i32.const 0 抹掉 cli_get_* 返回值?
            self.value_locals.remove(&output);
            self.scalar_locals.remove(&output);
            let local = self.alloc_anyref_local();
            self.reference_locals.insert(output, local);
        }
    }

    /// 确保 `output` ?local ?valtype 与即?`local.set` 的栈顶类型一致?
    ///
    /// 解决 plan 阶段?`value_types` 误分?i32、emit 却压 anyref（或相反）时?
    /// `local.set expected i32, found anyref` / 反向错误?
    pub(super) fn force_output_local_for_stack_type(&mut self, output: MirValueRef, stack_ty: u8) {
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

    pub(super) fn assign_output_local(&mut self, output: MirValueRef) {
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
                // 栈顶是标量却误挂 anyref：丢弃栈顶并?null，禁?`local.set expected anyref, found i32`?
                // 正确路径应先?force_output_local_for_stack_type 迁到 i32；此处为 fail-closed 兜底?
                WasmOpcode::Drop.encode(&mut self.code);
                self.emit_ref_null_anyref();
                self.value_locals.remove(&output);
                self.reference_locals.insert(output, local);
                self.emit_local_set(local);
                return;
            }
            self.emit_local_set(local);
        }
        else {
            // 使用 wasm_gc_field_type_byte 判断 wasm 实际栈类型，
            // 而非 storage_for_type——后者对 value-type Named 返回 Value(i32)?
            // ?wasm 类型段中 Named ?valtype ?anyref(VALTYPE_ANYREF)?
            let ty = self.mir_fn.value_types.get(&output);
            let is_anyref = ty.map(|ty| wasm_gc_field_type_byte_for_glue(ty, self.js_glue_utf8_as_anyref) == WASM_GC_ANYREF).unwrap_or(false);
            if is_anyref {
                let local = self.alloc_anyref_local();
                self.reference_locals.insert(output, local);
                self.emit_local_set(local);
            }
            else {
                let local = self.alloc_i32_local();
                self.scalar_locals.insert(output, local);
                self.emit_local_set(local);
            }
        }
    }

    fn emit_placeholder_for_output(&mut self, output: MirValueRef) {
        if self.reference_locals.contains_key(&output) {
            self.emit_ref_null_anyref();
            return;
        }
        if let Some(local) = self.value_locals.get(&output).copied().or_else(|| self.scalar_locals.get(&output).copied()) {
            match self.wasm_local_value_type(local) {
                VALTYPE_I64 => self.emit_i64_const(0),
                VALTYPE_F64 => self.emit_f64_const(0.0),
                WASM_GC_ANYREF | WASM_GC_EXTERNREF => self.emit_ref_null_anyref(),
                _ => self.emit_i32_const(0),
            }
            return;
        }
        let is_anyref = self
            .mir_fn
            .value_types
            .get(&output)
            .map(|ty| wasm_gc_field_type_byte_for_glue(ty, self.js_glue_utf8_as_anyref) == WASM_GC_ANYREF)
            .unwrap_or(false);
        if is_anyref {
            self.emit_ref_null_anyref();
        }
        else if self
            .mir_fn
            .value_types
            .get(&output)
            .map(|ty| matches!(ty, NyarType::Integer64 { .. } | NyarType::Integer128 { .. }))
            .unwrap_or(false)
        {
            self.emit_i64_const(0);
        }
        else {
            self.emit_i32_const(0);
        }
    }

    /// 为未解析的调用发射占位返回?替代 `unreachable` (0x00)?
    ///
    /// 旧实现对未解析的静态调用和 EffectHandler 陷阱发射 `unreachable`,
    /// 依赖 WASM 多态栈让后?`local.set` 通过验证。但 V8 在多态栈上下文中
    /// ?`local.get X (anyref)` →?`local.set Y (i32)` 仍做类型一致性检?
    /// 即使规范允许在多态栈?pop 任意类型,导致自举链路阻断?
    ///
    /// 替代方案:弹出已压入的参数(`drop` × N),再压入与 output local 类型匹配?
    /// 占位?`ref.null anyref` ?`i32.const 0`),使后?`local.set` 类型一致?
    /// 占位值类型由 output 已分配的 local 类型决定;若未分配,?`value_types` 推断?
    fn emit_unresolved_call_placeholder(&mut self, arguments: &[MirOperand], output: Option<MirValueRef>) {
        for _ in arguments {
            WasmOpcode::Drop.encode(&mut self.code);
        }
        if let Some(output) = output {
            self.emit_placeholder_for_output(output);
        }
    }

    /// Lower the injected `format` runtime stub on WASM/Node.
    ///
    /// The host-independent bootstrap only needs a deterministic UTF-8 handle
    /// here.  Consume the template/value operands and return the null handle
    /// through the scalar output local instead of treating `format` as an
    /// unresolved static function (which leaves large self-hosting partitions
    /// in the unresolved-call path).
    fn try_emit_format_runtime_stub(&mut self, callee: &MirOperand, arguments: &[MirOperand], output: Option<MirValueRef>) -> bool {
        let MirOperand::Symbol(path) = callee
        else {
            return false;
        };
        if path.parts().len() != 1 || path.parts().first().map(|part| part.as_str()) != Some("format") {
            return false;
        }
        for argument in arguments {
            self.emit_operand(argument);
            WasmOpcode::Drop.encode(&mut self.code);
        }
        if let Some(output) = output {
            self.emit_i32_const(0);
            self.force_output_local_for_stack_type(output, VALTYPE_I32);
            self.assign_output_local(output);
        }
        true
    }

    pub(super) fn resolve_callee_param_types(&self, callee: &MirOperand, import_index: Option<u32>) -> Vec<u8> {
        if let Some(index) = import_index {
            return self.import_param_types.get(index as usize).cloned().unwrap_or_default();
        }
        let MirOperand::Symbol(path) = callee
        else {
            return Vec::new();
        };
        let dotted = path.to_string();
        if let Some(params) = self.param_types_by_name.get(&dotted) {
            return params.clone();
        }
        let parts = path.parts();
        if parts.is_empty() {
            return Vec::new();
        }
        // ?`resolve_callee_function_index` 保持对称：仅单段路径?simple name?
        if parts.len() == 1 {
            if let Some(params) = self.param_types_by_name.get(parts[0].as_str()) {
                return params.clone();
            }
        }
        let simple = parts[parts.len() - 1].as_str();
        unique_simple_name_match(&self.param_types_by_name, simple).cloned().unwrap_or_default()
    }

    pub(super) fn resolve_callee_return_type(&self, callee: &MirOperand, import_index: Option<u32>) -> Option<u8> {
        if let Some(index) = import_index {
            return self.import_return_types.get(index as usize).copied().flatten();
        }
        let MirOperand::Symbol(path) = callee
        else {
            return None;
        };
        let dotted = path.to_string();
        if let Some(return_type) = self.return_types_by_name.get(&dotted) {
            return *return_type;
        }
        let parts = path.parts();
        if parts.is_empty() {
            return None;
        }
        // ?`resolve_callee_function_index` 保持对称：仅单段路径?simple name?
        // 多段路径的歧义简单名不得用「字典序第一?ends_with」冒充，否则
        // `ArrayList::get` 会命?`std::net::get` 等错?callee?
        if parts.len() == 1 {
            if let Some(return_type) = self.return_types_by_name.get(parts[0].as_str()) {
                return *return_type;
            }
        }
        let simple = parts[parts.len() - 1].as_str();
        unique_simple_name_match(&self.return_types_by_name, simple).copied().flatten()
    }

    pub(super) fn operand_wasm_stack_type(&self, operand: &MirOperand) -> u8 {
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
                WASM_GC_ANYREF
            }
            MirOperand::Constant(constant) => match constant {
                MirConstant::Utf8(_) => VALTYPE_I32,
                MirConstant::Utf16(_) => panic!("WASM lowering requires an explicit UTF-16 ABI contract"),
                MirConstant::Unit => WASM_GC_ANYREF,
                MirConstant::Float64(_) => VALTYPE_F64,
                MirConstant::Int(_) | MirConstant::Bool(_) => VALTYPE_I32,
            },
            MirOperand::Symbol(path) => {
                self.var_locals.get(&path.to_string()).copied().map(|local| self.wasm_local_value_type(local)).unwrap_or(WASM_GC_ANYREF)
            }
        }
    }

    pub(super) fn emit_operand_coerced(&mut self, operand: &MirOperand, expected: u8) {
        let actual = self.operand_wasm_stack_type(operand);
        if actual == expected {
            self.emit_operand(operand);
            return;
        }
        // Reference-local metadata is authoritative for GC aggregates. The
        // semantic classifier can still report an i32 fallback for a named
        // value; replacing that value with ref.null loses the object and only
        // surfaces later as an array/struct illegal cast.
        if expected == WASM_GC_ANYREF && self.operand_reference_local(operand).is_some() {
            self.emit_operand(operand);
            return;
        }
        if expected == WASM_GC_ANYREF && actual == VALTYPE_I32 {
            self.emit_ref_null_anyref();
            return;
        }
        if expected == VALTYPE_I32 && actual == WASM_GC_ANYREF {
            self.emit_i32_const(0);
            return;
        }
        // i64 形参：anyref/i32 不得原样压栈（否?component/core 校验 expected i64, found anyref）?
        if expected == VALTYPE_I64 && (actual == WASM_GC_ANYREF || actual == WASM_GC_EXTERNREF) {
            self.emit_i64_const(0);
            return;
        }
        if expected == VALTYPE_I64 && actual == VALTYPE_I32 {
            self.emit_operand(operand);
            // i64.extend_i32_s (0xAC) -?WasmOpcode 枚举尚未收录该变体?
            self.code.push(0xAC);
            return;
        }
        if expected == VALTYPE_I32 && actual == VALTYPE_I64 {
            self.emit_operand(operand);
            WasmOpcode::I32WrapI64.encode(&mut self.code);
            return;
        }
        if expected == WASM_GC_ANYREF && actual == VALTYPE_I64 {
            self.emit_ref_null_anyref();
            return;
        }
        if expected == VALTYPE_F64
            && (actual == WASM_GC_ANYREF || actual == WASM_GC_EXTERNREF || actual == VALTYPE_I32 || actual == VALTYPE_I64)
        {
            self.emit_f64_const(0.0);
            return;
        }
        if expected == WASM_GC_ANYREF && actual == VALTYPE_F64 {
            self.emit_ref_null_anyref();
            return;
        }
        // host import 签名使用 externref；MIR 操作数默认为 anyref?
        // 不能?ref.func/ref.is_null 做转换——以 null 占位保持栈类型一致?
        if expected == WASM_GC_EXTERNREF && actual == WASM_GC_ANYREF {
            self.emit_operand(operand);
            return;
        }
        if expected == WASM_GC_EXTERNREF && actual == VALTYPE_I32 {
            self.emit_ref_null_extern();
            return;
        }
        if expected == WASM_GC_EXTERNREF && actual == VALTYPE_I64 {
            self.emit_ref_null_extern();
            return;
        }
        if expected == WASM_GC_ANYREF && actual == WASM_GC_EXTERNREF {
            self.emit_ref_null_anyref();
            return;
        }
        self.emit_operand(operand);
    }

    pub(super) fn resolve_callee_import_index(&self, callee: &MirOperand, arguments: &[MirOperand]) -> Option<u32> {
        let path = match callee {
            MirOperand::Symbol(path) => path,
            _ => return None,
        };
        let dotted = path.to_string();
        if let Some(index) = self.callee_import_index.get(&dotted).copied() {
            return Some(index);
        }
        let parts = path.parts();
        if parts.is_empty() {
            return None;
        }
        let simple = parts[parts.len() - 1].as_str();
        if let Some(index) = self.callee_import_index.get(simple).copied() {
            return Some(index);
        }
        None
    }

    /// 判定 callee 是否?i32 原语运算（`prefix !`、`infix ==`、`infix +` 等）?
    ///
    /// ?`try_emit_i32_primitive_call` ?simple name 匹配保持一致?
    /// 用于 `output_storage_kind` ?`value_types` 缺失时避免将这些 i32 原语
    /// ?output 误分配到 `reference_locals`（anyref），从而触?
    /// `local.set expected anyref, found i32.eqz of type i32` 类型错误?
    fn is_i32_primitive_callee(&self, callee: &MirOperand) -> bool {
        let MirOperand::Symbol(path) = callee
        else {
            return false;
        };
        let simple = path.parts().last().map(|p| p.as_str()).unwrap_or("");
        matches!(
            simple,
            "infix ==" | "infix !=" | "infix <" | "infix <=" | "infix >" | "infix >=" | "infix +" | "infix -" | "infix *" | "prefix !"
        )
    }

    /// 将未解析?`infix ==` / `infix +` 等降?wasm i32 原语（整数路径）?
    /// WASI 轨：两端均为 utf8 句柄时，`==`/`!=` ?`[len][bytes]` 做内容比较（非指针相等）?
    /// 以便 `get-arguments` 拷贝出的句柄能匹配字面量?
    fn try_emit_i32_primitive_call(&mut self, callee: &MirOperand, arguments: &[MirOperand], output: Option<MirValueRef>) -> bool {
        let MirOperand::Symbol(path) = callee
        else {
            return false;
        };
        let simple = path.parts().last().map(|p| p.as_str()).unwrap_or("");
        if self.wasi_mode && matches!(simple, "infix ==" | "infix !=") && arguments.len() >= 2 {
            if self.operand_is_wasi_utf8_handle(&arguments[0]) && self.operand_is_wasi_utf8_handle(&arguments[1]) {
                self.emit_wasi_utf8_content_compare(&arguments[0], &arguments[1], simple == "infix !=");
                if let Some(output) = output {
                    self.force_output_local_for_stack_type(output, VALTYPE_I32);
                    self.store_scalar(output);
                }
                return true;
            }
        }
        let opcode = match simple {
            "infix ==" => WasmOpcode::I32Eq,
            "infix !=" => WasmOpcode::I32Ne,
            "infix <" => WasmOpcode::I32LtS,
            "infix <=" => WasmOpcode::I32LeS,
            "infix >" => WasmOpcode::I32GtS,
            "infix >=" => WasmOpcode::I32GeS,
            "infix +" => WasmOpcode::I32Add,
            "infix -" => WasmOpcode::I32Sub,
            "infix *" => WasmOpcode::I32Mul,
            "prefix !" => {
                if arguments.len() != 1 {
                    return false;
                }
                self.emit_i32_operand(&arguments[0]);
                WasmOpcode::I32Eqz.encode(&mut self.code);
                if let Some(output) = output {
                    self.force_output_local_for_stack_type(output, VALTYPE_I32);
                    self.store_scalar(output);
                }
                return true;
            }
            _ => return false,
        };
        if arguments.len() < 2 {
            return false;
        }
        self.emit_i32_operand(&arguments[0]);
        self.emit_i32_operand(&arguments[1]);
        opcode.encode(&mut self.code);
        if let Some(output) = output {
            self.force_output_local_for_stack_type(output, VALTYPE_I32);
            self.store_scalar(output);
        }
        true
    }

    /// WASI utf8 句柄：字面量偏移，或类型?utf8/Utf8Text（i32 指向 `[len][bytes]`）?
    fn operand_is_wasi_utf8_handle(&self, operand: &MirOperand) -> bool {
        match operand {
            MirOperand::Constant(MirConstant::Utf8(_)) => true,
            MirOperand::Value(vref) => self.mir_fn.value_types.get(vref).is_some_and(|ty| is_js_glue_host_string_type(ty)),
            _ => false,
        }
    }

    /// 比较两个 WASI utf8 句柄的内容；栈顶留下 0/1。`negate` 时对结果取反（用?`!=`）?
    fn emit_wasi_utf8_content_compare(&mut self, left: &MirOperand, right: &MirOperand, negate: bool) {
        let a = self.stack_ptr_local;
        let b = self.bump_end_local;
        let n = self.bump_pages_local;
        self.emit_i32_operand(left);
        self.emit_local_set(a);
        self.emit_i32_operand(right);
        self.emit_local_set(b);

        // if (result i32) a == b { 1 } else { len/bytes compare }
        self.emit_local_get(a);
        self.emit_local_get(b);
        WasmOpcode::I32Eq.encode(&mut self.code);
        WasmOpcode::If.encode(&mut self.code);
        self.code.push(VALTYPE_I32);
        encode_i32_const(1, &mut self.code);
        WasmOpcode::Else.encode(&mut self.code);
        self.emit_local_get(a);
        encode_i32_load(2, 0, &mut self.code);
        self.emit_local_get(b);
        encode_i32_load(2, 0, &mut self.code);
        WasmOpcode::I32Ne.encode(&mut self.code);
        WasmOpcode::If.encode(&mut self.code);
        self.code.push(VALTYPE_I32);
        encode_i32_const(0, &mut self.code);
        WasmOpcode::Else.encode(&mut self.code);
        // n = len; a = a+4; b = b+4
        self.emit_local_get(a);
        encode_i32_load(2, 0, &mut self.code);
        self.emit_local_set(n);
        self.emit_local_get(a);
        encode_i32_const(4, &mut self.code);
        encode_i32_add(&mut self.code);
        self.emit_local_set(a);
        self.emit_local_get(b);
        encode_i32_const(4, &mut self.code);
        encode_i32_add(&mut self.code);
        self.emit_local_set(b);
        // block (result i32) { loop { ... } unreachable }
        WasmOpcode::Block.encode(&mut self.code);
        self.code.push(VALTYPE_I32);
        encode_loop_empty(&mut self.code);
        self.emit_local_get(n);
        encode_i32_eqz(&mut self.code);
        WasmOpcode::If.encode(&mut self.code);
        self.code.push(BLOCKTYPE_EMPTY);
        encode_i32_const(1, &mut self.code);
        encode_br(2, &mut self.code); // →?block result
        WasmOpcode::End.encode(&mut self.code);
        self.emit_local_get(a);
        WasmOpcode::I32Load8U.encode(&mut self.code);
        encode_uleb128(0, &mut self.code);
        encode_uleb128(0, &mut self.code);
        self.emit_local_get(b);
        WasmOpcode::I32Load8U.encode(&mut self.code);
        encode_uleb128(0, &mut self.code);
        encode_uleb128(0, &mut self.code);
        WasmOpcode::I32Ne.encode(&mut self.code);
        WasmOpcode::If.encode(&mut self.code);
        self.code.push(BLOCKTYPE_EMPTY);
        encode_i32_const(0, &mut self.code);
        encode_br(2, &mut self.code); // →?block result
        WasmOpcode::End.encode(&mut self.code);
        self.emit_local_get(a);
        encode_i32_const(1, &mut self.code);
        encode_i32_add(&mut self.code);
        self.emit_local_set(a);
        self.emit_local_get(b);
        encode_i32_const(1, &mut self.code);
        encode_i32_add(&mut self.code);
        self.emit_local_set(b);
        self.emit_local_get(n);
        encode_i32_const(1, &mut self.code);
        encode_i32_sub(&mut self.code);
        self.emit_local_set(n);
        encode_br(0, &mut self.code);
        WasmOpcode::End.encode(&mut self.code); // loop
        encode_unreachable(&mut self.code);
        WasmOpcode::End.encode(&mut self.code); // block (result i32)
        WasmOpcode::End.encode(&mut self.code); // else len-eq
        WasmOpcode::End.encode(&mut self.code); // else a!=b handles
        if negate {
            encode_i32_eqz(&mut self.code);
        }
    }

    /// `length` ?GC/引用数组接收者上降为 `array.len`?
    fn try_emit_array_length_call(&mut self, callee: &MirOperand, arguments: &[MirOperand], output: Option<MirValueRef>) -> bool {
        let MirOperand::Symbol(path) = callee
        else {
            return false;
        };
        let simple = path.parts().last().map(|p| p.as_str()).unwrap_or("");
        if simple != "length" || arguments.is_empty() {
            return false;
        }
        let MirOperand::Value(vref) = &arguments[0]
        else {
            return false;
        };
        if !self.reference_locals.contains_key(vref) {
            return false;
        }
        self.emit_operand_coerced(&arguments[0], WASM_GC_ANYREF);
        self.emit_ref_cast_array();
        self.emit_array_len();
        if let Some(output) = output {
            self.store_scalar(output);
        }
        true
    }

    // ── unite sum / tuple / nullable option 降低 ──────────────────────

    /// 尝试?`Fine(x)` / `Fail(x)` / `EndOfFile()` ?unite 变体构造降低为
    /// `struct.new [tag, payload]`。与 CLR `emit_sum_variant_ctor` 同构?
    fn try_emit_sum_variant_ctor(&mut self, callee: &MirOperand, arguments: &[MirOperand], output: Option<MirValueRef>) -> bool {
        let MirOperand::Symbol(path) = callee
        else {
            return false;
        };
        let Some((sum_name, tag)) = self.resolve_wasm_sum_variant(path, output, arguments)
        else {
            return false;
        };
        let Some(type_index) = self.resolve_gc_sum_type_index(&sum_name)
        else {
            return false;
        };
        // anyref ?+ ref.cast：避?typed (ref T) local ?struct.set typeidx 错位
        // （曾触发 `struct.set expected (ref null X), found (ref Y)`）?
        self.emit_struct_new_default(type_index);
        let tmp = self.alloc_anyref_local();
        self.emit_local_set(tmp);
        self.emit_local_get(tmp);
        self.emit_ref_cast_struct(type_index);
        self.emit_i32_const(tag as i32);
        self.emit_struct_set(type_index, 0); // field 0 = tag
        // payload：anyref 直接存；i32（utf8/bool 句柄）装箱为 `[i32]`；void≠unit?
        match arguments {
            [] => {
                self.emit_ref_null_anyref();
            }
            [only] => {
                self.emit_operand_as_unite_payload(only);
            }
            [first, ..] => {
                self.emit_operand_as_unite_payload(first);
            }
        }
        let payload_tmp = self.alloc_anyref_local();
        self.emit_local_set(payload_tmp);
        self.emit_local_get(tmp);
        self.emit_ref_cast_struct(type_index);
        self.emit_local_get(payload_tmp);
        self.emit_struct_set(type_index, 1); // field 1 = payload
        if let Some(out) = output {
            self.emit_local_get(tmp);
            self.force_output_local_for_stack_type(out, VALTYPE_ANYREF);
            self.assign_output_local(out);
        }
        true
    }

    /// Lookup unite sum wasm-gc type_index by exact name, then simple-name fallback.
    fn resolve_gc_sum_type_index(&self, sum_name: &str) -> Option<u32> {
        if let Some(&idx) = self.gc_sum_type_indices.get(sum_name) {
            return Some(idx);
        }
        let simple = simple_name_of(sum_name);
        self.gc_sum_type_indices.iter().find(|(k, _)| simple_name_of(k) == simple).map(|(_, &v)| v)
    }

    /// Resolve Fine/Fail/Object/-?/ EndOfFile/StringLiteral-?variant ctor -?
    /// isomorphic to CLR `resolve_sum_variant_ctor`（含 unite ?payload-less enums）?
    fn resolve_wasm_sum_variant(&self, path: &NamePath, output: Option<MirValueRef>, arguments: &[MirOperand]) -> Option<(String, u32)> {
        let variant_name = path.parts().last()?.as_str();
        let mut matches: Vec<(String, u32)> = Vec::new();
        for sum in &self.ctx.submission.sum_types {
            // Only emit when a gc structtype was registered for this sum.
            if self.resolve_gc_sum_type_index(&sum.name).is_none() {
                continue;
            }
            if let Some(v) = sum.variants.iter().find(|v| v.name == variant_name) {
                matches.push((sum.name.clone(), v.tag));
            }
        }
        if matches.is_empty() {
            return None;
        }
        // 期望 sum 提示：output ?value_type 或函数返回类型?
        if let Some(expected) = self.expected_sum_name(output) {
            if let Some(pos) = matches.iter().position(|(n, _)| self.sum_name_matches(n, &expected)) {
                return Some(matches.remove(pos));
            }
        }
        // A qualified variant path is only a hint. Use it as a nominal
        // discriminator, never as a library-specific semantic rule.
        if path.parts().len() >= 2 {
            let hint = path.parts()[path.parts().len() - 2].as_str();
            if let Some(pos) = matches.iter().position(|(n, _)| self.sum_name_matches(n, hint)) {
                return Some(matches.remove(pos));
            }
        }
        // 按参?arity 消歧?
        let arg_arity = arguments.len();
        let mut by_arity: Vec<(String, u32)> = matches
            .iter()
            .filter(|(sum_name, tag)| {
                self.ctx
                    .submission
                    .sum_types
                    .iter()
                    .find(|s| s.name == *sum_name)
                    .and_then(|s| s.variants.iter().find(|v| v.tag == *tag && v.name == variant_name))
                    .map(|v| match v.payload_type.as_ref() {
                        None => 0,
                        Some(NyarType::Tuple(items)) => items.len(),
                        Some(_) => 1,
                    })
                    .unwrap_or(0)
                    == arg_arity
            })
            .cloned()
            .collect();
        if by_arity.len() == 1 {
            return Some(by_arity.remove(0));
        }
        if by_arity.len() > 1 {
            matches = by_arity;
        }
        // Payload-typed disambiguation is the generic tie breaker. This keeps
        // the resolver independent of aliases and library names.
        if let Some(arg) = arguments.first() {
            if let Some(arg_ty) = self.lookup_operand_nyar_type(arg) {
                let arg_name = Self::sum_type_name_from_nyar(&arg_ty).unwrap_or_default();
                if !arg_name.is_empty() {
                    let mut by_payload = Vec::new();
                    for (sum_name, tag) in &matches {
                        let Some(sum) = self.ctx.submission.sum_types.iter().find(|s| s.name == *sum_name)
                        else {
                            continue;
                        };
                        let Some(variant) = sum.variants.iter().find(|v| v.tag == *tag && v.name == variant_name)
                        else {
                            continue;
                        };
                        let Some(payload) = variant.payload_type.as_ref()
                        else {
                            continue;
                        };
                        if let Some(payload_name) = Self::sum_type_name_from_nyar(payload) {
                            if self.sum_name_matches(&payload_name, &arg_name) || self.sum_name_matches(&arg_name, &payload_name) {
                                by_payload.push((sum_name.clone(), *tag));
                            }
                        }
                    }
                    if by_payload.len() == 1 {
                        return Some(by_payload.remove(0));
                    }
                    if by_payload.len() > 1 {
                        matches = by_payload;
                    }
                }
            }
        }
        if matches.len() == 1 {
            return Some(matches.remove(0));
        }
        // Use the operation-local candidate only when it is uniquely scored.
        if let Some(picked) = self.pick_wasm_operation_local_sum(&matches) {
            return Some(picked);
        }
        None
    }

    fn pick_wasm_operation_local_sum(&self, matches: &[(String, u32)]) -> Option<(String, u32)> {
        let fn_symbol = &self.mir_fn.symbol;
        let lower = fn_symbol.to_ascii_lowercase();
        let mut parts: Vec<String> = Vec::new();
        for piece in lower.split([':', '_', '.']) {
            if piece.len() >= 3 {
                parts.push(piece.to_string());
            }
        }
        if parts.is_empty() {
            return None;
        }
        let mut scored: Vec<(usize, String, u32)> = matches
            .iter()
            .map(|(name, tag)| {
                let name_lower = name.to_ascii_lowercase();
                let score = parts.iter().filter(|p| name_lower.contains(p.as_str())).count();
                (score, name.clone(), *tag)
            })
            .filter(|(score, ..)| *score > 0)
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        if scored.len() == 1 || (scored.len() > 1 && scored[0].0 > scored[1].0) {
            let (_, name, tag) = scored.remove(0);
            return Some((name, tag));
        }
        None
    }

    fn lookup_operand_nyar_type(&self, operand: &MirOperand) -> Option<NyarType> {
        match operand {
            MirOperand::Value(vref) => self.mir_fn.value_types.get(vref).cloned(),
            _ => None,
        }
    }

    fn expected_sum_name(&self, output: Option<MirValueRef>) -> Option<String> {
        if let Some(vref) = output {
            if let Some(ty) = self.mir_fn.value_types.get(&vref) {
                if let Some(name) = Self::sum_type_name_from_nyar(ty) {
                    return Some(name);
                }
            }
        }
        Self::sum_type_name_from_nyar(&self.mir_fn.return_type)
    }

    fn sum_type_name_from_nyar(ty: &NyarType) -> Option<String> {
        match ty {
            NyarType::Named(name) => Some(name.to_string()),
            NyarType::Apply(base, _) => match base.as_ref() {
                NyarType::Named(name) => Some(name.to_string()),
                _ => None,
            },
            _ => None,
        }
    }

    fn sum_name_matches(&self, sum_name: &str, hint: &str) -> bool {
        let sum_simple = simple_name_of(sum_name);
        let hint_simple = simple_name_of(hint);
        sum_name == hint
            || sum_simple == hint
            || sum_simple == hint_simple
            || sum_name.ends_with(&format!("::{hint}"))
            || sum_name.ends_with(&format!("::{hint_simple}"))
    }

    /// `None()` →?ref.null anyref；`Some(x)` →?x as anyref?
    fn try_emit_nullable_option_ctor(&mut self, callee: &MirOperand, arguments: &[MirOperand], output: Option<MirValueRef>) -> bool {
        let MirOperand::Symbol(path) = callee
        else {
            return false;
        };
        if path.parts().len() != 1 {
            return false;
        }
        let name = path.parts()[0].as_str();
        match name {
            "None" if arguments.is_empty() => {
                self.emit_ref_null_anyref();
                if let Some(out) = output {
                    self.force_output_local_for_stack_type(out, VALTYPE_ANYREF);
                    self.assign_output_local(out);
                }
                true
            }
            "Some" if arguments.len() == 1 => {
                self.emit_operand_coerced(&arguments[0], VALTYPE_ANYREF);
                if let Some(out) = output {
                    self.force_output_local_for_stack_type(out, VALTYPE_ANYREF);
                    self.assign_output_local(out);
                }
                true
            }
            _ => false,
        }
    }

    /// `tuple_get_N(x)` →?GC `struct.get` 或线性内?field load?
    fn try_emit_tuple_get(&mut self, callee: &MirOperand, arguments: &[MirOperand], output: Option<MirValueRef>) -> bool {
        let MirOperand::Symbol(path) = callee
        else {
            return false;
        };
        let method_name = path.parts().last().map(|p| p.as_str()).unwrap_or("");
        if !is_tuple_get_stub_name(method_name) || arguments.len() != 1 {
            return false;
        }
        let index: usize = method_name.strip_prefix("tuple_get_").and_then(|s| s.parse().ok()).unwrap_or(0);
        let tuple_operand = &arguments[0];
        // ?clone 字段元数据，避免随后 emit_* 可变借用冲突?
        let Some(field_meta) = self.infer_tuple_layout(tuple_operand).and_then(|layout| {
            let type_index = self.gc_struct_type_indices.get(&layout.id).copied();
            let field = layout.fields.get(index)?.clone();
            let field_count = layout.fields.len();
            Some((type_index, field_count, field))
        })
        else {
            return false;
        };
        let (type_index, field_count, field) = field_meta;

        // GC struct 路径：仅 reference_locals（禁止对 i32 地址 ref.cast）?
        if let Some(type_index) = type_index {
            if let Some(local) = self.operand_reference_local(tuple_operand) {
                self.emit_local_get(local);
                self.emit_ref_cast_struct(type_index);
                let field_index = index.min(field_count.saturating_sub(1)) as u32;
                self.emit_struct_get(type_index, field_index);
                if let Some(out) = output {
                    let stack_ty = wasm_gc_field_type_byte_for_glue(&field.ty, self.js_glue_utf8_as_anyref);
                    self.force_output_local_for_stack_type(out, stack_ty);
                    self.assign_output_local(out);
                }
                else {
                    WasmOpcode::Drop.encode(&mut self.code);
                }
                return true;
            }
        }

        // 线性内存路径：?FieldGet(Value) 同构——嵌?Value 聚合只保?
        // `base+offset` 地址（i32），标量?`*.load`；禁止按 GC glue 把栈标成 anyref
        // （否?`local.set expected anyref, found i32.load`）?
        if let Some(base) = self.operand_address_local(tuple_operand) {
            self.emit_local_get(base);
            self.emit_i32_const(field.offset as i32);
            self.emit_i32_add();
            let field_is_value_type = self.storage_for_type(&field.ty) == StorageKind::Value;
            if let Some(out) = output {
                if field_is_value_type {
                    self.force_output_local_for_stack_type(out, VALTYPE_I32);
                    self.assign_output_local(out);
                }
                else {
                    self.emit_load_at_field(&field);
                    let store_ty = match field.ty {
                        NyarType::Float64 => VALTYPE_F64,
                        NyarType::Integer64 { .. } => VALTYPE_I64,
                        _ => VALTYPE_I32,
                    };
                    self.force_output_local_for_stack_type(out, store_ty);
                    self.assign_output_local(out);
                }
            }
            else if !field_is_value_type {
                self.emit_load_at_field(&field);
                WasmOpcode::Drop.encode(&mut self.code);
            }
            else {
                WasmOpcode::Drop.encode(&mut self.code);
            }
            return true;
        }
        false
    }

    /// Unite sum `FieldGet` 快捷路径，与 CLR `try_emit_unite_tagged_payload_get` 同构?
    ///
    /// unite sum ?wasm-gc structtype 固定?`[i32 tag, anyref payload]`?
    /// MIR 仍使?Fine/Fail 的语义字段名（`"tag"` / `"payload"` / `"value"` / `"error"`），
    /// 这些名字不在聚合布局?fields 列表中，?FieldGet 路径?return，导?output 无赋值?
    ///
    /// 降低规则（void≠unit 约束）：
    /// - `"tag"` →?struct.get field 0 →?i32（discriminant?
    /// - `"payload"` / `"value"` / `"error"` →?struct.get field 1 →?anyref?
    ///   ?MIR 输出类型为标量（utf8/bool/i32），再从 `[i32]` box 解箱?
    pub(super) fn try_emit_unite_field_get(
        &mut self,
        object: &MirOperand,
        field: &str,
        layout_id: Option<LayoutId>,
        output: Option<MirValueRef>,
    ) -> bool {
        let is_payload = matches!(field, "payload" | "value" | "error");
        let is_tag = field == "tag";
        if !is_payload && !is_tag {
            return false;
        }
        // 所?sum 共享 `[i32, anyref]`；解析失败时仍可用任一已登?type_index?
        let type_index =
            self.resolve_unite_sum_type_index_for_object(object, layout_id).or_else(|| self.gc_sum_type_indices.values().next().copied());
        let Some(type_index) = type_index
        else {
            return false;
        };
        let Some(object_local) = self.operand_reference_local(object).or_else(|| {
            // value_types 缺失时：若栈类型已是 anyref，仍允许?tag/payload?
            match object {
                MirOperand::Value(v) => {
                    let local = self.value_locals.get(v).copied().or_else(|| self.scalar_locals.get(v).copied())?;
                    let ty = self.wasm_local_value_type(local);
                    if ty == WASM_GC_ANYREF || ty == WASM_GC_EXTERNREF { Some(local) } else { None }
                }
                _ => None,
            }
        })
        else {
            // JVM/CLR 有时?payload-less enums 直接?i32 tag 用?
            // MIR 仍可能发 FieldGet(tag)；若 object 已是 i32，则 tag 即自身?
            if is_tag {
                if let Some(local) = self.operand_address_local(object).or_else(|| match object {
                    MirOperand::Value(v) => self.value_locals.get(v).copied().or_else(|| self.scalar_locals.get(v).copied()),
                    _ => None,
                }) {
                    if self.wasm_local_value_type(local) == VALTYPE_I32 {
                        self.emit_local_get(local);
                        if let Some(out) = output {
                            self.force_output_local_for_stack_type(out, VALTYPE_I32);
                            self.assign_output_local(out);
                        }
                        else {
                            WasmOpcode::Drop.encode(&mut self.code);
                        }
                        return true;
                    }
                }
            }
            return false;
        };
        // 防御：reference_locals 偶发挂到 i32 槽时禁止 ref.cast?
        if self.wasm_local_value_type(object_local) == VALTYPE_I32 {
            return false;
        }
        self.emit_local_get(object_local);
        self.emit_ref_cast_struct(type_index);
        if is_tag {
            self.emit_struct_get(type_index, 0);
            if let Some(out) = output {
                self.force_output_local_for_stack_type(out, VALTYPE_I32);
                self.assign_output_local(out);
            }
            else {
                WasmOpcode::Drop.encode(&mut self.code);
            }
            return true;
        }
        // payload / value / error
        self.emit_struct_get(type_index, 1);
        if let Some(out) = output {
            let wants_i32 = self.mir_fn.value_types.get(&out).is_some_and(|ty| self.unite_payload_wants_i32_unbox(ty));
            if wants_i32 {
                self.emit_unbox_i32_payload();
                self.force_output_local_for_stack_type(out, VALTYPE_I32);
            }
            else {
                self.force_output_local_for_stack_type(out, VALTYPE_ANYREF);
            }
            self.assign_output_local(out);
        }
        else {
            WasmOpcode::Drop.encode(&mut self.code);
        }
        true
    }

    /// Unite payload 是否应从 `[i32]` box 解箱为标?i32?
    fn unite_payload_wants_i32_unbox(&self, ty: &NyarType) -> bool {
        match ty {
            NyarType::Tuple(_)
            | NyarType::FixedArray { .. }
            | NyarType::Array(_)
            | NyarType::Named(_)
            | NyarType::Union(_)
            | NyarType::TraitObject(_)
            | NyarType::Float32
            | NyarType::Float64
            | NyarType::Integer64 { .. }
            | NyarType::Integer128 { .. }
            | NyarType::Unit
            | NyarType::Bottom => false,
            _ => wasm_gc_field_type_byte_for_glue(ty, self.js_glue_utf8_as_anyref) == VALTYPE_I32,
        }
    }

    /// 将操作数压成 unite payload（anyref）：引用原样；i32 装箱?`[i32]`?
    fn emit_operand_as_unite_payload(&mut self, operand: &MirOperand) {
        let actual = self.operand_wasm_stack_type(operand);
        // Aggregate/array references may be classified as the linear-memory
        // i32 fallback by semantic type lowering, while MIR has already
        // allocated a reference local. Preserve that object as the sum payload
        // instead of boxing the fallback integer.
        if self.operand_reference_local(operand).is_some() {
            self.emit_operand(operand);
            return;
        }
        if actual == WASM_GC_ANYREF || actual == WASM_GC_EXTERNREF {
            self.emit_operand_coerced(operand, VALTYPE_ANYREF);
            return;
        }
        if actual == VALTYPE_I32 {
            self.emit_box_i32_payload(operand);
            return;
        }
        // i64/f64 currently have no scalar payload boxing path; keep this
        // fail-closed rather than silently inventing a representation.
        self.emit_ref_null_anyref();
    }

    /// `i32` →?wasm-gc struct `[i32]`（anyref），?unite payload 槽使用?
    /// 栈效果：`[] →?[anyref]`。与 StructNew 同构：local.set + cast + struct.set + local.get?
    fn emit_box_i32_payload(&mut self, operand: &MirOperand) {
        let box_ty = self.gc_i32_box_type_index;
        self.emit_struct_new_default(box_ty);
        let tmp = self.alloc_anyref_local();
        self.emit_local_set(tmp);
        self.emit_local_get(tmp);
        self.emit_ref_cast_struct(box_ty);
        self.emit_operand(operand);
        self.emit_struct_set(box_ty, 0);
        self.emit_local_get(tmp);
    }

    /// 栈顶 anyref（i32 box）→ i32；null →?0?
    fn emit_unbox_i32_payload(&mut self) {
        let box_ty = self.gc_i32_box_type_index;
        let tmp = self.alloc_anyref_local();
        self.emit_local_tee(tmp);
        WasmOpcode::RefIsNull.encode(&mut self.code);
        // `if (result i32) i32.const 0 else local.get; ref.cast; struct.get end`
        self.code.push(0x04); // if
        self.code.push(VALTYPE_I32);
        self.emit_i32_const(0);
        self.code.push(0x05); // else
        self.emit_local_get(tmp);
        self.emit_ref_cast_struct(box_ty);
        self.emit_struct_get(box_ty, 0);
        self.code.push(0x0B); // end
    }

    /// p3 `write-via-stream`：utf8 线性句?→?`stream.new` / write / drop →?宿主?
    ///
    /// 顺序必须是：new →??reader 交给 write-via-stream →?write(writer) →?drop-writable →?drop future?
    /// ?write 再交 reader 会在 sync `stream.write` 上永久挂起?
    fn try_emit_wasi_cli_write_via_stream(
        &mut self,
        callee: &MirOperand,
        arguments: &[MirOperand],
        import_index: u32,
        output: Option<MirValueRef>,
    ) -> bool {
        if !self.wasi_mode || arguments.len() != 1 {
            return false;
        }
        let Some((module, field)) = self.host_imports.get(import_index as usize)
        else {
            return false;
        };
        if field != "write-via-stream" {
            return false;
        }
        if !(module.contains("stdout") || module.contains("stderr")) {
            return false;
        }
        let Some(stream_new) = self.find_host_import_index(module, "[stream-new-0]write-via-stream")
        else {
            return false;
        };
        let Some(stream_write) = self.find_host_import_index(module, "[stream-write-0]write-via-stream")
        else {
            return false;
        };
        let Some(stream_drop_w) = self.find_host_import_index(module, "[stream-drop-writable-0]write-via-stream")
        else {
            return false;
        };
        let Some(future_drop) = self.find_host_import_index(module, "[future-drop-readable-1]write-via-stream")
        else {
            return false;
        };
        let append_newline = self.callee_wants_console_newline(callee);

        // utf8 handle: [len:u32 LE][bytes…]
        let handle = self.alloc_i32_local();
        let len = self.alloc_i32_local();
        let bytes_ptr = self.alloc_i32_local();
        let pair = self.alloc_i64_local();
        let writer = self.alloc_i32_local();
        let reader = self.alloc_i32_local();
        let fut = self.alloc_i32_local();

        self.emit_operand_coerced(&arguments[0], VALTYPE_I32);
        self.emit_local_set(handle);

        self.emit_local_get(handle);
        encode_i32_load(2, 0, &mut self.code);
        self.emit_local_set(len);

        self.emit_local_get(handle);
        self.emit_i32_const(4);
        self.emit_i32_add();
        self.emit_local_set(bytes_ptr);

        self.emit_call(stream_new);
        self.emit_local_set(pair);

        // reader = low32, writer = high32（与 wit-bindgen raw_stream_new 同构?
        self.emit_local_get(pair);
        WasmOpcode::I32WrapI64.encode(&mut self.code);
        self.emit_local_set(reader);
        self.emit_local_get(pair);
        WasmOpcode::I64Const.encode(&mut self.code);
        encode_sleb128_i64(32, &mut self.code);
        self.code.push(0x88); // i64.shr_u（std-data opcode 枚举暂未收录?
        WasmOpcode::I32WrapI64.encode(&mut self.code);
        self.emit_local_set(writer);

        self.emit_local_get(reader);
        self.emit_call(import_index);
        self.emit_local_set(fut);

        self.emit_local_get(writer);
        self.emit_local_get(bytes_ptr);
        self.emit_local_get(len);
        self.emit_call(stream_write);
        WasmOpcode::Drop.encode(&mut self.code);

        if append_newline {
            // bump 1 字节写入 '\n'，再 stream.write（bump_allocate 在栈上留下对齐指针）
            self.bump_allocate(1, 1);
            let nl_ptr = self.alloc_i32_local();
            self.emit_local_tee(nl_ptr);
            self.emit_i32_const(0x0A);
            WasmOpcode::I32Store8.encode(&mut self.code);
            encode_uleb128(0u32, &mut self.code); // align=0
            encode_uleb128(0u32, &mut self.code); // offset=0
            self.emit_local_get(writer);
            self.emit_local_get(nl_ptr);
            self.emit_i32_const(1);
            self.emit_call(stream_write);
            WasmOpcode::Drop.encode(&mut self.code);
        }

        self.emit_local_get(writer);
        self.emit_call(stream_drop_w);

        self.emit_local_get(fut);
        self.emit_call(future_drop);

        if let Some(out) = output {
            // console write →?unit；void≠unit，用 anyref null 占位?
            self.force_output_local_for_stack_type(out, VALTYPE_ANYREF);
            self.emit_ref_null_anyref();
            self.assign_output_local(out);
        }
        true
    }

    fn find_host_import_index(&self, module: &str, field: &str) -> Option<u32> {
        self.host_imports.iter().position(|(m, f)| m == module && f == field).map(|index| index as u32)
    }

    fn callee_wants_console_newline(&self, callee: &MirOperand) -> bool {
        let MirOperand::Symbol(path) = callee
        else {
            return false;
        };
        let simple = path.parts().last().map(|part| part.as_str()).unwrap_or("");
        simple.contains("write_line") || simple.contains("error_line")
    }

    /// ?object 操作数的值类型或 layout_id 推断 unite sum 对应?wasm-gc type_index?
    fn resolve_unite_sum_type_index_for_object(&self, object: &MirOperand, layout_id: Option<LayoutId>) -> Option<u32> {
        // 优先：layout_id 对应?layout name 直接?gc_sum_type_indices?
        if let Some(lid) = layout_id {
            if let Some(layout) = self.ctx.layout_by_id(lid) {
                if let Some(idx) = self.resolve_gc_sum_type_index(&layout.name) {
                    return Some(idx);
                }
            }
        }
        // 退路：?object 操作数的 MIR 值类型推?sum_name?
        let vref = match object {
            MirOperand::Value(v) => *v,
            _ => return None,
        };
        let ty = self.mir_fn.value_types.get(&vref)?;
        let sum_name = Self::sum_type_name_from_nyar(ty)?;
        self.resolve_gc_sum_type_index(&sum_name)
    }

    fn infer_tuple_layout(&self, operand: &MirOperand) -> Option<&AggregateLayout> {
        let vref = match operand {
            MirOperand::Value(vref) => *vref,
            _ => return None,
        };
        let ty = self.mir_fn.value_types.get(&vref)?;
        self.ctx.layout_for_value_type(ty)
    }

    /// FieldGet ?layout_id 时：?object ?MIR 值类型恢复聚合布局（对?CLR）?
    pub(super) fn infer_aggregate_layout_for_operand(&self, operand: &MirOperand) -> Option<&AggregateLayout> {
        let vref = match operand {
            MirOperand::Value(vref) => *vref,
            _ => return None,
        };
        let ty = self.mir_fn.value_types.get(&vref)?;
        self.ctx.layout_for_value_type(ty).or_else(|| {
            // Named 可能?sum；sum ?AggregateLayout，但 FieldGet tag/payload 已由 unite 路径处理?
            // 此处仅覆?VonToken / VonParsedValue 等真实聚合?
            Self::sum_type_name_from_nyar(ty).and_then(|name| self.ctx.layout_by_type_name(&name))
        })
    }

    /// `is_null(x)` →?ref.is_null?
    fn try_emit_is_null(&mut self, callee: &MirOperand, arguments: &[MirOperand], output: Option<MirValueRef>) -> bool {
        let MirOperand::Symbol(path) = callee
        else {
            return false;
        };
        let simple = path.parts().last().map(|p| p.as_str()).unwrap_or("");
        if simple != "is_null" || arguments.len() != 1 {
            return false;
        }
        self.emit_operand_coerced(&arguments[0], VALTYPE_ANYREF);
        WasmOpcode::RefIsNull.encode(&mut self.code);
        if let Some(out) = output {
            self.force_output_local_for_stack_type(out, VALTYPE_I32);
            self.assign_output_local(out);
        }
        else {
            WasmOpcode::Drop.encode(&mut self.code);
        }
        true
    }
}
