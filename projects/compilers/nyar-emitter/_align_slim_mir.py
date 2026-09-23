#!/usr/bin/env python3
"""One-shot aligner: slim InstructionKind for Wasm + shared contracts. Not committed."""
from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parent


def write(rel: str, text: str) -> None:
    path = ROOT / rel
    path.write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {rel} ({len(text)} bytes)")


def patch_slots() -> None:
    path = ROOT / "src/lowering/shared/executable/slots.rs"
    t = path.read_text(encoding="utf-8")
    t = t.replace(
        "let storage_ty = ty.clone().or_else(|| match &instruction.output {\n"
        "                        Some(value) => function.value_types.get(value).cloned(),\n"
        "                        None => None,\n"
        "                    });",
        "let storage_ty = ty.clone().or_else(|| {\n"
        "                        crate::contracts::instruction_primary_result(instruction)\n"
        "                            .and_then(|value| function.value_types.get(&value).cloned())\n"
        "                    });",
    )
    t = t.replace(
        "if let Some(output) = instruction.output {\n"
        "                    self.value_locals.insert(output, local);\n"
        "                }\n"
        "            }\n"
        "            _ => {\n"
        "                if let Some(output) = instruction.output {",
        "if let Some(output) = crate::contracts::instruction_primary_result(instruction) {\n"
        "                    self.value_locals.insert(output, local);\n"
        "                }\n"
        "            }\n"
        "            _ => {\n"
        "                if let Some(output) = crate::contracts::instruction_primary_result(instruction) {",
    )
    t = t.replace(
        "let storage_ty = ty.clone().or_else(|| instruction.output.and_then(|value| function.value_types.get(&value).cloned()));",
        "let storage_ty = ty.clone().or_else(|| {\n"
        "                    crate::contracts::instruction_primary_result(instruction)\n"
        "                        .and_then(|value| function.value_types.get(&value).cloned())\n"
        "                });",
    )
    t = t.replace(
        "if let Some(output) = instruction.output {\n"
        "                self.value_locals.insert(output, local);\n"
        "            }\n"
        "            return;\n"
        "        }\n"
        "        let Some(output) = instruction.output",
        "if let Some(output) = crate::contracts::instruction_primary_result(instruction) {\n"
        "                self.value_locals.insert(output, local);\n"
        "            }\n"
        "            return;\n"
        "        }\n"
        "        let Some(output) = crate::contracts::instruction_primary_result(instruction)",
    )
    old_storage = '''fn storage_for_instruction(kind: &ExecutableInstructionKind) -> ExecutableStorageKind {
    match kind {
        ExecutableInstructionKind::StructNew { storage, .. }
        | ExecutableInstructionKind::TupleNew { storage, .. }
        | ExecutableInstructionKind::FixedArrayNew { storage, .. } => *storage,
        ExecutableInstructionKind::FieldGet { storage, .. } | ExecutableInstructionKind::FieldSet { storage, .. } => *storage,
        _ => ExecutableStorageKind::Reference,
    }
}'''
    new_storage = '''fn storage_for_instruction(kind: &ExecutableInstructionKind) -> ExecutableStorageKind {
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
}'''
    if old_storage not in t:
        raise SystemExit("storage_for_instruction block not found")
    t = t.replace(old_storage, new_storage)

    old_jvm = '''fn jvm_field_types(ctx: &ExecutableLoweringContext<'_>, kind: &ExecutableInstructionKind, output_ty: &NyarType) -> Vec<NyarType> {
    match kind {
        ExecutableInstructionKind::StructNew { layout_id, type_name, .. } => {
            let layout = ctx.layout_by_id(layout_id.unwrap_or(0)).or_else(|| ctx.layout_by_type_name(type_name));
            match layout {
                Some(l) => l.fields.iter().flat_map(|field| ctx.flatten_value_type_slots(&field.ty)).collect(),
                None => ctx.flatten_value_type_slots(output_ty),
            }
        }
        ExecutableInstructionKind::TupleNew { layout_id, element_types, .. } => {
            let element_types: Vec<_> = element_types.clone();
            let layout = (*layout_id)
                .or_else(|| layout_id_for_nyar_type(&NyarType::Tuple(element_types.clone()), ctx.layouts))
                .and_then(|id| ctx.layout_by_id(id));
            match layout {
                Some(l) => l.fields.iter().flat_map(|field| ctx.flatten_value_type_slots(&field.ty)).collect(),
                None => element_types.iter().flat_map(|ty| ctx.flatten_value_type_slots(ty)).collect(),
            }
        }
        ExecutableInstructionKind::FixedArrayNew { layout_id, element_type, length, .. } => {
            let element_type = platform_type(element_type);
            let layout = (*layout_id)
                .or_else(|| {
                    layout_id_for_nyar_type(&NyarType::FixedArray { element: Box::new(element_type.clone()), length: *length }, ctx.layouts)
                })
                .and_then(|id| ctx.layout_by_id(id));
            match layout {
                Some(l) => l.fields.iter().flat_map(|field| ctx.flatten_value_type_slots(&field.ty)).collect(),
                None => (0..*length).flat_map(|_| ctx.flatten_value_type_slots(&element_type)).collect(),
            }
        }
        _ => jvm_type_field_types(ctx, output_ty),
    }
}'''
    new_jvm = '''fn jvm_field_types(ctx: &ExecutableLoweringContext<'_>, kind: &ExecutableInstructionKind, output_ty: &NyarType) -> Vec<NyarType> {
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
}'''
    if old_jvm not in t:
        raise SystemExit("jvm_field_types block not found")
    t = t.replace(old_jvm, new_jvm)
    # layout_id_for_nyar_type / platform_type may become unused
    if "layout_id_for_nyar_type" not in t.split("fn jvm_field_types")[1]:
        t = t.replace("use nyar_types::layout_id_for_nyar_type;\n\n", "")
        t = t.replace(
            "lowering::{\n"
            "        clr_types::nyar_type_to_msil,\n"
            "        shared::executable::{ExecutableLoweringContext, collect_reachable_blocks, platform_type},\n"
            "    },",
            "lowering::{\n"
            "        clr_types::nyar_type_to_msil,\n"
            "        shared::executable::{ExecutableLoweringContext, collect_reachable_blocks},\n"
            "    },",
        )
    path.write_text(t, encoding="utf-8", newline="\n")
    print("patched slots.rs")


def patch_type_registry() -> None:
    path = ROOT / "src/lowering/backends/wasm/mir/type_registry.rs"
    t = path.read_text(encoding="utf-8")
    t = t.replace(
        "use super::{\n"
        "    ExecutableLoweringContext, IntrinsicOpcode, LayoutId, MirInstructionKind, MirOperand, NyarType, StorageKind,\n"
        "    representation::{is_js_glue_host_string_type, mir_storage_for_type, wasm_gc_field_type_byte_for_glue},\n"
        "};",
        "use super::{\n"
        "    ExecutableLoweringContext, LayoutId, MirInstructionKind, MirOperand, NyarType, StorageKind,\n"
        "    representation::{is_js_glue_host_string_type, mir_storage_for_type, wasm_gc_field_type_byte_for_glue},\n"
        "};",
    )
    old_collect = '''                match &instruction.kind {
                    MirInstructionKind::StructNew { layout_id, storage, .. } => {
                        // Physical rule: only Reference StructNew forces a GC
                        // structtype. Value aggregates stay linear / boxed.
                        if *storage == StorageKind::Reference {
                            if let Some(id) = layout_id {
                                ids.insert(*id);
                            }
                        }
                    }
                    MirInstructionKind::FieldGet { layout_id, .. } | MirInstructionKind::FieldSet { layout_id, .. } => {
                        if let Some(id) = layout_id {
                            ids.insert(*id);
                        }
                    }
                    MirInstructionKind::AggregateCopy { layout_id, .. } => {
                        ids.insert(*layout_id);
                    }
                    _ => {}
                }'''
    new_collect = '''                match &instruction.kind {
                    MirInstructionKind::StructNew { type_name, .. } => {
                        if let Some(layout) = ctx.layout_by_type_name(type_name) {
                            if layout.storage == StorageKind::Reference {
                                ids.insert(layout.id);
                            }
                        }
                    }
                    MirInstructionKind::FieldGet { object, .. } | MirInstructionKind::FieldSet { object, .. } => {
                        if let MirOperand::Value(vref) = object {
                            if let Some(ty) = view.function.value_types.get(vref) {
                                if let Some(layout) = ctx.layout_for_value_type(ty) {
                                    ids.insert(layout.id);
                                }
                            }
                        }
                    }
                    MirInstructionKind::AggregateCopy { source, dest, .. } => {
                        for operand in [source, dest] {
                            if let MirOperand::Value(vref) = operand {
                                if let Some(ty) = view.function.value_types.get(vref) {
                                    if let Some(layout) = ctx.layout_for_value_type(ty) {
                                        ids.insert(layout.id);
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }'''
    if old_collect not in t:
        raise SystemExit("collect_mir_reference_layout_ids match not found")
    t = t.replace(old_collect, new_collect)

    old_array = '''                    match &instruction.kind {
                        MirInstructionKind::ArrayNew { element_type, .. } | MirInstructionKind::ArrayLiteral { element_type, .. } => {
                            ensure(element_type, type_indices, &mut map);
                        }
                        MirInstructionKind::Call { intrinsic_opcode, arguments, .. } => {
                            let is_array_access = matches!(
                                intrinsic_opcode,
                                Some(IntrinsicOpcode::ArrayGet | IntrinsicOpcode::ArraySet | IntrinsicOpcode::ArrayPush)
                            );
                            if is_array_access {
                                if let Some(MirOperand::Value(receiver)) = arguments.first() {
                                    if let Some(NyarType::Array(element) | NyarType::FixedArray { element, .. }) =
                                        view.function.value_types.get(receiver)
                                    {
                                        ensure(element.as_ref(), type_indices, &mut map);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }'''
    new_array = '''                    match &instruction.kind {
                        MirInstructionKind::ArrayNew { array_type, .. } | MirInstructionKind::ArrayFromElements { array_type, .. } => {
                            match array_type {
                                NyarType::Array(element) | NyarType::FixedArray { element, .. } => {
                                    ensure(element.as_ref(), type_indices, &mut map);
                                }
                                _ => {}
                            }
                        }
                        MirInstructionKind::ArrayGet { array, .. }
                        | MirInstructionKind::ArraySet { array, .. }
                        | MirInstructionKind::ArrayLength { array } => {
                            if let MirOperand::Value(receiver) = array {
                                if let Some(NyarType::Array(element) | NyarType::FixedArray { element, .. }) =
                                    view.function.value_types.get(receiver)
                                {
                                    ensure(element.as_ref(), type_indices, &mut map);
                                }
                            }
                        }
                        _ => {}
                    }'''
    if old_array not in t:
        raise SystemExit("register_gc_array_types match not found")
    t = t.replace(old_array, new_array)
    path.write_text(t, encoding="utf-8", newline="\n")
    print("patched type_registry.rs")


def patch_calls_visibility() -> None:
    path = ROOT / "src/lowering/backends/wasm/mir/calls.rs"
    t = path.read_text(encoding="utf-8")
    for name in [
        "force_output_local_for_stack_type",
        "assign_output_local",
        "resolve_callee_param_types",
        "resolve_callee_return_type",
        "operand_wasm_stack_type",
        "emit_operand_coerced",
        "resolve_callee_import_index",
        "try_emit_unite_field_get",
        "infer_aggregate_layout_for_operand",
    ]:
        t = t.replace(f"    fn {name}(", f"    pub(super) fn {name}(")
    t = t.replace(
        "            MirDispatchKind::EffectHandler | MirDispatchKind::Indirect => {\n"
        "                eprintln!(\"[wasm::mir] unsupported dispatch in `{}`: callee={:?} dispatch={dispatch:?}\", self.mir_fn.symbol, callee);\n"
        "                self.emit_unresolved_call_placeholder(arguments, output);\n"
        "                value_already_on_stack = output.is_some();\n"
        "            }",
        "            MirDispatchKind::Indirect => {\n"
        "                eprintln!(\"[wasm::mir] unsupported Indirect dispatch in `{}`: callee={:?}\", self.mir_fn.symbol, callee);\n"
        "                self.emit_unresolved_call_placeholder(arguments, output);\n"
        "                value_already_on_stack = output.is_some();\n"
        "            }",
    )
    # try_emit_unite_field_get still takes layout_id Option — keep signature, callers pass None
    path.write_text(t, encoding="utf-8", newline="\n")
    print("patched calls.rs visibility + EffectHandler")


if __name__ == "__main__":
    patch_slots()
    patch_type_registry()
    patch_calls_visibility()
    print("phase1 ok")
