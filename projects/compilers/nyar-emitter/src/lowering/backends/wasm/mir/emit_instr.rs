//! Split from former monolithic wasm mir lowerer (ADR 0008).
#![allow(deprecated)]

#[allow(deprecated)]
use super::*;

#[allow(deprecated)]
impl<'a> WasmMirLowerer<'a> {
    pub(crate) fn emit_instruction(&mut self, instruction: &MirInstruction) {
        match &instruction.kind {
            MirInstructionKind::LoadConstant { constant, .. } => {
                // Int 常量在字节码层默认是 i32.const，但 plan 可能?value_types
                //（Integer64）把 output 分到 i64 local。必须按槽位 valtype 发射?
                // 否则 `i32.const -1; local.set <i64>` →?expected i64, found i32?
                    let slot_ty = self.output_scalar_slot_type(output);
                    self.emit_load_constant_for_slot(constant, slot_ty);
                    self.store_scalar(output);
                }
                else {
                    self.emit_load_constant(constant);
                }
            }
            MirInstructionKind::StoreVar { name, value, .. } => {
                // 根据 var local 类型决定 value 的实际发射占位类?
                //
                // 变量 local ?`plan_instruction` 首次分配后类型即固定?
                // 但同一变量可能在不?StoreVar 中被赋予不同 storage 语义的?
                // (典型场景:首次 StoreVar ?i32 值→分配 i32 local;
                // 后续 StoreVar ?`Constant(Unit)`/`Constant(String)` →?emit_operand ?ref.null)?
                // ?local ?i32 ?value 实际?ref.null,会触?
                // `local.set expected i32, found ref.null of type anyref`?
                //
                // 因此:对会?ref.null ?value(Symbol 未解?/ Constant String|Unit),
                // ?var local ?i32,改压 `i32.const 0` 保持类型一?
                // ?var local ?anyref,正常?ref.null?
                if let Some(local) = self.var_locals.get(name).copied() {
                    let local_is_anyref = self.wasm_local_value_type(local) == WASM_GC_ANYREF;
                    match value {
                        MirOperand::Symbol(path) if !self.var_locals.contains_key(&path.to_string()) => {
                            panic!(
                                "WASM emit fail-closed: StoreVar unresolved Symbol `{}` in `{}`; refuse invent 0/null (ADR 0008)",
                                path, self.mir_fn.symbol
                            );
                        }
                        MirOperand::Constant(MirConstant::Utf8(text)) => {
                            self.emit_load_constant(&MirConstant::Utf8(text.clone()));
                        }
                        MirOperand::Constant(MirConstant::Unit) => {
                            if local_is_anyref {
                                self.emit_load_constant(&MirConstant::Unit);
                            }
                            else {
                                panic!(
                                    "WASM emit fail-closed: StoreVar Unit into non-anyref local `{name}` in `{}` (ADR 0008)",
                                    self.mir_fn.symbol
                                );
                            }
                        }
                        _ => {
                            let value_is_reference = self.operand_is_reference_storage(value)
                                || self.operand_wasm_stack_type(value) == WASM_GC_ANYREF
                                || self.operand_wasm_stack_type(value) == WASM_GC_EXTERNREF;
                            let mut target_local = local;
                            let local_ty = self.wasm_local_value_type(local);
                            let local_is_anyref = local_ty == WASM_GC_ANYREF || local_ty == WASM_GC_EXTERNREF;
                            if local_is_anyref && !value_is_reference {
                                panic!(
                                    "WASM emit fail-closed: StoreVar anyref local `{name}` got non-reference value in `{}` (ADR 0008)",
                                    self.mir_fn.symbol
                                );
                            }
                            else if !local_is_anyref && value_is_reference {
                                if self.js_glue_utf8_as_anyref {
                                    // utf8 宿主字符串必须保?anyref local，禁?i32.const 0 降级?
                                    self.var_locals.remove(name);
                                    target_local = self.alloc_anyref_local();
                                    self.var_locals.insert(name.clone(), target_local);
                                    self.emit_operand(value);
                                }
                                else {
                                    // 值槽?i32 但源?anyref：迁?anyref local，禁?local.get→i32.set?
                                    self.var_locals.remove(name);
                                    target_local = self.alloc_anyref_local();
                                    self.var_locals.insert(name.clone(), target_local);
                                    self.emit_operand(value);
                                }
                            }
                            else if !local_is_anyref {
                                // 标量槽：按真?valtype coerce（Int 常量→i64 槽须 extend）?
                                self.emit_operand_coerced(value, local_ty);
                            }
                            else {
                                self.emit_operand(value);
                            }
                            self.emit_local_set(target_local);
                                let is_anyref = self.wasm_local_value_type(target_local) == WASM_GC_ANYREF;
                                if is_anyref {
                                    self.value_locals.remove(&output);
                                    self.scalar_locals.remove(&output);
                                    self.reference_locals.insert(output, target_local);
                                }
                                else {
                                    self.reference_locals.remove(&output);
                                    self.value_locals.insert(output, target_local);
                                }
                            }
                            return;
                        }
                    }
                    self.emit_local_set(local);
                    // StoreVar ?output vref 表示该变量绑定产生的 SSA 值，
                    // 后续读取?vref 时需要找到对应的 local?
                    // 若不在此处建?output →?local 映射，emit_operand 会落?
                    // placeholder 路径发射 i32.const 0，造成值传递断裂?
                    // 根据 var_local 的实际类型选择写入 value_locals ?reference_locals?
                    // 避免 emit_operand 读取?local 类型与期望栈类型不匹配?
                        let is_anyref = self.wasm_local_value_type(local) == WASM_GC_ANYREF;
                        if is_anyref {
                            // ?output 之前被误分配?value_locals，需移除避免冲突?
                            self.value_locals.remove(&output);
                            self.reference_locals.insert(output, local);
                        }
                        else {
                            // ?output 之前被误分配?reference_locals，需移除避免冲突?
                            self.reference_locals.remove(&output);
                            self.value_locals.insert(output, local);
                        }
                    }
                }
                else {
                    // 变量未分?local(防御性回退):?emit operand 保持栈平衡?
                    self.emit_operand(value);
                }
            }
            MirInstructionKind::Copy { source } => {
                // plan/emit 顺序不一致时，output 可能被误分配?i32?
                // ?source **真实栈类?*（含 param 槽位 valtype）校?output 槽?
                // 铁律：`emit_operand` 压栈类型必须与即?`local.set` 的槽 valtype 一致，
                // 否则出现 `local.set expected i32, found anyref`（func ?array 形参 Copy）?
                let source_stack_ty = self.operand_wasm_stack_type(source);
                self.emit_operand(source);
                    self.force_output_local_for_stack_type(output, source_stack_ty);
                    self.assign_output_local(output);
                }
                else {
                    WasmOpcode::Drop.encode(&mut self.code);
                }
            }
            MirInstructionKind::StructNew { storage, layout_id, fields, type_name, .. } => {
                else {
                    return;
                };
                let layout = self.resolve_layout(*layout_id, type_name);
                let use_gc_struct = self.struct_new_uses_gc_struct(*storage, *layout_id, type_name);
                if use_gc_struct {
                    let Some(type_index) = self.resolve_gc_struct_type_index(layout.id, &layout.name)
                    else {
                        self.trap_missing_gc_struct(layout.id, &layout.name, "StructNew");
                        return;
                    };
                    let local = if let Some(&local) = self.reference_locals.get(&output) {
                        local
                    }
                    else {
                        self.value_locals.remove(&output);
                        self.scalar_locals.remove(&output);
                        let local = self.alloc_struct_ref_local(type_index);
                        self.reference_locals.insert(output, local);
                        local
                    };
                    self.emit_struct_new_default(type_index);
                    self.emit_local_set(local);
                    for (field_name, value) in fields {
                        let Some(field_index) = layout.fields.iter().position(|item| item.name == *field_name)
                        else {
                            continue;
                        };
                        let field = &layout.fields[field_index];
                        self.emit_local_get(local);
                        self.emit_ref_cast_struct(type_index);
                        self.emit_operand_coerced(value, self.gc_struct_field_stack_type(field));
                        self.emit_struct_set(type_index, field_index as u32);
                    }
                }
                else {
                    match *storage {
                        StorageKind::Value => {
                            let Some(&local) = self.value_locals.get(&output)
                            else {
                                return;
                            };
                            self.bump_allocate(layout.size, layout.align);
                            self.emit_local_set(local);
                            for (field_name, value) in fields {
                                let Some(field) = layout.fields.iter().find(|item| item.name == *field_name)
                                else {
                                    continue;
                                };
                                self.emit_local_get(local);
                                self.emit_i32_const(field.offset as i32);
                                self.emit_i32_add();
                                self.emit_operand_coerced(value, self.field_store_stack_type(field));
                                self.emit_store_at_field(field);
                            }
                        }
                        StorageKind::Reference => {
                            let Some(&local) = self.reference_locals.get(&output)
                            else {
                                return;
                            };
                            let Some(type_index) = self.resolve_gc_struct_type_index(layout.id, &layout.name)
                            else {
                                self.trap_missing_gc_struct(layout.id, &layout.name, "StructNew/Reference");
                                return;
                            };
                            self.emit_struct_new_default(type_index);
                            self.emit_local_set(local);
                            for (field_name, value) in fields {
                                let Some(field_index) = layout.fields.iter().position(|item| item.name == *field_name)
                                else {
                                    continue;
                                };
                                let field = &layout.fields[field_index];
                                self.emit_local_get(local);
                                self.emit_ref_cast_struct(type_index);
                                self.emit_operand_coerced(value, self.gc_struct_field_stack_type(field));
                                self.emit_struct_set(type_index, field_index as u32);
                            }
                        }
                    }
                }
            }
            MirInstructionKind::TupleNew { fields, storage, layout_id, .. } => {
                else {
                    return;
                };
                match *storage {
                    StorageKind::Value => {
                        let Some(&local) = self.value_locals.get(&output)
                        else {
                            return;
                        };
                        let Some(layout_id) = layout_id
                        else {
                            return;
                        };
                        let Some(layout) = self.ctx.layout_by_id(*layout_id).cloned()
                        else {
                            return;
                        };
                        self.bump_allocate(layout.size, layout.align);
                        self.emit_local_set(local);
                        for (index, value) in fields.iter().enumerate() {
                            let Some(field) = layout.fields.get(index)
                            else {
                                continue;
                            };
                            self.emit_local_get(local);
                            self.emit_i32_const(field.offset as i32);
                            self.emit_i32_add();
                            self.emit_operand_coerced(value, self.field_store_stack_type(field));
                            self.emit_store_at_field(field);
                        }
                    }
                    StorageKind::Reference => {
                        // tuple 当前恒为值语?若到达此分支说明上游 MIR 不一致?
                        // ?unreachable trap 暴露问题,而非静默跳过?
                        encode_unreachable(&mut self.code);
                    }
                }
            }
            MirInstructionKind::ArrayNew { element_type, length, .. } => {
                // heap [T] 构?wasm-gc array.new_default <type_index> <length>?
                else {
                    return;
                };
                let Some(&local) = self.reference_locals.get(&output)
                else {
                    return;
                };
                let Some(type_index) = self.resolve_gc_array_type_index(element_type)
                else {
                    eprintln!("[wasm::mir] missing gc arraytype for element `{element_type:?}` at ArrayNew in `{}`", self.mir_fn.symbol);
                    encode_unreachable(&mut self.code);
                    return;
                };
                self.emit_operand(length);
                self.emit_array_new_default(type_index);
                self.emit_local_set(local);
            }
            MirInstructionKind::AggregateCopy { source, dest, layout_id } => {
                let Some(layout) = self.ctx.layout_by_id(*layout_id)
                else {
                    return;
                };
                // 判定是否?gc_struct 深拷贝路径?
                // 不能?`gc_struct_type_indices.contains_key` 单独触发——Value layout 也可?
                // ?Field*/AggregateCopy 扫描被登记，?Symbol 地址拷贝仍须 memory.copy?
                // ?StructNew storage=Reference / layout.storage=Value 的不一致对齐：
                // 只要 source/dest 已在 reference_locals（anyref），就必须深拷贝?
                let source_in_ref = if let MirOperand::Value(vref) = source { self.reference_locals.contains_key(vref) } else { false };
                let dest_in_ref = if let MirOperand::Value(vref) = dest { self.reference_locals.contains_key(vref) } else { false };
                let use_gc_struct = source_in_ref || dest_in_ref || layout.storage == StorageKind::Reference;
                eprintln!(
                    "[wasm::aggregate-copy-contract] fn={} layout={} semantic_storage={:?} source_in_ref={} dest_in_ref={} use_gc_struct={}",
                    self.mir_fn.symbol, layout.name, layout.storage, source_in_ref, dest_in_ref, use_gc_struct,
                );
                if use_gc_struct {
                    // 引用类型聚合:深拷贝。struct.new_with_default + 逐字?struct.get ?+ struct.set 目的?
                    // 注意:这是语义上的"值拷?,即产生新对象而非共享引用?
                    let Some(dest_local) = self.operand_reference_local(dest)
                    else {
                        return;
                    };
                    let Some(source_local) = self.operand_reference_local(source)
                    else {
                        return;
                    };
                    // Physical unite/Result rule: Value-storage payloads that
                    // already live in anyref slots keep payload identity via
                    // shallow anyref copy. Never ref.cast them into a sibling
                    // aggregate layout (that is the Fine/Fail cross-cast trap).
                    if layout.storage == StorageKind::Value {
                        self.emit_local_get(source_local);
                        self.emit_local_set(dest_local);
                        return;
                    }
                    let Some(type_index) = self.resolve_gc_struct_type_index(*layout_id, &layout.name)
                    else {
                        self.trap_missing_gc_struct(*layout_id, &layout.name, "AggregateCopy");
                        return;
                    };
                    // 先构?dest 对象(字段全默认??
                    // struct.new_default (0xFB 0x01) <type_index>
                    self.emit_struct_new_default(type_index);
                    self.emit_local_set(dest_local);
                    for (field_index, field) in layout.fields.iter().enumerate() {
                        // struct.set 期望?[ref, value]?
                        // 先压 dest_ref,再从 source 读字段值压?最?struct.set 写回 dest?
                        // 这样避免?temp local 的类型问?struct.get 可能返回 anyref)?
                        self.emit_local_get(source_local);
                        self.emit_ref_cast_struct(type_index);
                        // struct.get (0xFB 0x02) <type_index> <field_index>
                        self.emit_local_get(dest_local);
                        self.emit_ref_cast_struct(type_index);
                        self.emit_struct_get(type_index, field_index as u32);
                        // struct.set (0xFB 0x05) <type_index> <field_index>
                        self.emit_struct_set(type_index, field_index as u32);
                        let _ = field;
                    }
                }
                else {
                    // 值类型聚??memory.copy 复制线性内存字节?
                    match (self.operand_address_local(source), self.operand_address_local(dest)) {
                        (Some(source_local), Some(dest_local)) => {
                            let size = layout.size;
                            if size > 0 {
                                self.emit_local_get(dest_local);
                                self.emit_local_get(source_local);
                                self.emit_i32_const(size as i32);
                                self.emit_memory_copy();
                            }
                        }
                        _ => {}
                    }
                }
            }
            MirInstructionKind::FieldGet { object, field, storage, layout_id } => {
                // Unite sum fast-path: `Fine`/`Fail` payload ?tag 字段不在聚合布局中，
                // ?wasm-gc structtype [i32 tag, anyref payload] 已在 gc_sum_type_indices 登记?
                // ?CLR `try_emit_unite_tagged_payload_get` 同构——先于布局查找拦截?
                    return;
                }
                // CLR 同构：layout_id 缺失时从 object ?MIR 类型推断聚合布局
                // （enums/unite FieldGet(`tag`/`payload`) 与结构字段均可）?
                let layout_id_val = match *layout_id {
                    Some(id) => id,
                    None => {
                        if let Some(layout) = self.infer_aggregate_layout_for_operand(object) {
                            layout.id
                        }
                        else {
                            eprintln!("[wasm::mir] FieldGet missing layout_id in `{}`: field=`{field}` object={object:?}", self.mir_fn.symbol);
                            encode_unreachable(&mut self.code);
                            return;
                        }
                    }
                };
                let layout = self.resolve_layout(Some(layout_id_val), "");
                let use_gc_struct = self.struct_new_uses_gc_struct(*storage, Some(layout_id_val), &layout.name);
                if use_gc_struct && self.operand_reference_local(object).is_some() {
                    let Some(object_local) = self.operand_reference_local(object)
                    else {
                        return;
                    };
                    let Some(field_index) = layout.fields.iter().position(|item| item.name == *field)
                    else {
                        return;
                    };
                    let Some(type_index) = self.resolve_gc_struct_type_index(layout.id, &layout.name)
                    else {
                        self.trap_missing_gc_struct(layout.id, &layout.name, "FieldGet");
                        return;
                    };
                    self.emit_local_get(object_local);
                    self.emit_ref_cast_struct(type_index);
                    self.emit_struct_get(type_index, field_index as u32);
                        let field_ty = &layout.fields[field_index].ty;
                        // struct.get 返回类型?wasm 类型段定义决定（wasm_gc_field_type_byte），
                        // 必须按实际栈类型分配 local（i32/i64/f64/anyref），禁止一?i32?
                        let stack_ty = wasm_gc_field_type_byte_for_glue(field_ty, self.js_glue_utf8_as_anyref);
                        self.force_output_local_for_stack_type(output, stack_ty);
                        self.assign_output_local(output);
                    }
                    else {
                        WasmOpcode::Drop.encode(&mut self.code);
                    }
                }
                else {
                    let object_is_ref = self.operand_reference_local(object).is_some()
                        || self.operand_wasm_stack_type(object) == WASM_GC_ANYREF
                        || self.operand_wasm_stack_type(object) == WASM_GC_EXTERNREF;
                    match *storage {
                        StorageKind::Value => {
                            if object_is_ref {
                                // V 侧：`FieldGet` ?`!is_reference` ?fail-closed，不?GC struct?
                                // ?layout 已因 AggregateCopy/StructNew(Ref) 登记，且 object ?anyref?
                                // 仍按 GC 读字段（?seed AggregateCopy 深拷贝路径一致），禁止一?trap?
                                if let Some(type_index) = self.resolve_gc_struct_type_index(layout.id, &layout.name) {
                                    let Some(object_local) = self.operand_reference_local(object)
                                    else {
                                        self.trap_missing_gc_struct(layout_id_val, &layout.name, "FieldGet/Value+ref");
                                        return;
                                    };
                                    let Some(field_index) = layout.fields.iter().position(|item| item.name == *field)
                                    else {
                                        return;
                                    };
                                    self.emit_local_get(object_local);
                                    self.emit_ref_cast_struct(type_index);
                                    self.emit_struct_get(type_index, field_index as u32);
                                        let field_ty = &layout.fields[field_index].ty;
                                        let stack_ty = wasm_gc_field_type_byte_for_glue(field_ty, self.js_glue_utf8_as_anyref);
                                        self.force_output_local_for_stack_type(output, stack_ty);
                                        self.assign_output_local(output);
                                    }
                                    else {
                                        WasmOpcode::Drop.encode(&mut self.code);
                                    }
                                }
                                else {
                                    self.trap_missing_gc_struct(layout_id_val, &layout.name, "FieldGet/Value+ref");
                                }
                            }
                            else {
                                let Some(object_local) = self.operand_address_local(object)
                                else {
                                    return;
                                };
                                let field_layout = self.resolve_field_layout(field, *layout_id);
                                self.emit_local_get(object_local);
                                self.emit_i32_const(field_layout.offset as i32);
                                self.emit_i32_add();
                                let field_is_value_type = self.storage_for_type(&field_layout.ty) == StorageKind::Value;
                                    if field_is_value_type {
                                        let out_local = self.value_locals.get(&output).copied().unwrap_or_else(|| self.alloc_i32_local());
                                        self.emit_local_set(out_local);
                                        self.value_locals.insert(output, out_local);
                                    }
                                    else {
                                        self.emit_load_at_field(&field_layout);
                                        // 值类型聚合内的引用字段在线性内存中?i32 存放?
                                        let out_local = self
                                            .scalar_locals
                                            .get(&output)
                                            .copied()
                                            .or_else(|| self.value_locals.get(&output).copied())
                                            .unwrap_or_else(|| self.alloc_i32_local());
                                        self.emit_local_set(out_local);
                                        self.value_locals.insert(output, out_local);
                                    }
                                }
                                else if !field_is_value_type {
                                    self.emit_load_at_field(&field_layout);
                                    WasmOpcode::Drop.encode(&mut self.code);
                                }
                                else {
                                    WasmOpcode::Drop.encode(&mut self.code);
                                }
                            }
                        }
                        StorageKind::Reference => {
                            let Some(object_local) = self.operand_reference_local(object)
                            else {
                                panic!(
                                    "WASM emit fail-closed: FieldGet object is not a reference local in `{}` (ADR 0008)",
                                    self.mir_fn.symbol
                                );
                            };
                            let Some(layout_id) = layout_id
                            else {
                                panic!(
                                    "WASM emit fail-closed: FieldGet missing layout_id in `{}`; refuse invent null (ADR 0008)",
                                    self.mir_fn.symbol
                                );
                            };
                            let Some(layout) = self.ctx.layout_by_id(*layout_id)
                            else {
                                panic!(
                                    "WASM emit fail-closed: FieldGet unknown layout_id in `{}` (ADR 0008)",
                                    self.mir_fn.symbol
                                );
                            };
                            let Some(field_index) = layout.fields.iter().position(|item| item.name == *field)
                            else {
                                panic!(
                                    "WASM emit fail-closed: FieldGet unknown field `{field}` in `{}` (ADR 0008)",
                                    self.mir_fn.symbol
                                );
                            };
                            let Some(type_index) = self.resolve_gc_struct_type_index(*layout_id, &layout.name)
                            else {
                                self.trap_missing_gc_struct(*layout_id, &layout.name, "FieldGet");
                                return;
                            };
                            self.emit_local_get(object_local);
                            self.emit_ref_cast_struct(type_index);
                            self.emit_struct_get(type_index, field_index as u32);
                                let field_ty = &layout.fields[field_index].ty;
                                let stack_ty = wasm_gc_field_type_byte_for_glue(field_ty, self.js_glue_utf8_as_anyref);
                                self.force_output_local_for_stack_type(output, stack_ty);
                                self.assign_output_local(output);
                            }
                            else {
                                WasmOpcode::Drop.encode(&mut self.code);
                            }
                        }
                    }
                }
            }
            MirInstructionKind::FieldSet { object, field, value, storage, layout_id } => {
                match *storage {
                    StorageKind::Value => {
                        let Some(object_local) = self.operand_address_local(object)
                        else {
                            return;
                        };
                        let field_layout = self.resolve_field_layout(field, *layout_id);
                        self.emit_local_get(object_local);
                        self.emit_i32_const(field_layout.offset as i32);
                        self.emit_i32_add();
                        self.emit_operand_coerced(value, self.field_store_stack_type(&field_layout));
                        self.emit_store_at_field(&field_layout);
                    }
                    StorageKind::Reference => {
                        let Some(object_local) = self.operand_reference_local(object)
                        else {
                            return;
                        };
                        let Some(layout_id) = layout_id
                        else {
                            return;
                        };
                        let Some(layout) = self.ctx.layout_by_id(*layout_id)
                        else {
                            return;
                        };
                        let Some(field_index) = layout.fields.iter().position(|item| item.name == *field)
                        else {
                            return;
                        };
                        let Some(type_index) = self.resolve_gc_struct_type_index(*layout_id, &layout.name)
                        else {
                            self.trap_missing_gc_struct(*layout_id, &layout.name, "FieldSet");
                            return;
                        };
                        let field_layout = self.resolve_field_layout(field, Some(*layout_id));
                        // struct.set 期望?[ref, value]?
                        self.emit_local_get(object_local);
                        self.emit_ref_cast_struct(type_index);
                        self.emit_operand_coerced(value, self.gc_struct_field_stack_type(&field_layout));
                        self.emit_struct_set(type_index, field_index as u32);
                    }
                }
            }
            MirInstructionKind::Call { callee, arguments } => {
                // ADR 0010: no intrinsic_opcode / dispatch / witness on Call.
            }
            MirInstructionKind::SumNew { sum_type, type_args, variant, payload_type, payload, .. } => {
            }
            MirInstructionKind::SumPayloadGet { sum_type, type_args, variant, payload_type, object, .. } => {
            }
            MirInstructionKind::SumVariantIs { sum_type, type_args, variant, object } => {
            }
            // pattern 无法 lowering：extractor ?resolved 或类型推断失败，
            // 运行?trap——emit wasm `unreachable` (0x00) 立即触发 trap?
            MirInstructionKind::PatternMatch { value, .. } => {
                eprintln!("[wasm::mir] PatternMatch trap in `{}`: value={:?}", self.mir_fn.symbol, value);
                encode_unreachable(&mut self.code);
            }
            other => {
                panic!(
                    "WASM emit fail-closed: unhandled Semantic MIR instruction in `{}`: {other:?}; refuse silent drop (ADR 0008)",
                    self.mir_fn.symbol
                );
            }
        }
    }

    /// Returns the smallest function type index in the type section.
    /// This is the first functype entry after `main_type` + any structtype/arraytype entries.
    pub(crate) fn first_function_type_index(&self) -> u32 {
        self.type_index_by_name.values().copied().min().unwrap_or(1)
    }

    pub(crate) fn struct_new_uses_gc_struct(&self, storage: MirStorageKind, layout_id: Option<LayoutId>, type_name: &str) -> bool {
        let Some(layout_id) = layout_id
        else {
            return false;
        };
        let layout = self.resolve_layout(Some(layout_id), type_name);
        if !self.gc_struct_type_indices.contains_key(&layout.id) {
            return false;
        }
        // Physical rule only: Reference storage or anyref-bearing fields.
        // No type-name special cases (WorkspaceAutoLinkResult-style bypasses
        // paper over ABI bugs and recreate Fine/Fail cross-casts elsewhere).
        if matches!(storage, StorageKind::Reference) {
            return true;
        }
        layout.fields.iter().any(|field| {
            if self.js_glue_utf8_as_anyref && is_js_glue_host_string_type(&field.ty) {
                return true;
            }
            if wasm_gc_field_type_byte_for_glue(&field.ty, self.js_glue_utf8_as_anyref) == WASM_GC_ANYREF {
                return true;
            }
            self.storage_for_type(&field.ty) == StorageKind::Reference
        })
    }

    pub(crate) fn resolve_layout(&self, layout_id: Option<LayoutId>, type_name: &str) -> AggregateLayout {
        if let Some(id) = layout_id {
            if let Some(layout) = self.ctx.layout_by_id(id) {
                return layout.clone();
            }
        }
        if let Some(layout) = self.ctx.layout_by_type_name(type_name).cloned() {
            return layout;
        }
        // 兜底：合成空 layout 而非 panic?
        // `Self` 等未解析类型名在 HIR→MIR 阶段若未被替换为具体类型?
        // panic 会中断整个编译，无法看到后续错误?
        // 合成?layout ?StructNew 创建空对象、FieldSet 跳过?
        // 编译可继续，便于诊断其他错误?
        panic!(
            "WASM semantic MIR contract violation: missing aggregate layout for type `{type_name}` (layout_id={layout_id:?}) in `{}`",
            self.mir_fn.symbol
        );
    }

    pub(crate) fn resolve_field_layout(&self, field: &str, layout_id: Option<LayoutId>) -> FieldLayout {
        let Some(layout_id) = layout_id
        else {
            return FieldLayout { name: field.to_string(), ty: NyarType::Unit, offset: 0, size: 0, align: 1 };
        };
        self.ctx.field_layout(layout_id, field).cloned().unwrap_or(FieldLayout {
            name: field.to_string(),
            ty: NyarType::Unit,
            offset: 0,
            size: 0,
            align: 1,
        })
    }

}
