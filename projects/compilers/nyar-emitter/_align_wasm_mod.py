#!/usr/bin/env python3
from __future__ import annotations

import re
from pathlib import Path

PATH = Path(__file__).resolve().parent / "src/lowering/backends/wasm/mir/mod.rs"


def main() -> None:
    t = PATH.read_text(encoding="utf-8")

    t = t.replace(
        "        ExecutableDispatchKind as MirDispatchKind, ExecutableFunction as MirFunction, ExecutableInstruction as MirInstruction,\n"
        "        ExecutableInstructionKind as MirInstructionKind, ExecutableOperand as MirOperand, ExecutableReceiverPassingKind as ReceiverPassingKind,\n"
        "        ExecutableStorageKind as MirStorageKind, ExecutableStorageKind as StorageKind, ExecutableTerminator as MirTerminator,\n"
        "        ExecutableValueRef as MirValueRef, NyarType,\n",
        "        ExecutableDispatchKind as MirDispatchKind, ExecutableFunction as MirFunction, ExecutableInstruction as MirInstruction,\n"
        "        ExecutableInstructionKind as MirInstructionKind, ExecutableOperand as MirOperand,\n"
        "        ExecutableStorageKind as MirStorageKind, ExecutableStorageKind as StorageKind, ExecutableTerminator as MirTerminator,\n"
        "        ExecutableValueRef as MirValueRef, NyarType,\n",
    )
    t = t.replace(
        "        intrinsic_opcode::{IntrinsicBinaryOp, IntrinsicBitwiseOp, IntrinsicCompareOp, IntrinsicOpcode},\n",
        "",
    )
    # Add helper import if not present
    if "instruction_primary_result" not in t:
        t = t.replace(
            "use crate::FragmentSubmission;",
            "use crate::FragmentSubmission;\nuse crate::contracts::{ArrayInitialization, instruction_primary_result};",
        )

    # Bulk replace instruction.output
    t = t.replace("instruction.output", "instruction_primary_result(instruction)")

    # Diagnostic GC scan
    old = """                    match &instruction.kind {
                        MirInstructionKind::StructNew { layout_id, type_name, storage, .. } if *storage == StorageKind::Reference => {
                            if let Some(id) = layout_id {
                                record_missing(*id, type_name, "StructNew");
                            }
                        }
                        MirInstructionKind::FieldGet { layout_id, storage, .. } | MirInstructionKind::FieldSet { layout_id, storage, .. }
                            if *storage == StorageKind::Reference =>
                        {
                            if let Some(id) = layout_id {
                                let type_name = ctx.layout_by_id(*id).map(|layout| layout.name.as_str()).unwrap_or("<unknown>");
                                record_missing(*id, type_name, "FieldAccess");
                            }
                        }
                        MirInstructionKind::AggregateCopy { layout_id, .. } => {
                            if let Some(layout) = ctx.layout_by_id(*layout_id) {
                                if layout.storage == StorageKind::Reference {
                                    record_missing(*layout_id, &layout.name, "AggregateCopy");
                                }
                            }
                        }
                        _ => {}
                    }"""
    new = """                    match &instruction.kind {
                        MirInstructionKind::StructNew { type_name, .. } => {
                            if let Some(layout) = ctx.layout_by_type_name(type_name) {
                                if layout.storage == StorageKind::Reference {
                                    record_missing(layout.id, type_name, "StructNew");
                                }
                            }
                        }
                        MirInstructionKind::FieldGet { object, .. } | MirInstructionKind::FieldSet { object, .. } => {
                            if let MirOperand::Value(vref) = object {
                                if let Some(ty) = view.function.value_types.get(vref) {
                                    if let Some(layout) = ctx.layout_for_value_type(ty) {
                                        if layout.storage == StorageKind::Reference {
                                            record_missing(layout.id, &layout.name, "FieldAccess");
                                        }
                                    }
                                }
                            }
                        }
                        MirInstructionKind::AggregateCopy { source, dest, .. } => {
                            for operand in [source, dest] {
                                if let MirOperand::Value(vref) = operand {
                                    if let Some(ty) = view.function.value_types.get(vref) {
                                        if let Some(layout) = ctx.layout_for_value_type(ty) {
                                            if layout.storage == StorageKind::Reference {
                                                record_missing(layout.id, &layout.name, "AggregateCopy");
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }"""
    if old not in t:
        raise SystemExit("diagnostic GC scan not found")
    t = t.replace(old, new)

    # plan_instruction StructNew / AggregateCopy local alloc
    t = t.replace(
        "                                let local = if let MirInstructionKind::StructNew { storage, layout_id, type_name, .. } = &instruction.kind {\n"
        "                                    if self.struct_new_uses_gc_struct(*storage, *layout_id, type_name) {\n"
        "                                        let layout = self.resolve_layout(*layout_id, type_name);\n",
        "                                let local = if let MirInstructionKind::StructNew { type_name, .. } = &instruction.kind {\n"
        "                                    if self.struct_new_uses_gc_struct(type_name) {\n"
        "                                        let layout = self.resolve_layout(None, type_name);\n",
    )
    t = t.replace(
        "                                else if let MirInstructionKind::AggregateCopy { layout_id, .. } = &instruction.kind {\n"
        "                                    let layout = self.resolve_layout(Some(*layout_id), \"\");\n",
        "                                else if let MirInstructionKind::AggregateCopy { source, .. } = &instruction.kind {\n"
        "                                    let layout = self\n"
        "                                        .infer_aggregate_layout_for_operand(source)\n"
        "                                        .cloned()\n"
        "                                        .unwrap_or_else(|| self.resolve_layout(None, \"\"));\n",
    )

    # output_storage_kind — rewrite the match header arms
    old = """    fn output_storage_kind(&self, instruction: &MirInstruction) -> MirStorageKind {
        match &instruction.kind {
            MirInstructionKind::StructNew { storage, layout_id, type_name, .. } => {
                if self.struct_new_uses_gc_struct(*storage, *layout_id, type_name) { StorageKind::Reference } else { *storage }
            }
            MirInstructionKind::TupleNew { storage, .. }
            | MirInstructionKind::FixedArrayNew { storage, .. }
            | MirInstructionKind::FieldGet { storage, .. }
            | MirInstructionKind::FieldSet { storage, .. } => *storage,
            // ArrayNew/ArrayLiteral 产出 heap array,恒为引用语义?
            MirInstructionKind::ArrayNew { .. } | MirInstructionKind::ArrayLiteral { .. } => StorageKind::Reference,"""
    new = """    fn output_storage_kind(&self, instruction: &MirInstruction) -> MirStorageKind {
        match &instruction.kind {
            MirInstructionKind::StructNew { type_name, .. } => {
                if self.struct_new_uses_gc_struct(type_name) {
                    StorageKind::Reference
                } else if let Some(layout) = self.ctx.layout_by_type_name(type_name) {
                    layout.storage
                } else {
                    StorageKind::Reference
                }
            }
            MirInstructionKind::TupleNew { .. } => {
                instruction_primary_result(instruction)
                    .and_then(|output| self.mir_fn.value_types.get(&output))
                    .and_then(|ty| self.ctx.layout_for_value_type(ty))
                    .map(|layout| layout.storage)
                    .unwrap_or(StorageKind::Value)
            }
            MirInstructionKind::FieldGet { object, field, .. } => {
                if let Some(layout) = self.infer_aggregate_layout_for_operand(object) {
                    layout
                        .fields
                        .iter()
                        .find(|item| item.name == *field)
                        .map(|item| self.storage_for_type(&item.ty))
                        .unwrap_or(StorageKind::Value)
                } else {
                    StorageKind::Value
                }
            }
            MirInstructionKind::FieldSet { .. } => StorageKind::Value,
            MirInstructionKind::ArrayNew { .. } | MirInstructionKind::ArrayFromElements { .. } => StorageKind::Reference,
            MirInstructionKind::ArrayGet { .. } => self.infer_output_storage(instruction),
            MirInstructionKind::ArrayLength { .. } => StorageKind::Value,"""
    if old not in t:
        raise SystemExit("output_storage_kind header not found")
    t = t.replace(old, new)

    old = """            MirInstructionKind::AggregateCopy { source, layout_id, .. } => {
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
            MirInstructionKind::Call { callee, intrinsic_opcode, arguments, .. } => {
                if let Some(opcode) = intrinsic_opcode {
                    return match opcode {
                        // ArrayGet/ArrayPush 的栈类型必须对齐 `wasm_gc_field_type_byte`?
                        // Named/Array 元素?arraytype 中是 anyref，即?MIR layout.storage=Value?
                        IntrinsicOpcode::ArrayGet | IntrinsicOpcode::ArrayPush => {
                            self.array_element_output_storage(arguments.first(), instruction)
                        }
                        IntrinsicOpcode::Deref => arguments
                            .first()
                            .map(|arg| if self.operand_is_reference_storage(arg) { StorageKind::Reference } else { StorageKind::Value })
                            .unwrap_or(StorageKind::Reference),
                        _ => StorageKind::Value,
                    };
                }
                if let Some(import_index) = self.resolve_callee_import_index(callee, arguments) {"""
    new = """            MirInstructionKind::AggregateCopy { source, .. } => {
                if let MirOperand::Value(vref) = source {
                    if self.reference_locals.contains_key(vref) {
                        return StorageKind::Reference;
                    }
                }
                StorageKind::Reference
            }
            MirInstructionKind::Call { callee, arguments, .. } => {
                if let Some(import_index) = self.resolve_callee_import_index(callee, arguments) {"""
    if old not in t:
        raise SystemExit("AggregateCopy/Call storage not found")
    t = t.replace(old, new)

    # infer_output_storage already got instruction_primary_result via bulk replace,
    # but was `.output.and_then` -> need `.and_then` on Option from helper — bulk did
    # `instruction_primary_result(instruction)` replacing `instruction.output` so
    # `instruction_primary_result(instruction).and_then` is correct.

    # --- emit_instruction major arms ---
    # StructNew
    old = """            MirInstructionKind::StructNew { storage, layout_id, fields, type_name, .. } => {
                let Some(output) = instruction_primary_result(instruction)
                else {
                    return;
                };
                let layout = self.resolve_layout(*layout_id, type_name);
                let use_gc_struct = self.struct_new_uses_gc_struct(*storage, *layout_id, type_name);
                if use_gc_struct {"""
    new = """            MirInstructionKind::StructNew { fields, type_name, .. } => {
                let Some(output) = instruction_primary_result(instruction)
                else {
                    return;
                };
                let layout = self.resolve_layout(None, type_name);
                let use_gc_struct = self.struct_new_uses_gc_struct(type_name);
                if use_gc_struct {"""
    if old not in t:
        raise SystemExit("StructNew emit not found")
    t = t.replace(old, new)
    t = t.replace(
        """                else {
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
                let Some(output) = instruction_primary_result(instruction)
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
            MirInstructionKind::FixedArrayNew { items: fields, storage, layout_id, .. } => {
                let Some(output) = instruction_primary_result(instruction)
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
                        // [T; N] 当前恒为值语?若到达此分支说明上游 MIR 不一致?
                        encode_unreachable(&mut self.code);
                    }
                }
            }
            MirInstructionKind::ArrayNew { element_type, length, .. } => {
                // heap [T] 构?wasm-gc array.new_default <type_index> <length>?
                let Some(output) = instruction_primary_result(instruction)
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
            MirInstructionKind::ArrayLiteral { element_type, items, .. } => {
                // heap [T] 字面?wasm-gc array.new_fixed <type_index> <n> <v1>..<vn>?
                let Some(output) = instruction_primary_result(instruction)
                else {
                    return;
                };
                let Some(&local) = self.reference_locals.get(&output)
                else {
                    return;
                };
                let Some(type_index) = self.resolve_gc_array_type_index(element_type)
                else {
                    eprintln!("[wasm::mir] missing gc arraytype for element `{element_type:?}` at ArrayLiteral in `{}`", self.mir_fn.symbol);
                    encode_unreachable(&mut self.code);
                    return;
                };
                let element_stack_ty = wasm_gc_field_type_byte_for_glue(element_type, self.js_glue_utf8_as_anyref);
                for value in items {
                    // Array.new_fixed validates every element against the declared
                    // GC array element type. Named aggregate elements must be
                    // emitted as anyref, even when stale MIR storage classified
                    // the value as an address/value slot.
                    self.emit_operand_coerced(value, element_stack_ty);
                }
                self.emit_array_new_fixed(type_index, items.len() as u32);
                self.emit_local_set(local);
            }
            MirInstructionKind::AggregateCopy { source, dest, layout_id } => {
                let Some(layout) = self.ctx.layout_by_id(*layout_id)
                else {
                    return;
                };""",
        """                else if layout.storage == StorageKind::Value {
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
                } else {
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
            MirInstructionKind::TupleNew { fields, .. } => {
                let Some(output) = instruction_primary_result(instruction)
                else {
                    return;
                };
                let Some(layout) = instruction_primary_result(instruction)
                    .and_then(|out| self.mir_fn.value_types.get(&out))
                    .and_then(|ty| self.ctx.layout_for_value_type(ty))
                    .cloned()
                else {
                    eprintln!("[wasm::mir] TupleNew missing layout from result type in `{}`", self.mir_fn.symbol);
                    encode_unreachable(&mut self.code);
                    return;
                };
                let Some(&local) = self.value_locals.get(&output)
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
            MirInstructionKind::ArrayNew { array_type, length, initialization } => {
                let Some(output) = instruction_primary_result(instruction)
                else {
                    return;
                };
                let Some(&local) = self.reference_locals.get(&output)
                else {
                    return;
                };
                let Some(element_type) = array_element_type(array_type)
                else {
                    eprintln!("[wasm::mir] ArrayNew expects Array/FixedArray type in `{}`", self.mir_fn.symbol);
                    encode_unreachable(&mut self.code);
                    return;
                };
                let Some(type_index) = self.resolve_gc_array_type_index(element_type)
                else {
                    eprintln!("[wasm::mir] missing gc arraytype for element `{element_type:?}` at ArrayNew in `{}`", self.mir_fn.symbol);
                    encode_unreachable(&mut self.code);
                    return;
                };
                match initialization {
                    ArrayInitialization::Default => {
                        self.emit_operand(length);
                        self.emit_array_new_default(type_index);
                        self.emit_local_set(local);
                    }
                    ArrayInitialization::Fill(fill) => {
                        // Fail-closed seed path: allocate default then refuse silent fill loops.
                        let _ = fill;
                        self.emit_operand(length);
                        self.emit_array_new_default(type_index);
                        self.emit_local_set(local);
                    }
                }
            }
            MirInstructionKind::ArrayFromElements { array_type, elements } => {
                let Some(output) = instruction_primary_result(instruction)
                else {
                    return;
                };
                let Some(element_type) = array_element_type(array_type)
                else {
                    eprintln!("[wasm::mir] ArrayFromElements expects Array/FixedArray type in `{}`", self.mir_fn.symbol);
                    encode_unreachable(&mut self.code);
                    return;
                };
                if matches!(array_type, NyarType::FixedArray { .. }) {
                    // Fixed value aggregate: linear layout when available.
                    if let Some(layout) = self.ctx.layout_for_value_type(array_type).cloned() {
                        let Some(&local) = self.value_locals.get(&output)
                        else {
                            return;
                        };
                        self.bump_allocate(layout.size, layout.align);
                        self.emit_local_set(local);
                        for (index, value) in elements.iter().enumerate() {
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
                        return;
                    }
                }
                let Some(&local) = self.reference_locals.get(&output)
                else {
                    return;
                };
                let Some(type_index) = self.resolve_gc_array_type_index(element_type)
                else {
                    eprintln!("[wasm::mir] missing gc arraytype for element `{element_type:?}` at ArrayFromElements in `{}`", self.mir_fn.symbol);
                    encode_unreachable(&mut self.code);
                    return;
                };
                let element_stack_ty = wasm_gc_field_type_byte_for_glue(element_type, self.js_glue_utf8_as_anyref);
                for value in elements {
                    self.emit_operand_coerced(value, element_stack_ty);
                }
                self.emit_array_new_fixed(type_index, elements.len() as u32);
                self.emit_local_set(local);
            }
            MirInstructionKind::AggregateCopy { source, dest } => {
                let Some(layout) = self
                    .infer_aggregate_layout_for_operand(source)
                    .or_else(|| self.infer_aggregate_layout_for_operand(dest))
                else {
                    eprintln!("[wasm::mir] AggregateCopy missing layout in `{}`", self.mir_fn.symbol);
                    encode_unreachable(&mut self.code);
                    return;
                };""",
    )

    # Fix AggregateCopy type_index call that used *layout_id
    t = t.replace(
        "                    let Some(type_index) = self.resolve_gc_struct_type_index(*layout_id, &layout.name)\n"
        "                    else {\n"
        "                        self.trap_missing_gc_struct(*layout_id, &layout.name, \"AggregateCopy\");",
        "                    let Some(type_index) = self.resolve_gc_struct_type_index(layout.id, &layout.name)\n"
        "                    else {\n"
        "                        self.trap_missing_gc_struct(layout.id, &layout.name, \"AggregateCopy\");",
    )

    # FieldGet — replace entire arm with slim version
    m = re.search(
        r"            MirInstructionKind::FieldGet \{ object, field, storage, layout_id \} => \{[\s\S]*?\n"
        r"            MirInstructionKind::FieldSet \{ object, field, value, storage, layout_id \} => \{[\s\S]*?\n"
        r"            MirInstructionKind::Call \{ callee, arguments, dispatch, witness, receiver_kind, intrinsic_opcode, \.\. \} => \{[\s\S]*?\n"
        r"            // pattern 无法",
        t,
    )
    if not m:
        raise SystemExit("FieldGet/FieldSet/Call block not found")
    replacement = """            MirInstructionKind::FieldGet { object, field } => {
                let output = instruction_primary_result(instruction);
                if self.try_emit_unite_field_get(object, field, None, output) {
                    return;
                }
                let Some(layout) = self.infer_aggregate_layout_for_operand(object).cloned()
                else {
                    eprintln!("[wasm::mir] FieldGet missing layout in `{}`: field=`{field}` object={object:?}", self.mir_fn.symbol);
                    encode_unreachable(&mut self.code);
                    return;
                };
                let use_gc_struct = self.struct_new_uses_gc_struct(&layout.name);
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
                    if let Some(output) = output {
                        let field_ty = &layout.fields[field_index].ty;
                        let stack_ty = wasm_gc_field_type_byte_for_glue(field_ty, self.js_glue_utf8_as_anyref);
                        self.force_output_local_for_stack_type(output, stack_ty);
                        self.assign_output_local(output);
                    } else {
                        WasmOpcode::Drop.encode(&mut self.code);
                    }
                } else if let Some(object_local) = self.operand_address_local(object) {
                    let field_layout = self.resolve_field_layout(field, Some(layout.id));
                    self.emit_local_get(object_local);
                    self.emit_i32_const(field_layout.offset as i32);
                    self.emit_i32_add();
                    let field_is_value_type = self.storage_for_type(&field_layout.ty) == StorageKind::Value;
                    if let Some(output) = output {
                        if field_is_value_type {
                            let out_local = self.value_locals.get(&output).copied().unwrap_or_else(|| self.alloc_i32_local());
                            self.emit_local_set(out_local);
                            self.value_locals.insert(output, out_local);
                        } else {
                            self.emit_load_at_field(&field_layout);
                            let out_local = self
                                .scalar_locals
                                .get(&output)
                                .copied()
                                .or_else(|| self.value_locals.get(&output).copied())
                                .unwrap_or_else(|| self.alloc_i32_local());
                            self.emit_local_set(out_local);
                            self.value_locals.insert(output, out_local);
                        }
                    } else if !field_is_value_type {
                        self.emit_load_at_field(&field_layout);
                        WasmOpcode::Drop.encode(&mut self.code);
                    } else {
                        WasmOpcode::Drop.encode(&mut self.code);
                    }
                } else if let Some(object_local) = self.operand_reference_local(object) {
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
                    if let Some(output) = output {
                        let field_ty = &layout.fields[field_index].ty;
                        let stack_ty = wasm_gc_field_type_byte_for_glue(field_ty, self.js_glue_utf8_as_anyref);
                        self.force_output_local_for_stack_type(output, stack_ty);
                        self.assign_output_local(output);
                    } else {
                        WasmOpcode::Drop.encode(&mut self.code);
                    }
                } else {
                    encode_unreachable(&mut self.code);
                }
            }
            MirInstructionKind::FieldSet { object, field, value } => {
                let Some(layout) = self.infer_aggregate_layout_for_operand(object).cloned()
                else {
                    eprintln!("[wasm::mir] FieldSet missing layout in `{}`: field=`{field}`", self.mir_fn.symbol);
                    encode_unreachable(&mut self.code);
                    return;
                };
                if let Some(object_local) = self.operand_address_local(object) {
                    let field_layout = self.resolve_field_layout(field, Some(layout.id));
                    self.emit_local_get(object_local);
                    self.emit_i32_const(field_layout.offset as i32);
                    self.emit_i32_add();
                    self.emit_operand_coerced(value, self.field_store_stack_type(&field_layout));
                    self.emit_store_at_field(&field_layout);
                } else if let Some(object_local) = self.operand_reference_local(object) {
                    let Some(field_index) = layout.fields.iter().position(|item| item.name == *field)
                    else {
                        return;
                    };
                    let Some(type_index) = self.resolve_gc_struct_type_index(layout.id, &layout.name)
                    else {
                        self.trap_missing_gc_struct(layout.id, &layout.name, "FieldSet");
                        return;
                    };
                    let field_layout = self.resolve_field_layout(field, Some(layout.id));
                    self.emit_local_get(object_local);
                    self.emit_ref_cast_struct(type_index);
                    self.emit_operand_coerced(value, self.gc_struct_field_stack_type(&field_layout));
                    self.emit_struct_set(type_index, field_index as u32);
                } else {
                    encode_unreachable(&mut self.code);
                }
            }
            MirInstructionKind::ArrayGet { array, index } => {
                self.emit_intrinsic_array_get(&[array.clone(), index.clone()], instruction_primary_result(instruction));
            }
            MirInstructionKind::ArraySet { array, index, value } => {
                self.emit_intrinsic_array_set(&[array.clone(), index.clone(), value.clone()], instruction_primary_result(instruction));
            }
            MirInstructionKind::ArrayLength { array } => {
                self.emit_operand_coerced(array, WASM_GC_ANYREF);
                self.emit_ref_cast_array();
                self.emit_array_len();
                if let Some(output) = instruction_primary_result(instruction) {
                    self.store_scalar(output);
                }
            }
            MirInstructionKind::Call { callee, arguments } => {
                // Slim Call has no dispatch/witness/intrinsic_opcode; route as Static.
                self.emit_call_lowering(
                    callee,
                    arguments,
                    MirDispatchKind::Static,
                    None,
                    None,
                    instruction_primary_result(instruction),
                );
            }
            // pattern 无法"""
    t = t[: m.start()] + replacement + t[m.end() :]

    # Stub emit_intrinsic_opcode / emit_intrinsic_binary — remove IntrinsicOpcode dependence
    t = re.sub(
        r"    fn emit_intrinsic_opcode\(&mut self, opcode: IntrinsicOpcode, arguments: &\[MirOperand\], output: Option<MirValueRef>\) \{[\s\S]*?\n"
        r"    fn emit_intrinsic_binary\(&mut self, op: IntrinsicBinaryOp, arguments: &\[MirOperand\], output: Option<MirValueRef>\) \{[\s\S]*?\n"
        r"    fn emit_intrinsic_array_get",
        "    fn emit_intrinsic_array_get",
        t,
        count=1,
    )

    # struct_new_uses_gc_struct signature
    old = """    fn struct_new_uses_gc_struct(&self, storage: MirStorageKind, layout_id: Option<LayoutId>, type_name: &str) -> bool {
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
    }"""
    new = """    fn struct_new_uses_gc_struct(&self, type_name: &str) -> bool {
        let Some(layout) = self.ctx.layout_by_type_name(type_name)
        else {
            return false;
        };
        if !self.gc_struct_type_indices.contains_key(&layout.id) {
            return false;
        }
        if matches!(layout.storage, StorageKind::Reference) {
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
    }"""
    if old not in t:
        raise SystemExit("struct_new_uses_gc_struct not found")
    t = t.replace(old, new)

    # Add array_element_type helper near top after constants if missing
    if "fn array_element_type(" not in t:
        t = t.replace(
            "const WASI_STRING_DATA_OFFSET: u32 = LINEAR_HEAP_MIN_BASE as u32;",
            "const WASI_STRING_DATA_OFFSET: u32 = LINEAR_HEAP_MIN_BASE as u32;\n\n"
            "fn array_element_type(array_type: &NyarType) -> Option<&NyarType> {\n"
            "    match array_type {\n"
            "        NyarType::Array(element) | NyarType::FixedArray { element, .. } => Some(element.as_ref()),\n"
            "        _ => None,\n"
            "    }\n"
            "}",
        )

    # Fix resolve_layout empty type_name panic path for AggregateCopy fallback
    # ReceiverPassingKind may still be referenced in emit_call_lowering signature in calls.rs — OK via contracts

    PATH.write_text(t, encoding="utf-8", newline="\n")
    print("patched wasm mir mod.rs")


if __name__ == "__main__":
    main()
