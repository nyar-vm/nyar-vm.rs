//! Split from former monolithic wasm mir lowerer (ADR 0008).
#![allow(deprecated)]

#[allow(deprecated)]
use super::*;

#[allow(deprecated)]
impl<'a> WasmMirLowerer<'a> {
    pub(crate) fn plan_instruction(&mut self, instruction: &MirInstruction) {
        match &instruction.kind {
            MirInstructionKind::StoreVar { name, value, ty } => {
                if !self.var_locals.contains_key(name) {
                    // 变量 local 类型决策:
                    //
                    // emit 阶段?value 的发射取决于 value 本身,与显?`ty` 注解无关:
                    //   - `Constant(String/Unit)` →?`ref.null anyref`(引用语义);
                    //   - `Symbol`(未解? →?`ref.null anyref`(引用语义,?emit_operand fallback 一?;
                    //   - `Value(vref)` →??reference_locals/value_types 决定?
                    //
                    // ?`ty` 标注 Value ?value 实际是引用语?典型场景:
                    // StoreVar ?`Constant(String)` 存入声明?i32 的变?,
                    // plan 仍需分配 anyref local,否则 emit_operand ?ref.null
                    // ?local.set ?i32 local,触发
                    // `local.set expected i32, found ref.null of type anyref` 阻断自举?
                    //
                    // 因此:变量 local 的存储语?= `ty` 声明 OR value 实际语义,
                    // 二者只要其一为引?即分?anyref local?
                    let ty_says_reference = ty.as_ref().map(|ty| self.storage_for_type(ty) == StorageKind::Reference).unwrap_or(false);
                    let is_reference = ty_says_reference || self.operand_is_reference_storage(value);
                    let local = if is_reference {
                        self.alloc_anyref_local()
                    }
                    else {
                        let stack_ty = ty
                            .as_ref()
                            .map(|ty| wasm_param_value_type_for(self.ctx, ty, self.gc_struct_type_indices, self.js_glue_utf8_as_anyref))
                            .unwrap_or_else(|| self.operand_wasm_stack_type(value));
                        self.alloc_scalar_local_for_stack_type(stack_ty)
                    };
                    self.var_locals.insert(name.clone(), local);
                }
            }
            _ => {
                    let planned_storage = self.output_storage_kind(instruction);
                    // Nominal MIR storage can be stale for aggregates carrying GC
                    // references (notably constructor/call results).  Keep the
                    // output in a reference local whenever its ABI type is anyref;
                    // otherwise ArrayPush and field access would see an i32 slot
                    // and coerce the value to ref.null.
                    let storage = self
                        .mir_fn
                        .value_types
                        .get(&output)
                        .map(|ty| wasm_param_value_type_for(self.ctx, ty, self.gc_struct_type_indices, self.js_glue_utf8_as_anyref))
                        .filter(|vt| matches!(*vt, WASM_GC_ANYREF | WASM_GC_EXTERNREF))
                        .map(|_| StorageKind::Reference)
                        .unwrap_or(planned_storage);
                    match storage {
                        StorageKind::Value => {
                            if !self.value_locals.contains_key(&output) {
                                let stack_ty = self
                                    .mir_fn
                                    .value_types
                                    .get(&output)
                                    .map(|ty| wasm_param_value_type_for(self.ctx, ty, self.gc_struct_type_indices, self.js_glue_utf8_as_anyref))
                                    .unwrap_or_else(|| {
                                        panic!(
                                            "WASM emit fail-closed: missing value_types for scalar plan %{} in `{}` (ADR 0008)",
                                            output.0, self.mir_fn.symbol
                                        )
                                    });
                                let local = self.alloc_scalar_local_for_stack_type(stack_ty);
                                self.value_locals.insert(output, local);
                            }
                        }
                        StorageKind::Reference => {
                            if !self.reference_locals.contains_key(&output) {
                                // ?output 已在 `value_locals`（如块参数预分配?i32），
                                // 必须移除，否?`emit_operand` 会优先读?i32 local?
                                // ?`store_scalar`/Call 写入 anyref local，造成读写不一致?
                                self.value_locals.remove(&output);
                                let local = if let MirInstructionKind::StructNew { storage, layout_id, type_name, .. } = &instruction.kind {
                                    if self.struct_new_uses_gc_struct(*storage, *layout_id, type_name) {
                                        let layout = self.resolve_layout(*layout_id, type_name);
                                        if let Some(type_index) = self.resolve_gc_struct_type_index(layout.id, &layout.name) {
                                            self.alloc_struct_ref_local(type_index)
                                        }
                                        else {
                                            self.alloc_anyref_local()
                                        }
                                    }
                                    else {
                                        self.alloc_anyref_local()
                                    }
                                }
                                else if let MirInstructionKind::AggregateCopy { layout_id, .. } = &instruction.kind {
                                    let layout = self.resolve_layout(Some(*layout_id), "");
                                    if self.gc_struct_type_indices.contains_key(&layout.id) {
                                        if let Some(type_index) = self.resolve_gc_struct_type_index(layout.id, &layout.name) {
                                            self.alloc_struct_ref_local(type_index)
                                        }
                                        else {
                                            self.alloc_anyref_local()
                                        }
                                    }
                                    else {
                                        self.alloc_anyref_local()
                                    }
                                }
                                else {
                                    self.alloc_anyref_local()
                                };
                                self.reference_locals.insert(output, local);
                            }
                        }
                    }
                }
            }
        }
    }

    /// 判定指令输出的存储语义。值类型用线性内存地址 (i32 local),
    /// 引用类型?wasm-gc 对象引用 (anyref local)?
    ///
    /// ?`mir_fn.value_types` 缺少条目时，根据指令本身推断存储语义?
    /// 而非统一默认 `Reference`——否?`LoadConstant { Int(0) }` 这类标量输出
    /// 会被误分?anyref local，?`emit_load_constant` 发射 `i32.const`?
    /// 造成 `local.set expected anyref, found i32` 类型不匹配?
    pub(crate) fn output_storage_kind(&self, instruction: &MirInstruction) -> MirStorageKind {
        match &instruction.kind {
            MirInstructionKind::StructNew { storage, layout_id, type_name, .. } => {
                if self.struct_new_uses_gc_struct(*storage, *layout_id, type_name) { StorageKind::Reference } else { *storage }
            }
            MirInstructionKind::TupleNew { storage, .. }
            | MirInstructionKind::FieldGet { storage, .. }
            | MirInstructionKind::FieldSet { storage, .. } => *storage,
            // ArrayNew 产出 heap array,恒为引用语义?
            MirInstructionKind::ArrayNew { .. } => StorageKind::Reference,
            // 标量常量（Int/Bool/Float64）恒为值语义；String/Unit 为引用语义?
            // String/Unit ?`emit_load_constant` 固定?`ref.null anyref`,
            // ?output 必须分配 anyref local,直接返回 Reference,
            // 不依?`value_types` 推断(后者可能误标为 Value 导致类型不匹??
            MirInstructionKind::LoadConstant { constant, .. } => match constant {
                MirConstant::Utf8(_) => StorageKind::Value,
                MirConstant::Unit => StorageKind::Reference,
                _ => StorageKind::Value,
            },
            // Copy 的输出存储语义应跟随 source,而非独立?value_types 推断?
            // MIR 类型推断可能?Copy ?output 标记为值类型（i32），
            // ?source 实际?anyref（如来自返回引用类型?Call）?
            // ?output 被分?i32 local,emit_operand ?anyref ?store_scalar ?i32,
            // 触发 `local.set expected i32, found anyref` 类型错误阻断自举?
            MirInstructionKind::Copy { source } => match source {
                MirOperand::Value(vref) if self.reference_locals.contains_key(vref) => StorageKind::Reference,
                MirOperand::Value(_) if self.operand_is_reference_storage(source) => StorageKind::Reference,
                // 未解?Symbol ?emit_operand fallback ?ref.null anyref,
                // ?Copy output 必须分配 anyref local,否则 store_scalar 写入 i32 local
                // 触发 `local.set expected i32, found ref.null` 类型错误?
                MirOperand::Symbol(path) if !self.var_locals.contains_key(&path.to_string()) => StorageKind::Reference,
                // WASI 轨：字符串字面量作为 i32 偏移量，不走 anyref 路径?
                MirOperand::Constant(MirConstant::Utf8(_)) if self.wasi_mode => StorageKind::Value,
                MirOperand::Constant(MirConstant::Utf8(_) | MirConstant::Unit) => StorageKind::Reference,
                _ => self.output_storage_from_contract(instruction),
            },
            // AggregateCopy 的输出存储语义应跟随 source / layout.storage?
            // AggregateCopy 对引用聚合做"深拷?:dest 必须分配 anyref local,
            // emit 阶段才能?reference_locals 中找?dest 并执?struct.new_default + 逐字段复制?
            // 不能直接?layout.storage——它?StructNew ?storage 字段可能不一?
            // (StructNew storage=Reference ?layout.storage=Value)?
            // 导致 dest 被分?i32 local,emit 阶段 operand_reference_local(dest) 返回 None 跳过赋?
            // dest 保持旧?null),后续 Call 传入 null 触发 ref.cast "illegal cast" trap?
            MirInstructionKind::AggregateCopy { source, layout_id, .. } => {
                if let MirOperand::Value(vref) = source {
                    if self.reference_locals.contains_key(vref) {
                        return StorageKind::Reference;
                    }
                }
                if let Some(layout) = self.ctx.layout_by_id(*layout_id) {
                    // AggregateCopy materializes a heap aggregate and its
                    // destination is later consumed through reference-local
                    // field access. A value-layout here would allocate an
                    // i32 slot, leave the reference destination unset, and
                    // make the next ref.cast trap with `illegal cast`.
                    let _ = layout;
                    return StorageKind::Reference;
                }
                StorageKind::Reference
            }
            MirInstructionKind::Call { callee, arguments, .. } => {
                let _ = (callee, arguments);
                if let Some(import_index) = self.resolve_callee_import_index(callee, arguments) {
                    if let Some(wasm_return) = self.resolve_callee_return_type(callee, Some(import_index)) {
                        return if matches!(wasm_return, VALTYPE_I32 | VALTYPE_I64 | VALTYPE_F64) {
                            StorageKind::Value
                        }
                        else {
                            StorageKind::Reference
                        };
                    }
                }
                if let MirOperand::Symbol(_) = callee {
                    if let Some(function_index) = self.resolve_callee_function_index(callee) {
                        if let Some(wasm_return) = self
                            .return_types_by_function_index
                            .get(&function_index)
                            .copied()
                            .flatten()
                            .or_else(|| self.resolve_callee_return_type(callee, None))
                        {
                            return if matches!(wasm_return, VALTYPE_I32 | VALTYPE_I64 | VALTYPE_F64) {
                                StorageKind::Value
                            }
                            else {
                                StorageKind::Reference
                            };
                        }
                    }
                    else if let Some(wasm_return) = self.resolve_callee_return_type(callee, None) {
                        return if matches!(wasm_return, VALTYPE_I32 | VALTYPE_I64 | VALTYPE_F64) {
                            StorageKind::Value
                        }
                        else {
                            StorageKind::Reference
                        };
                    }
                }
                self.output_storage_from_contract(instruction)
            }
            _ => self.output_storage_from_contract(instruction),
        }
    }

    /// 判定 operand 在当前函?lowering 上下文中的实际存储语义?
    ///
    /// 用于 `StoreVar` 在缺失类型注解时决定变量 local 类型:
    /// - `Value(vref)`: 优先查已分配?local(`reference_locals` 优先),
    ///   其次?`value_types` 推断,缺失时回退 `Reference`(安全??
    /// - `Constant`: String/Unit 为引用语?其余为值语义?
    /// - `Symbol`: 查对?var local 的实际类?`local_types` 数组)?
    pub(crate) fn operand_is_reference_storage(&self, operand: &MirOperand) -> bool {
        match operand {
            MirOperand::Value(vref) => {
                if self.reference_locals.contains_key(vref) {
                    return true;
                }
                // 入口 anyref 参数若被误挂?value_locals，不能因“在 value_locals”就判成标量?
                // 否则 StoreVar/Copy ?`local.get`(anyref) →?`local.set`(i32)?
                if let Some(local) = self.value_locals.get(vref).copied().or_else(|| self.scalar_locals.get(vref).copied()) {
                    let ty = self.wasm_local_value_type(local);
                    return ty == WASM_GC_ANYREF || ty == WASM_GC_EXTERNREF;
                }
                self.mir_fn
                    .value_types
                    .get(vref)
                    .map(|ty| type_is_wasm_gc_heap_reference(ty) || self.storage_for_type(ty) == StorageKind::Reference)
                    .unwrap_or_else(|| panic!(
                        "WASM emit fail-closed: missing value_types for reference-storage probe in `{}` (ADR 0008)",
                        self.mir_fn.symbol
                    ))
            }
            MirOperand::Constant(constant) => matches!(constant, MirConstant::Unit),
            MirOperand::Symbol(path) => {
                // ?`emit_operand` ?Symbol fallback 一?
                // 未解析的 Symbol 默认引用语义,避免 plan 分配 i32 local
                // ?emit ?ref.null 导致 `local.set expected i32, found ref.null`?
                self.var_locals.get(&path.to_string()).copied().map(|local| self.wasm_local_value_type(local) == WASM_GC_ANYREF).unwrap_or_else(|| panic!(
                    "WASM emit fail-closed: unresolved Symbol in reference-storage probe in `{}` (ADR 0008)",
                    self.mir_fn.symbol
                ))
            }
        }
    }

    /// ArrayGet/ArrayPush 输出存储：对?`register_gc_array_types` / `wasm_gc_field_type_byte_for_glue`?
    /// Named 类元素通常仍是 anyref；但宿主 utf8/utf16 ?Node 轨是 i32 句柄?
    pub(crate) fn array_element_output_storage(&self, receiver: Option<&MirOperand>, instruction: &MirInstruction) -> MirStorageKind {
        if let Some(receiver) = receiver {
            if let Some(element) = self.infer_array_element_type(receiver) {
                return if self.array_element_is_anyref(&element) { StorageKind::Reference } else { StorageKind::Value };
            }
        }
            return if self.array_element_is_anyref(out_ty) { StorageKind::Reference } else { StorageKind::Value };
        }
        panic!(
            "WASM emit fail-closed: array element output missing value_types / receiver element type in `{}` (ADR 0008)",
            self.mir_fn.symbol
        );
    }
    pub(crate) fn array_element_is_anyref(&self, element: &NyarType) -> bool {
        wasm_gc_field_type_byte_for_glue(element, self.js_glue_utf8_as_anyref) == WASM_GC_ANYREF
    }
}
