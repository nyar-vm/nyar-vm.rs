#!/usr/bin/env python3
from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def patch_physical() -> None:
    path = ROOT / "src/lowering/features/physical_contract.rs"
    t = path.read_text(encoding="utf-8")
    t = t.replace(
        "use nyar_types::{\n"
        "    NyarType,\n"
        "    executable::{TextConversionSemantics, TextEncoding, TextProjectionBoundary},\n"
        "};",
        "use nyar_types::NyarType;",
    )
    old = (
        "/// A target-authorized text conversion extracted from Semantic MIR.  This is\n"
        "/// a backend-private planning fact, never a default host string carrier.\n"
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n"
        "pub(crate) struct PhysicalTextProjection {\n"
        "    pub source_encoding: TextEncoding,\n"
        "    pub target_encoding: TextEncoding,\n"
        "    pub boundary: TextProjectionBoundary,\n"
        "}"
    )
    new = (
        "/// Backend-private text projection placeholder.\n"
        "///\n"
        "/// Semantic MIR no longer carries TextConvert / TextEncoding God fields (ADR 0011).\n"
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n"
        "pub(crate) struct PhysicalTextProjection {\n"
        "    pub authorized: bool,\n"
        "}"
    )
    if old not in t:
        raise SystemExit("PhysicalTextProjection not found")
    t = t.replace(old, new)

    old = (
        "            let ExecutableInstructionKind::Call { dispatch, callee, parameter_types, intrinsic_opcode, .. } = &instruction.kind\n"
        "            else {\n"
        "                continue;\n"
        "            };\n"
        "            if intrinsic_opcode.is_some() || !matches!(dispatch, crate::contracts::DispatchKind::Static) {\n"
        "                continue;\n"
        "            }\n"
        "            let location = format!(\"block {} instruction {instruction_index}\", block.id.0);\n"
        "            let Some(formals) = parameter_types\n"
        "            else {\n"
        "                return Err(PhysicalPlanError::new(\"BPHYS004\", function, location, \"static call requires an exact semantic formal signature\"));\n"
        "            };\n"
        "            let ExecutableOperand::Symbol(path) = callee\n"
        "            else {\n"
        "                return Err(PhysicalPlanError::new(\"BPHYS004\", function, location, \"static call requires an exact callee identity\"));\n"
        "            };\n"
        "            let callee = QualifiedName::new(path.parts().to_vec());\n"
        "            if executable.get_function(&callee).is_none() {\n"
        "                return Err(PhysicalPlanError::new(\n"
        "                    \"BPHYS004\",\n"
        "                    function,\n"
        "                    location,\n"
        "                    \"static call target is not an exact local semantic function\",\n"
        "                ));\n"
        "            }\n"
        "            let parameters = formals\n"
        "                .iter()\n"
        "                .map(|ty| physical_category(backend, ty, false, false, function, \"call parameter\"))\n"
        "                .collect::<Result<Vec<_>, _>>()?;\n"
        "            calls.insert((block.id.0, instruction_index), PhysicalCallContract { callee, parameters });"
    )
    new = (
        "            let ExecutableInstructionKind::Call { callee, arguments, .. } = &instruction.kind\n"
        "            else {\n"
        "                continue;\n"
        "            };\n"
        "            let location = format!(\"block {} instruction {instruction_index}\", block.id.0);\n"
        "            let ExecutableOperand::Symbol(path) = callee\n"
        "            else {\n"
        "                // Indirect / value callees are planned later via BackendPrivatePlan.\n"
        "                continue;\n"
        "            };\n"
        "            let callee = QualifiedName::new(path.parts().to_vec());\n"
        "            if executable.get_function(&callee).is_none() {\n"
        "                return Err(PhysicalPlanError::new(\n"
        "                    \"BPHYS004\",\n"
        "                    function,\n"
        "                    location,\n"
        "                    \"static call target is not an exact local semantic function\",\n"
        "                ));\n"
        "            }\n"
        "            let parameters = arguments\n"
        "                .iter()\n"
        "                .map(|arg| match arg {\n"
        "                    ExecutableOperand::Value(v) => function\n"
        "                        .value_types\n"
        "                        .get(v)\n"
        "                        .ok_or_else(|| PhysicalPlanError::new(\"BPHYS004\", function, location.clone(), \"call argument missing semantic type\")),\n"
        "                    _ => Ok(&NyarType::Unit),\n"
        "                })\n"
        "                .collect::<Result<Vec<_>, _>>()?\n"
        "                .into_iter()\n"
        "                .map(|ty| physical_category(backend, ty, false, false, function, \"call parameter\"))\n"
        "                .collect::<Result<Vec<_>, _>>()?;\n"
        "            calls.insert((block.id.0, instruction_index), PhysicalCallContract { callee, parameters });"
    )
    if old not in t:
        raise SystemExit("Call planning block not found")
    t = t.replace(old, new)

    m = re.search(r"fn collect_text_projections\([\s\S]*?\n\}\n\nfn boundary_matches_backend", t)
    if not m:
        raise SystemExit("collect_text_projections not found")
    replacement = (
        "fn collect_text_projections(\n"
        "    _function: &ExecutableFunction,\n"
        "    _backend: PhysicalBackend,\n"
        ") -> Result<(BTreeSet<ExecutableValueRef>, BTreeMap<ExecutableValueRef, PhysicalTextProjection>), PhysicalPlanError> {\n"
        "    // TextConvert deleted from Semantic MIR; Utf8/Utf16 identity lives on NyarType.\n"
        "    Ok((BTreeSet::new(), BTreeMap::new()))\n"
        "}\n\n"
        "fn boundary_matches_backend"
    )
    t = t[: m.start()] + replacement + t[m.end() - len("fn boundary_matches_backend") :]
    t = re.sub(
        r"fn boundary_matches_backend[\s\S]*?\n\}\n\nfn encoding_matches_type[\s\S]*?\n\}\n\n",
        "",
        t,
        count=1,
    )
    t = t.replace(
        "        // A managed text carrier is never inferred from the target. UTF-8 and\n"
        "        // UTF-16 remain distinct semantic values until a backend-private text\n"
        "        // projection contract explicitly selects an ABI or host carrier.\n"
        "        NyarType::Utf8 | NyarType::Utf16 if !text_authorized => {\n"
        "            return Err(PhysicalPlanError::new(\"BPHYS007\", function, location, \"text requires an explicit backend text projection contract\"));\n"
        "        }\n"
        "        NyarType::Utf8 | NyarType::Utf16 => PhysicalValueCategory::Reference,",
        "        // Utf8 vs Utf16 remain distinct NyarType identities (no TextConvert God opcode).\n"
        "        NyarType::Utf8 | NyarType::Utf16 => {\n"
        "            let _ = text_authorized;\n"
        "            PhysicalValueCategory::Reference\n"
        "        }",
    )
    path.write_text(t, encoding="utf-8", newline="\n")
    print("patched physical_contract.rs")


def patch_semantic() -> None:
    path = ROOT / "src/lowering/features/semantic_mir_contract.rs"
    t = path.read_text(encoding="utf-8")
    t = t.replace(
        "use nyar_types::{\n"
        "    Constant, IntrinsicOpcode, NyarType,\n"
        "    executable::{TextConversionSemantics, TextEncoding, TextProjectionBoundary},\n"
        "};",
        "use nyar_types::{Constant, NyarType};",
    )

    old = (
        "            if let ExecutableInstructionKind::StructNew { type_name, storage, layout_id, fields } = &instruction.kind {\n"
        "                let location = format!(\"block {} instruction {index}\", block.id.0);\n"
        "                let Some(layout_id) = layout_id\n"
        "                else {\n"
        "                    return Err(SemanticMirContractError {\n"
        "                        code: \"SMIR010\",\n"
        "                        function: function.symbol.clone(),\n"
        "                        location,\n"
        "                        detail: \"aggregate construction has no layout id\".to_string(),\n"
        "                    });\n"
        "                };\n"
        "                let Some(layout) = submission.aggregate_layouts.layouts.iter().find(|layout| layout.id == *layout_id)\n"
        "                else {\n"
        "                    return Err(SemanticMirContractError {\n"
        "                        code: \"SMIR010\",\n"
        "                        function: function.symbol.clone(),\n"
        "                        location,\n"
        "                        detail: \"aggregate construction references an unknown layout\".to_string(),\n"
        "                    });\n"
        "                };\n"
        "                let output_type = instruction.output.and_then(|output| function.value_types.get(&output));\n"
        "                let fields_match = fields.len() == layout.fields.len()\n"
        "                    && fields.iter().all(|(name, value)| {\n"
        "                        layout.fields.iter().find(|field| field.name == *name).is_some_and(\n"
        "                            |field| matches!(value, ExecutableOperand::Value(value) if function.value_types.get(value) == Some(&field.ty)),\n"
        "                        )\n"
        "                    });\n"
        "                if layout.name != *type_name\n"
        "                    || layout.storage != *storage\n"
        "                    || output_type != Some(&NyarType::Named(nyar::Identifier::new(type_name)))\n"
        "                    || !fields_match\n"
        "                {\n"
        "                    return Err(SemanticMirContractError {\n"
        "                        code: \"SMIR010\",\n"
        "                        function: function.symbol.clone(),\n"
        "                        location,\n"
        "                        detail: \"aggregate construction contract disagrees with declared layout metadata\".to_string(),\n"
        "                    });\n"
        "                }\n"
        "                continue;\n"
        "            }"
    )
    new = (
        "            if let ExecutableInstructionKind::StructNew { type_name, fields } = &instruction.kind {\n"
        "                let location = format!(\"block {} instruction {index}\", block.id.0);\n"
        "                let Some(layout) = submission.aggregate_layouts.layouts.iter().find(|layout| layout.name == *type_name)\n"
        "                else {\n"
        "                    return Err(SemanticMirContractError {\n"
        "                        code: \"SMIR010\",\n"
        "                        function: function.symbol.clone(),\n"
        "                        location,\n"
        "                        detail: \"aggregate construction references an unknown layout\".to_string(),\n"
        "                    });\n"
        "                };\n"
        "                let output_type = crate::contracts::instruction_primary_result(instruction)\n"
        "                    .and_then(|output| function.value_types.get(&output));\n"
        "                let fields_match = fields.len() == layout.fields.len()\n"
        "                    && fields.iter().all(|(name, value)| {\n"
        "                        layout.fields.iter().find(|field| field.name == *name).is_some_and(\n"
        "                            |field| matches!(value, ExecutableOperand::Value(value) if function.value_types.get(value) == Some(&field.ty)),\n"
        "                        )\n"
        "                    });\n"
        "                if output_type != Some(&NyarType::Named(nyar::Identifier::new(type_name))) || !fields_match {\n"
        "                    return Err(SemanticMirContractError {\n"
        "                        code: \"SMIR010\",\n"
        "                        function: function.symbol.clone(),\n"
        "                        location,\n"
        "                        detail: \"aggregate construction contract disagrees with declared layout metadata\".to_string(),\n"
        "                    });\n"
        "                }\n"
        "                continue;\n"
        "            }"
    )
    if old not in t:
        raise SystemExit("StructNew contract not found")
    t = t.replace(old, new)

    t = t.replace(
        "if instruction.output.and_then(|output| function.value_types.get(&output))",
        "if crate::contracts::instruction_primary_result(instruction).and_then(|output| function.value_types.get(&output))",
    )
    t = t.replace(
        "let output_type = instruction.output.and_then(|output| function.value_types.get(&output));",
        "let output_type = crate::contracts::instruction_primary_result(instruction).and_then(|output| function.value_types.get(&output));",
    )

    old = (
        "            let (field, storage, layout_id, value) = match &instruction.kind {\n"
        "                ExecutableInstructionKind::FieldGet { field, storage, layout_id, .. } => (field, storage, layout_id, None),\n"
        "                ExecutableInstructionKind::FieldSet { field, storage, layout_id, value, .. } => (field, storage, layout_id, Some(value)),\n"
        "                _ => continue,\n"
        "            };\n"
        "            let location = format!(\"block {} instruction {index}\", block.id.0);\n"
        "            let Some(layout_id) = layout_id\n"
        "            else {\n"
        "                return Err(SemanticMirContractError {\n"
        "                    code: \"SMIR010\",\n"
        "                    function: function.symbol.clone(),\n"
        "                    location,\n"
        "                    detail: \"aggregate field access has no layout id\".to_string(),\n"
        "                });\n"
        "            };\n"
        "            let Some(layout) = submission.aggregate_layouts.layouts.iter().find(|layout| layout.id == *layout_id)\n"
        "            else {\n"
        "                return Err(SemanticMirContractError {\n"
        "                    code: \"SMIR010\",\n"
        "                    function: function.symbol.clone(),\n"
        "                    location,\n"
        "                    detail: format!(\"aggregate field access references unknown layout {layout_id}\"),\n"
        "                });\n"
        "            };\n"
        "            if layout.storage != *storage {\n"
        "                return Err(SemanticMirContractError {\n"
        "                    code: \"SMIR010\",\n"
        "                    function: function.symbol.clone(),\n"
        "                    location,\n"
        "                    detail: \"aggregate field access storage differs from its declared layout\".to_string(),\n"
        "                });\n"
        "            }\n"
        "            let Some(declared) = layout.fields.iter().find(|candidate| candidate.name == *field)\n"
        "            else {\n"
        "                return Err(SemanticMirContractError {\n"
        "                    code: \"SMIR010\",\n"
        "                    function: function.symbol.clone(),\n"
        "                    location,\n"
        "                    detail: format!(\"aggregate layout {} has no field `{field}`\", layout.name),\n"
        "                });\n"
        "            };\n"
        "            if let Some(output) = instruction.output {\n"
        "                if function.value_types.get(&output) != Some(&declared.ty) {\n"
        "                    return Err(SemanticMirContractError {\n"
        "                        code: \"SMIR010\",\n"
        "                        function: function.symbol.clone(),\n"
        "                        location,\n"
        "                        detail: \"FieldGet result type differs from declared aggregate field type\".to_string(),\n"
        "                    });\n"
        "                }\n"
        "            }\n"
        "            if let Some(ExecutableOperand::Value(value)) = value {\n"
        "                if function.value_types.get(value) != Some(&declared.ty) {\n"
        "                    return Err(SemanticMirContractError {\n"
        "                        code: \"SMIR010\",\n"
        "                        function: function.symbol.clone(),\n"
        "                        location,\n"
        "                        detail: \"FieldSet value type differs from declared aggregate field type\".to_string(),\n"
        "                    });\n"
        "                }\n"
        "            }"
    )
    new = (
        "            let (object, field, value) = match &instruction.kind {\n"
        "                ExecutableInstructionKind::FieldGet { object, field, .. } => (object, field, None),\n"
        "                ExecutableInstructionKind::FieldSet { object, field, value, .. } => (object, field, Some(value)),\n"
        "                _ => continue,\n"
        "            };\n"
        "            let location = format!(\"block {} instruction {index}\", block.id.0);\n"
        "            let object_ty = match object {\n"
        "                ExecutableOperand::Value(v) => function.value_types.get(v),\n"
        "                _ => None,\n"
        "            };\n"
        "            let Some(NyarType::Named(name)) = object_ty\n"
        "            else {\n"
        "                return Err(SemanticMirContractError {\n"
        "                    code: \"SMIR010\",\n"
        "                    function: function.symbol.clone(),\n"
        "                    location,\n"
        "                    detail: \"aggregate field access requires a nominal object type\".to_string(),\n"
        "                });\n"
        "            };\n"
        "            let Some(layout) = submission.aggregate_layouts.layouts.iter().find(|layout| layout.name == name.as_str())\n"
        "            else {\n"
        "                return Err(SemanticMirContractError {\n"
        "                    code: \"SMIR010\",\n"
        "                    function: function.symbol.clone(),\n"
        "                    location,\n"
        "                    detail: format!(\"aggregate field access references unknown layout `{name}`\"),\n"
        "                });\n"
        "            };\n"
        "            let Some(declared) = layout.fields.iter().find(|candidate| candidate.name == *field)\n"
        "            else {\n"
        "                return Err(SemanticMirContractError {\n"
        "                    code: \"SMIR010\",\n"
        "                    function: function.symbol.clone(),\n"
        "                    location,\n"
        "                    detail: format!(\"aggregate layout {} has no field `{field}`\", layout.name),\n"
        "                });\n"
        "            };\n"
        "            if let Some(output) = crate::contracts::instruction_primary_result(instruction) {\n"
        "                if function.value_types.get(&output) != Some(&declared.ty) {\n"
        "                    return Err(SemanticMirContractError {\n"
        "                        code: \"SMIR010\",\n"
        "                        function: function.symbol.clone(),\n"
        "                        location,\n"
        "                        detail: \"FieldGet result type differs from declared aggregate field type\".to_string(),\n"
        "                    });\n"
        "                }\n"
        "            }\n"
        "            if let Some(ExecutableOperand::Value(value)) = value {\n"
        "                if function.value_types.get(value) != Some(&declared.ty) {\n"
        "                    return Err(SemanticMirContractError {\n"
        "                        code: \"SMIR010\",\n"
        "                        function: function.symbol.clone(),\n"
        "                        location,\n"
        "                        detail: \"FieldSet value type differs from declared aggregate field type\".to_string(),\n"
        "                    });\n"
        "                }\n"
        "            }"
    )
    if old not in t:
        raise SystemExit("FieldGet/Set contract not found")
    t = t.replace(old, new)

    old = (
        "            let ExecutableInstructionKind::Call { dispatch, callee, intrinsic_opcode, .. } = &instruction.kind\n"
        "            else {\n"
        "                continue;\n"
        "            };\n"
        "            if !matches!(dispatch, crate::contracts::DispatchKind::Static) || intrinsic_opcode.is_some() {\n"
        "                continue;\n"
        "            }"
    )
    new = (
        "            let ExecutableInstructionKind::Call { callee, .. } = &instruction.kind\n"
        "            else {\n"
        "                continue;\n"
        "            };"
    )
    if old not in t:
        raise SystemExit("static call resolution not found")
    t = t.replace(old, new)

    t = t.replace("if let Some(output) = instruction.output {", "if let Some(output) = crate::contracts::instruction_primary_result(instruction) {")
    t = t.replace(
        "                    if let Some(output) = instruction.output {\n"
        "                        if function.value_types.get(&output) != Some(&literal_type) {",
        "                    if let Some(output) = crate::contracts::instruction_primary_result(instruction) {\n"
        "                        if function.value_types.get(&output) != Some(&literal_type) {",
    )

    # Remove TextConvert validation block
    t = re.sub(
        r"            if let ExecutableInstructionKind::TextConvert \{[\s\S]*?\n            \}\n",
        "",
        t,
        count=1,
    )

    # Remove parameter_types / intrinsic_opcode call checks
    t = re.sub(
        r"            if let ExecutableInstructionKind::Call \{ arguments, parameter_types, \.\. \} = &instruction\.kind \{[\s\S]*?\n            \}\n"
        r"            if let ExecutableInstructionKind::Call \{ arguments, intrinsic_opcode: Some\(opcode\), \.\. \} = &instruction\.kind \{[\s\S]*?\n            \}\n",
        "",
        t,
        count=1,
    )

    # Remove validate_text_convert_contract and validate_intrinsic_call functions (keep until instruction_operands)
    t = re.sub(
        r"\nfn validate_text_convert_contract\([\s\S]*?\n\}\n\nfn validate_intrinsic_call\([\s\S]*?\n\}\n\nfn instruction_operands",
        "\nfn instruction_operands",
        t,
        count=1,
    )

    old = (
        "fn instruction_operands(kind: &ExecutableInstructionKind) -> Vec<&ExecutableOperand> {\n"
        "    use crate::contracts::InstructionKind::*;\n"
        "    match kind {\n"
        "        LoadConstant { .. } | LoadSymbol { .. } => Vec::new(),\n"
        "        Copy { source } => vec![source],\n"
        "        StoreVar { value, .. } => vec![value],\n"
        "        Call { callee, arguments, witness, effect, .. } => {\n"
        "            let mut values = vec![callee];\n"
        "            values.extend(arguments);\n"
        "            if let Some(witness) = witness {\n"
        "                values.push(witness);\n"
        "            }\n"
        "            if let Some(effect) = effect {\n"
        "                values.push(effect);\n"
        "            }\n"
        "            values\n"
        "        }\n"
        "        StructNew { fields, .. } => fields.iter().map(|(_, value)| value).collect(),\n"
        "        TupleNew { fields, .. } => fields.iter().collect(),\n"
        "        FixedArrayNew { items, .. } => items.iter().collect(),\n"
        "        AggregateCopy { source, dest, .. } => vec![source, dest],\n"
        "        FieldGet { object, .. } => vec![object],\n"
        "        FieldSet { object, value, .. } => vec![object, value],\n"
        "        SumNew { payload, .. } => payload.iter().collect(),\n"
        "        SumPayloadGet { object, .. } => vec![object],\n"
        "        TextConvert { value, .. } => vec![value],\n"
        "        PatternMatch { value, .. } => vec![value],\n"
        "        ArrayNew { length, .. } => vec![length],\n"
        "        ArrayLiteral { items, .. } => items.iter().collect(),\n"
        "    }\n"
        "}"
    )
    new = (
        "fn instruction_operands(kind: &ExecutableInstructionKind) -> Vec<&ExecutableOperand> {\n"
        "    use crate::contracts::InstructionKind::*;\n"
        "    match kind {\n"
        "        LoadConstant { .. } | LoadSymbol { .. } => Vec::new(),\n"
        "        Copy { source } => vec![source],\n"
        "        StoreVar { value, .. } => vec![value],\n"
        "        Call { callee, arguments } => {\n"
        "            let mut values = vec![callee];\n"
        "            values.extend(arguments);\n"
        "            values\n"
        "        }\n"
        "        StructNew { fields, .. } => fields.iter().map(|(_, value)| value).collect(),\n"
        "        TupleNew { fields, .. } => fields.iter().collect(),\n"
        "        AggregateCopy { source, dest, .. } => vec![source, dest],\n"
        "        FieldGet { object, .. } => vec![object],\n"
        "        FieldSet { object, value, .. } => vec![object, value],\n"
        "        SumNew { payload, .. } => payload.iter().collect(),\n"
        "        SumPayloadGet { object, .. } | SumVariantIs { object, .. } => vec![object],\n"
        "        PatternMatch { value, .. } => vec![value],\n"
        "        ArrayNew { length, initialization, .. } => {\n"
        "            let mut values = vec![length];\n"
        "            if let crate::contracts::ArrayInitialization::Fill(fill) = initialization {\n"
        "                values.push(fill);\n"
        "            }\n"
        "            values\n"
        "        }\n"
        "        ArrayFromElements { elements, .. } => elements.iter().collect(),\n"
        "        ArrayGet { array, index } => vec![array, index],\n"
        "        ArraySet { array, index, value } => vec![array, index, value],\n"
        "        ArrayLength { array } => vec![array],\n"
        "    }\n"
        "}"
    )
    if old not in t:
        raise SystemExit("instruction_operands not found")
    t = t.replace(old, new)

    # Export ArrayInitialization from contracts if needed - check contracts re-export
    path.write_text(t, encoding="utf-8", newline="\n")
    print("patched semantic_mir_contract.rs")


if __name__ == "__main__":
    patch_physical()
    patch_semantic()
