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
        let callee_return;
        match dispatch {
            MirDispatchKind::Static => {
                if let Some(import_index) = self.resolve_callee_import_index(callee, arguments) {
                    if self.try_emit_wasi_cli_write_via_stream(callee, arguments, import_index, output) {
                        return;
                    }
                    callee_return = self.resolve_callee_return_type(callee, Some(import_index));
                    let param_types = self.resolve_callee_param_types(callee, Some(import_index));
                    self.emit_call_arguments(arguments, &param_types);
                    self.emit_call(import_index);
                }
                else if let Some(function_index) = self.resolve_callee_function_index(callee) {
                    callee_return = *self.return_types_by_function_index.get(&function_index)
                        .expect("WASM 函数下标缺少返回签名");
                    let param_types = self.param_types_by_function_index.get(&function_index)
                        .expect("WASM 函数下标缺少参数签名").clone();
                    self.emit_call_arguments(arguments, &param_types);
                    self.emit_call(function_index);
                }
                else {
                    panic!("WASM 静态调用缺少已解析 callable identity: {} callee={callee:?}", self.mir_fn.symbol);
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
                    panic!("WASM witness 调用缺少显式分派操作数: {}", self.mir_fn.symbol);
                }
                let type_index = self.resolve_callee_type_index(callee);
                self.emit_call_indirect(type_index, 0);
            }
            MirDispatchKind::Indirect => {
                panic!("WASM 缺少类型化间接调用计划: {}", self.mir_fn.symbol);
            }
        }
        self.emit_store_call_output(output, callee_return);
    }

    fn emit_store_call_output(&mut self, output: Option<MirValueRef>, callee_return: Option<u8>) {
        match output {
            Some(output) => {
                let return_type = callee_return.expect("WASM 有 SSA 结果的调用缺少返回合同");
                self.validate_output_slot_type(output, return_type);
                self.assign_output_local(output);
            }
            None if callee_return.is_some() => WasmOpcode::Drop.encode(&mut self.code),
            None => {}
        }
    }

    /// 编码阶段只验证预规划槽，不改变 SSA 表示。
    pub(super) fn validate_output_slot_type(&self, output: MirValueRef, stack_ty: u8) {
        let local = self.planned_value_local(output);
        assert_eq!(self.wasm_local_value_type(local), stack_ty, "WASM 编码结果与预规划槽冲突: %{} in {}", output.0, self.mir_fn.symbol);
    }

    pub(super) fn assign_output_local(&mut self, output: MirValueRef) {
        let local = self.planned_value_local(output);
        self.emit_local_set(local);
    }



    pub(super) fn resolve_callee_param_types(&self, callee: &MirOperand, import_index: Option<u32>) -> Vec<u8> {
        if let Some(index) = import_index {
            return self.import_param_types.get(index as usize).expect("WASM 导入缺少参数签名").clone();
        }
        let MirOperand::Item(instance) = callee else {
            panic!("WASM 普通调用缺少实例身份");
        };
        self.param_types_by_instance.get(instance).expect("WASM 实例缺少参数签名").clone()
    }

    pub(super) fn resolve_callee_return_type(&self, callee: &MirOperand, import_index: Option<u32>) -> Option<u8> {
        if let Some(index) = import_index {
            return *self.import_return_types.get(index as usize).expect("WASM 导入缺少返回签名");
        }
        let MirOperand::Item(instance) = callee else {
            panic!("WASM 普通调用缺少实例身份");
        };
        *self.return_types_by_instance.get(instance).expect("WASM 实例缺少返回签名")
    }

    pub(super) fn operand_wasm_stack_type(&self, operand: &MirOperand) -> u8 {
        match operand {
            MirOperand::Value(value) => self.wasm_local_value_type(self.planned_value_local(*value)),
            MirOperand::Constant(constant) => match constant {
                MirConstant::Utf8(_) => VALTYPE_I32,
                MirConstant::Utf16(_) => panic!("WASM lowering requires an explicit UTF-16 ABI contract"),
                MirConstant::Unit => WASM_GC_ANYREF,
                MirConstant::Float64(_) => VALTYPE_F64,
                MirConstant::Int(_) | MirConstant::Bool(_) => VALTYPE_I32,
            },
            MirOperand::Symbol(_) => panic!("WASM 值操作数缺少 SSA 身份"),
            MirOperand::Item(_) => panic!("WASM callable identity cannot be used as a value operand"),
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
        // i64 形参：anyref 经 `[i64]` box 解箱；i32 走 extend。
        if expected == VALTYPE_I64 && (actual == WASM_GC_ANYREF || actual == WASM_GC_EXTERNREF) {
            self.emit_operand(operand);
            self.emit_unbox_i64_payload();
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
            self.emit_box_i64_payload(operand);
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

    pub(super) fn resolve_callee_import_index(&self, callee: &MirOperand, _arguments: &[MirOperand]) -> Option<u32> {
        let MirOperand::Item(instance) = callee else { return None; };
        self.callee_import_index.get(instance).copied()
    }


    /// Lookup unite sum wasm-gc type_index by its resolved owner name.
    fn resolve_gc_sum_type_index(&self, sum_name: &str) -> Option<u32> {
        self.gc_sum_type_indices.get(sum_name).copied()
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
                    let local = self.value_locals.get(v).copied()?;
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
                    MirOperand::Value(v) => self.value_locals.get(v).copied(),
                    _ => None,
                }) {
                    if self.wasm_local_value_type(local) == VALTYPE_I32 {
                        self.emit_local_get(local);
                        if let Some(out) = output {
                            self.validate_output_slot_type(out, VALTYPE_I32);
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
                self.validate_output_slot_type(out, VALTYPE_I32);
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
                self.validate_output_slot_type(out, VALTYPE_I32);
            }
            else {
                self.validate_output_slot_type(out, VALTYPE_ANYREF);
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
        if actual == VALTYPE_I64 {
            self.emit_box_i64_payload(operand);
            return;
        }
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

    /// `i64` → wasm-gc struct `[i64]`（anyref），供泛型 `T` 数组槽使用。
    fn emit_box_i64_payload(&mut self, operand: &MirOperand) {
        let box_ty = self.gc_i64_box_type_index;
        self.emit_struct_new_default(box_ty);
        let tmp = self.alloc_anyref_local();
        self.emit_local_set(tmp);
        self.emit_local_get(tmp);
        self.emit_ref_cast_struct(box_ty);
        self.emit_operand(operand);
        self.emit_struct_set(box_ty, 0);
        self.emit_local_get(tmp);
    }

    /// 栈顶 anyref（i64 box）→ i64；null → 0。
    pub(super) fn emit_unbox_i64_payload(&mut self) {
        let box_ty = self.gc_i64_box_type_index;
        let tmp = self.alloc_anyref_local();
        self.emit_local_tee(tmp);
        WasmOpcode::RefIsNull.encode(&mut self.code);
        self.code.push(0x04);
        self.code.push(VALTYPE_I64);
        self.emit_i64_const(0);
        self.code.push(0x05);
        self.emit_local_get(tmp);
        self.emit_ref_cast_struct(box_ty);
        self.emit_struct_get(box_ty, 0);
        self.code.push(0x0B);
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
            self.validate_output_slot_type(out, VALTYPE_ANYREF);
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

}
