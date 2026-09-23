use crate::{
    contracts::ValueOrigin,
    executable_provider::{
        ExecutableBlock as MirBlock, ExecutableBlockRef as MirBlockRef, ExecutableConstant as MirConstant, ExecutableFunction as MirFunction,
        ExecutableInstruction as MirInstruction, ExecutableInstructionKind as MirInstructionKind, ExecutableOperand as MirOperand,
        ExecutableReceiverPassingKind as ReceiverPassingKind, ExecutableStorageKind as StorageKind, ExecutableTerminator as MirTerminator,
        ExecutableValueRef as MirValueRef, NyarType,
    },
    nyar_backend_clr::{MsilInstruction, MsilInstructionOperand, MsilMethodBody, MsilMethodRef, MsilMethodSignature, MsilOpcode, MsilType},
};

use super::{
    clr_types::nyar_type_to_msil,
    executable::{ExecutableLoweringContext, block_label, collect_reachable_blocks, slots::ExecutableSlotPlan},
    intrinsic_opcode::{IntrinsicBinaryOp, IntrinsicBitwiseOp, IntrinsicCompareOp, IntrinsicOpcode},
    pattern_matching_contract::validate_pattern_matching_invariants,
    sanitize_operation_symbol, sanitize_symbol,
    witness_abi::{is_injected_runtime_stub_symbol, is_tuple_get_stub_name, witness_slot_msil_signature},
};
use crate::{
    FragmentSubmission,
    lowering::shared::interop::{ClrHostMethodTarget, ClrHostReturnAdapt, clr_host_method_target},
};
use miette::{Result, miette};
use nyar::{ExternalImportLink, Identifier, NamePath, NyarFunctionType, QualifiedName};
use std::collections::BTreeMap;

pub(crate) fn lower_mir_function_to_msil(
    submission: &FragmentSubmission,
    operation: &QualifiedName,
    mir_fn: &MirFunction,
) -> Result<MsilMethodBody> {
    if let Err(error) = validate_pattern_matching_invariants(mir_fn) {
        debug_assert!(false, "pattern matching contract violation in function `{}` ({}): {error:?}", mir_fn.symbol, operation);
    }
    // Empty `Array.get` / `Array.set` host_contract bodies must become ordinal wrappers.
    // Without this, calls fall through to fuzzy `[clr]` name match (`HttpClient.GetStringAsync`).
    if let Some(kind) = array_ordinal_host_kind(operation) {
        if mir_function_is_empty_host(mir_fn) {
            return Ok(synthesize_array_ordinal_host_method(submission, operation, mir_fn, kind));
        }
    }
    let ctx = ExecutableLoweringContext::new(submission);
    let slots = ExecutableSlotPlan::plan_clr(&ctx, mir_fn);
    let mut lowerer =
        ClrMirLowerer { submission, ctx, mir_fn, operation, slots, instructions: Vec::new(), unite_payload_tag_ok: BTreeMap::new() };
    lowerer.emit_entry_parameter_prologue();
    if let Some(opcode) = mir_fn.intrinsic {
        if mir_fn.blocks.iter().all(|block| block.instructions.is_empty()) {
            let arguments = mir_fn
                .blocks
                .get(mir_fn.entry.0 as usize)
                .map(|block| block.parameters.iter().copied().map(MirOperand::Value).collect::<Vec<_>>())
                .unwrap_or_default();
            lowerer.emit_intrinsic_opcode(opcode, &arguments, None);
            lowerer.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None });
        }
        else {
            let block_order = collect_reachable_blocks(mir_fn);
            for block_id in &block_order {
                if let Some(block) = mir_fn.blocks.get(block_id.0 as usize) {
                    lowerer.emit_block(block)?;
                }
            }
        }
    }
    else {
        let block_order = collect_reachable_blocks(mir_fn);
        for block_id in &block_order {
            if let Some(block) = mir_fn.blocks.get(block_id.0 as usize) {
                lowerer.emit_block(block)?;
            }
        }
    }
    let return_type = nyar_type_to_msil(&mir_fn.return_type, &submission.aggregate_layouts);
    let parameter_types = mir_fn.param_types.iter().map(|ty| nyar_type_to_msil(ty, &submission.aggregate_layouts)).collect();
    let body = MsilMethodBody {
        method: MsilMethodRef {
            owner: None,
            name: sanitize_operation_symbol(operation),
            signature: MsilMethodSignature::new(return_type, parameter_types),
        },
        locals: lowerer.slots.local_types.clone(),
        instructions: lowerer.instructions,
        max_stack: 32,
        is_entry_point: false,
        is_async: false,
    };
    Ok(body)
}

struct ClrMirLowerer<'a> {
    submission: &'a FragmentSubmission,
    ctx: ExecutableLoweringContext<'a>,
    mir_fn: &'a MirFunction,
    operation: &'a QualifiedName,
    slots: ExecutableSlotPlan,
    instructions: Vec<MsilInstruction>,
    /// Unite `FieldGet payload` for Fine/Fail extractors: SSA payload → bool local that is
    /// `true` iff `scrutinee.tag` matched the expected variant (so `is_null` can mean miss).
    unite_payload_tag_ok: BTreeMap<MirValueRef, u16>,
}

impl<'a> ClrMirLowerer<'a> {
    fn emit_entry_parameter_prologue(&mut self) {
        let Some(entry_block) = self.mir_fn.blocks.get(self.mir_fn.entry.0 as usize)
        else {
            return;
        };
        for (index, param) in entry_block.parameters.iter().enumerate() {
            let Some(&local) = self.slots.block_param_locals.get(&(self.mir_fn.entry, index))
            else {
                continue;
            };
            self.emit_ldarg(index as u16);
            self.emit_stloc(local);
            self.slots.value_locals.insert(*param, local);
        }
    }

    fn emit_block(&mut self, block: &MirBlock) -> Result<()> {
        self.instructions.push(MsilInstruction { label: Some(block_label(block.id)), opcode: MsilOpcode::Nop, operand: None });
        for instruction in &block.instructions {
            self.emit_instruction(instruction)?;
        }
        self.emit_terminator(block);
        Ok(())
    }

    fn emit_instruction(&mut self, instruction: &MirInstruction) -> Result<()> {
        match &instruction.kind {
            MirInstructionKind::LoadConstant { constant, .. } => {
                self.emit_load_constant(constant);
                if let Some(output) = instruction.output {
                    let ty = match constant {
                        MirConstant::Int(_) => MsilType::Int32 { signed: true },
                        MirConstant::Float64(_) => MsilType::Float64,
                        MirConstant::Bool(_) => MsilType::Bool,
                        // CLR's `System.String` is a physical UTF-16 carrier.
                        // Both source encodings arrive here explicitly; no
                        // source type is recovered from this carrier.
                        MirConstant::Utf8(_) | MirConstant::Utf16(_) => MsilType::String,
                        MirConstant::Unit => MsilType::Object,
                    };
                    self.store_call_result(output, &ty);
                }
            }
            MirInstructionKind::StoreVar { name, value, .. } => {
                self.emit_operand(value);
                // Home must be the loop-carried / named block-param local when present so
                // later peeks that still hold the pre-match SSA binding see this store.
                let local = self.slots.var_locals.get(name).copied().or_else(|| {
                    self.mir_fn.values.iter().rev().find_map(|value_def| match &value_def.origin {
                        ValueOrigin::BlockParameter { name: param_name, .. } if param_name == name => {
                            self.slots.value_locals.get(&value_def.id).copied()
                        }
                        _ => None,
                    })
                });
                if let Some(local) = local {
                    self.slots.var_locals.entry(name.clone()).or_insert(local);
                    self.retype_local_from_operand(local, value);
                    self.emit_stloc(local);
                    if let Some(output) = instruction.output {
                        self.slots.value_locals.insert(output, local);
                    }
                }
            }
            MirInstructionKind::Copy { source } => {
                self.emit_operand(source);
                if let Some(output) = instruction.output {
                    self.store_typed_from_operand(output, source);
                }
            }
            MirInstructionKind::StructNew { type_name, storage, fields, layout_id, .. } => {
                // HIR `Self { … }` often survives as type_name "Self"; PE needs a real TypeDef owner.
                let type_name = self.resolve_struct_type_name(type_name, *layout_id);
                // Unite/enum arms written as `Integer { value: … }` lower as StructNew with the
                // *variant* name. PE TypeDefs exist only for the sum (`MsilInstructionOperand`),
                // so rewrite to tagged sum allocation — never `newobj Integer::.ctor`.
                let field_args: Vec<MirOperand> = fields.iter().map(|(_, value)| value.clone()).collect();
                let variant_path = NamePath::new(vec![Identifier::new(type_name.as_str())]);
                let expected_sum = self.expected_sum_for_variant_ctor(&variant_path, instruction.output, &field_args);
                if let Some((sum_name, tag, is_unite)) = self.resolve_sum_variant_ctor(&variant_path, expected_sum.as_deref(), &field_args) {
                    let has_aggregate = self.ctx.layout_by_type_name(&type_name).is_some();
                    // Unite variants are never standalone classes; valuetype enum arms without a
                    // real aggregate layout also go through sum allocation.
                    if is_unite || !has_aggregate {
                        self.emit_sum_variant_ctor(&sum_name, tag, is_unite, &field_args, instruction.output)?;
                        return Ok(());
                    }
                }
                let qualified = self.ctx.clr_qualified_type_name(&type_name);
                let local = instruction.output.and_then(|value| self.slots.value_locals.get(&value).copied()).expect("struct output local");
                if *storage == StorageKind::Value {
                    self.emit_ldloca(local);
                    self.instructions.push(MsilInstruction {
                        label: None,
                        opcode: MsilOpcode::Initobj,
                        operand: Some(MsilInstructionOperand::Type(qualified.clone())),
                    });
                    for (field_name, value) in fields {
                        self.emit_ldloca(local);
                        self.emit_operand(value);
                        self.instructions.push(MsilInstruction {
                            label: None,
                            opcode: MsilOpcode::Stfld,
                            operand: Some(MsilInstructionOperand::Field(qualified.clone(), field_name.clone())),
                        });
                    }
                }
                else {
                    self.instructions.push(MsilInstruction {
                        label: None,
                        opcode: MsilOpcode::Newobj,
                        operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                            owner: Some(qualified.clone()),
                            name: ".ctor".to_string(),
                            signature: MsilMethodSignature::new(MsilType::Void, Vec::new()),
                        })),
                    });
                    for (field_name, value) in fields {
                        self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Dup, operand: None });
                        self.emit_operand(value);
                        self.instructions.push(MsilInstruction {
                            label: None,
                            opcode: MsilOpcode::Stfld,
                            operand: Some(MsilInstructionOperand::Field(qualified.clone(), field_name.clone())),
                        });
                    }
                    self.emit_stloc(local);
                }
            }
            MirInstructionKind::AggregateCopy { source, dest, layout_id } => {
                let dest_local = self
                    .operand_local(dest)
                    .ok_or_else(|| miette!("CLR AggregateCopy dest has no local in `{}` (dest={:?})", self.operation, dest))?;
                let _ = layout_id;
                // Never use `cpblk` for managed valuetypes: ECMA-335 forbids it when the
                // type contains GC references (e.g. `BuildRequest` with `string` fields),
                // and the JIT reports `BadImageFormatException` (0x8007000B). A whole-value
                // `ldloc`/`stloc` is the legal move for both scalar and aggregate valuetypes.
                //
                // Source is not always an SSA local: MIR may copy from a Symbol (named var /
                // nullary sum variant) or Constant. Falling back to `emit_operand` matches
                // block-argument copies; panicking on `operand_local` aborted emit after
                // `partitions: 1` with no artifact (seen on `std.data.binary.pe`).
                if let Some(source_local) = self.operand_local(source) {
                    self.emit_ldloc(source_local);
                }
                else {
                    self.emit_operand_required(source, "AggregateCopy source")?;
                }
                // Retype dest from source: loop-carried SSA slots are sometimes planned as
                // `string` while the value flowing is `int32`/`uint32` (PEVerify Int32↔String).
                self.retype_local_from_operand(dest_local, source);
                self.emit_stloc(dest_local);
            }
            MirInstructionKind::FieldGet { object, field, storage, layout_id } => {
                // Unite sums store the active variant payload in `payload`; MIR still names
                // Fine/Fail fields `value` / `error`.
                let field_name = if matches!(field.as_str(), "value" | "error")
                    && (self.sum_type_name_for_operand(object).is_some()
                        || matches!(self.msil_type_of_operand(object), MsilType::Object)
                        // Erased Fine/Fail scrutinee may still be Named(Result) while MIR
                        // keeps the binding field as `value`/`error`.
                        || matches!(self.msil_type_of_operand(object), MsilType::Named(ref name) if {
                            self.submission.sum_types.iter().any(|sum| {
                                sum.is_unite
                                    && (sum.name == *name || Self::type_name_matches(&sum.name, name))
                                    && sum.variants.iter().any(|variant| {
                                        matches!(variant.name.as_str(), "Fine" | "Fail" | "Some" | "None")
                                    })
                            })
                        })) {
                    "payload".to_string()
                }
                else {
                    field.clone()
                };
                // CLR maps `utf8`/`Utf8Text` → `System.String`; there is no `_repr: [u8]` field.
                // Materialize UTF-8 bytes via `Encoding.UTF8.GetBytes` so `ch._repr⁅i⁆` (PE #US /
                // MSIL ldstr helpers) still lowers to `ldelem.u1` on a real `uint8[]`.
                if field_name == "_repr" {
                    if let Some(output) = instruction.output {
                        if self.try_emit_utf8_repr_bytes(object, output) {
                            return Ok(());
                        }
                    }
                }
                // Pattern-match extractors (`case Fine(x)`): MIR does FieldGet(payload) then
                // `is_null`. Unite discriminators are `tag`, not null payload — check tag first,
                // unbox only on match (else Fail(VonDiagnostic) → unbox.any VonParsedValue ICEs).
                // Never fall through to blind `ldfld payload` + narrow for unite sums.
                if field_name == "payload" {
                    if let Some(output) = instruction.output {
                        if self.try_emit_unite_tagged_payload_get(object, *storage, *layout_id, output)? {
                            return Ok(());
                        }
                        if self.sum_type_name_for_operand(object).is_some_and(|name| {
                            self.submission
                                .sum_types
                                .iter()
                                .any(|sum| (sum.name == name || Self::type_name_matches(&sum.name, &name)) && sum.is_unite)
                        }) {
                            return Err(miette!(
                                code = "nyar::clr::unite_payload_tag_unresolved",
                                help = "unite match 必须先 ldfld tag 再按臂 unbox payload",
                                "CLR 拒绝降低函数 `{}` 中无法解析 tag 的 unite payload FieldGet",
                                self.operation
                            ));
                        }
                    }
                }
                let owner = if *storage == StorageKind::Value {
                    if let Some(layout_id) = *layout_id {
                        match self.ctx.layout_by_id(layout_id) {
                            Some(layout) if layout.fields.iter().any(|f| f.name == field_name) => {
                                let is_unite_payload = matches!(field_name.as_str(), "payload" | "value" | "error")
                                    && !(layout.fields.iter().any(|f| f.name == "tag") && layout.fields.iter().any(|f| f.name == "payload"));
                                if is_unite_payload {
                                    // Stale MIR layout_id pointing at `AlgebraicTerm` (has `payload`
                                    // but no `tag`) must not own unite Fine/Fail extracts.
                                    self.resolve_reference_field_owner(&field_name, None, object)
                                }
                                else {
                                    self.ctx.clr_qualified_type_name(&layout.name)
                                }
                            }
                            Some(_) => {
                                // MIR layout_id may name the outer tuple while `field` belongs to
                                // an element type (`tuple.id` should be `ExecutableBlockRef.id`).
                                self.resolve_reference_field_owner(&field_name, None, object)
                            }
                            None => {
                                return Err(miette!(
                                    code = "nyar::clr::field_get_unknown_layout",
                                    "CLR 拒绝降低函数 `{}` 中未知 layout_id 的 FieldGet(`{}`)",
                                    self.operation,
                                    field_name
                                ));
                            }
                        }
                    }
                    else if let Some(sum_name) = self.sum_type_name_for_operand(object) {
                        // Unite/enum value-type FieldGet(`tag`/`payload`) may arrive without
                        // layout_id when MIR still carries only the sum type name.
                        self.ctx.clr_qualified_type_name(&sum_name)
                    }
                    else {
                        // Fall back to reference-style owner resolution (field name / inferred).
                        self.resolve_reference_field_owner(&field_name, *layout_id, object)
                    }
                }
                else {
                    self.resolve_reference_field_owner(&field_name, *layout_id, object)
                };
                // Unite TypeDefs only declare `tag`/`payload`. MIR Fine/Fail still names
                // bindings `value`/`error` — never emit `Option.error` / `Result.value` FieldRefs.
                let emit_field = self.unite_payload_field_name(&owner, &field_name);
                if *storage == StorageKind::Value {
                    let Some(object_local) = self.operand_local(object)
                    else {
                        return Err(miette!(
                            code = "nyar::clr::field_get_unmaterialized_object",
                            help = "value-type FieldGet 的 object 无本地槽（自由变量/未绑定 SSA）。应在源码中显式传参。",
                            "CLR 拒绝降低函数 `{}` 中无法物化的 value FieldGet（owner=`{}`, field=`{}`）",
                            self.operation,
                            owner,
                            emit_field
                        ));
                    };
                    // Boxed valuetype enum in an `object` slot (stale layout / pre-registration):
                    // `ldloca`+`ldfld tag` reads the object reference bits as a valuetype → wrong
                    // tags. Unbox into a temp first.
                    let local_is_object = matches!(self.slots.local_types.get(object_local as usize), Some(MsilType::Object));
                    let owner_is_valuetype = {
                        let simple = owner.rsplit(['.', '/']).next().unwrap_or(owner.as_str());
                        self.submission.aggregate_layouts.value_type_names.contains(simple)
                            || self.submission.aggregate_layouts.value_type_names.contains(&owner)
                            || self.clr_named_is_valuetype_sum(simple)
                    };
                    if local_is_object && owner_is_valuetype {
                        self.emit_ldloc(object_local);
                        self.instructions.push(MsilInstruction {
                            label: None,
                            opcode: MsilOpcode::UnboxAny,
                            operand: Some(MsilInstructionOperand::Type(owner.clone())),
                        });
                        let temp = self.alloc_temp_local(MsilType::Named(owner.clone()));
                        self.emit_stloc(temp);
                        self.emit_ldloca(temp);
                    }
                    else {
                        self.emit_ldloca(object_local);
                    }
                }
                else {
                    let pushed = self.emit_operand_expecting(object, None);
                    // Call results for `Apply(VonParseResult, T)` were historically `object`;
                    // `ldfld VonParseResult::*` requires the sum class on the stack.
                    // Only castclass when we actually pushed a value; a dangling castclass on
                    // an empty stack causes InvalidProgramException (CLR stack-height mismatch).
                    if !pushed {
                        return Err(miette!(
                            code = "nyar::clr::field_get_unmaterialized_object",
                            help = "FieldGet 的 object 操作数无法入栈（自由变量/未绑定 SSA）。应在源码中显式传参，勿依赖隐式捕获。",
                            "CLR 拒绝降低函数 `{}` 中无法物化的 FieldGet（owner=`{}`, field=`{}`）",
                            self.operation,
                            owner,
                            emit_field
                        ));
                    }
                    if matches!(self.msil_type_of_operand(object), MsilType::Object) && !owner.starts_with('[') {
                        self.instructions.push(MsilInstruction {
                            label: None,
                            opcode: MsilOpcode::Castclass,
                            operand: Some(MsilInstructionOperand::Type(owner.clone())),
                        });
                    }
                }
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Ldfld,
                    operand: Some(MsilInstructionOperand::Field(owner.clone(), emit_field.clone())),
                });
                if let Some(output) = instruction.output {
                    // Sum `payload` is typed `object` in the TypeDef; Fine/Fail bindings have a
                    // concrete MIR type — narrow with `unbox.any` / `castclass` from that evidence.
                    if emit_field == "payload" {
                        if let Some(out_ty) =
                            self.lookup_value_type(&output).map(|ty| nyar_type_to_msil(ty, &self.submission.aggregate_layouts))
                        {
                            self.emit_narrow_object_to_msil(&out_ty);
                            self.store_call_result(output, &out_ty);
                            return Ok(());
                        }
                    }
                    // Nested valuetype enum stored as `object` on the TypeDef (`VonToken.kind`
                    // before enum names were registered in `value_type_names`): unbox into the
                    // MIR valuetype slot so later `ldloca`+`ldfld tag` sees a real enum.
                    if let Some(out_ty) = self.lookup_value_type(&output).map(|ty| nyar_type_to_msil(ty, &self.submission.aggregate_layouts)) {
                        if let MsilType::Named(name) = &out_ty {
                            let is_valuetype_enum =
                                self.clr_named_is_valuetype_sum(name) || self.submission.aggregate_layouts.value_type_names.contains(name);
                            let owner_simple = owner.rsplit(['.', '/']).next().unwrap_or(owner.as_str());
                            let field_msil = self.ctx.layout_by_type_name(owner_simple).and_then(|layout| {
                                layout
                                    .fields
                                    .iter()
                                    .find(|f| f.name == emit_field)
                                    .map(|f| nyar_type_to_msil(&f.ty, &self.submission.aggregate_layouts))
                            });
                            if is_valuetype_enum && matches!(field_msil, Some(MsilType::Object) | None) {
                                // Only unbox when the FieldDef is (or was) object-typed. Embedded
                                // Named valuetype fields already push the valuetype.
                                if matches!(field_msil, Some(MsilType::Object)) {
                                    self.emit_narrow_object_to_msil(&out_ty);
                                    self.store_call_result(output, &out_ty);
                                    return Ok(());
                                }
                            }
                        }
                    }
                    self.store_to_value(output);
                }
            }
            MirInstructionKind::FieldSet { object, field, value, storage, layout_id } => {
                if *storage == StorageKind::Value {
                    let Some(object_local) = self.operand_local(object)
                    else {
                        return Err(miette!(
                            code = "nyar::clr::field_set_unmaterialized_object",
                            help = "value-type FieldSet 的 object 无本地槽（自由变量/未绑定 SSA）。",
                            "CLR 拒绝降低函数 `{}` 中无法物化的 value FieldSet（field=`{}`）",
                            self.operation,
                            field
                        ));
                    };
                    self.emit_ldloca(object_local);
                }
                else {
                    self.emit_operand(object);
                }
                self.emit_operand(value);
                let owner = if *storage == StorageKind::Value {
                    let Some(layout_id) = *layout_id
                    else {
                        return Err(miette!(
                            code = "nyar::clr::field_set_missing_layout",
                            help = "value-type FieldSet 需要 aggregate layout_id（MIR/executable 须携带布局元数据）",
                            "CLR 拒绝降低函数 `{}` 中缺少 layout_id 的 FieldSet（fail-closed；不 panic）",
                            self.operation
                        ));
                    };
                    self.ctx.layout_by_id(layout_id).map(|layout| self.ctx.clr_qualified_type_name(&layout.name)).ok_or_else(|| {
                        miette!(
                            code = "nyar::clr::field_set_unknown_layout",
                            "CLR 拒绝降低函数 `{}` 中未知 layout_id 的 FieldSet",
                            self.operation
                        )
                    })?
                }
                else {
                    self.resolve_reference_field_owner(field, *layout_id, object)
                };
                let emit_field = self.unite_payload_field_name(&owner, field);
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Stfld,
                    operand: Some(MsilInstructionOperand::Field(owner, emit_field)),
                });
            }
            MirInstructionKind::Call { callee, arguments, dispatch, witness, receiver_kind, parameter_types, intrinsic_opcode, .. } => {
                // The executable instruction may carry the opcode directly,
                // while synthetic frontend calls may carry it in the module's
                // structured intrinsic registry. Both are semantic metadata;
                // neither permits recovering meaning from a symbol spelling.
                let registry_opcode = match callee {
                    MirOperand::Symbol(path) => self
                        .submission
                        .intrinsics
                        .get(&path.to_string())
                        .or_else(|| path.parts().last().and_then(|name| self.submission.intrinsics.get(name.as_str())))
                        .copied(),
                    _ => None,
                };
                if let Some(opcode) = intrinsic_opcode.or(registry_opcode) {
                    self.emit_intrinsic_opcode(opcode, arguments, instruction.output);
                    return Ok(());
                }
                // Function-typed callee → `System.Delegate::DynamicInvoke`.
                // MIR should mark `DispatchKind::Indirect`; Static is also accepted when the
                // callee's type is already `NyarType::Function` (type-driven, not name-heuristic).
                if let Some(fn_ty) = self.callee_function_type(callee) {
                    self.emit_function_value_invoke(callee, arguments, instruction.output, &fn_ty)?;
                    return Ok(());
                }
                if matches!(*dispatch, crate::contracts::DispatchKind::Indirect) {
                    return Err(miette!(
                        code = "nyar::clr::indirect_call_missing_function_type",
                        help = "Indirect 调用要求 callee 带有已解析的 NyarType::Function",
                        "CLR 拒绝降低函数 `{}` 中缺少 Function 类型的 Indirect 调用",
                        self.operation
                    ));
                }
                // Array `suffix []` / bare `infix` operators may lower as ordinary MIR Calls.
                // Array `.length` must NOT be guessed by callee name here: it goes through
                // virtual dispatch → `[intrinsic("array.len")]` → `resolve_intrinsic_opcode`.
                if let MirOperand::Symbol(path) = callee {
                    // `[vm("i64_to_i32")] micro i64_to_i32(i64): i32` is a core
                    // conversion ABI, not a user MethodDef. Leaving it as a local call
                    // makes the CLR module fail its unresolved-local-call gate while
                    // lowering WASM SLEB helpers.
                    if self.try_emit_i64_to_i32_conversion(path, arguments, instruction.output) {
                        return Ok(());
                    }
                    // Unite/enum variant constructors (`Fine` / `Fail` / `Some` / …) are not MIR
                    // functions — lower them to tagged sum allocation before pushing call args.
                    // Prefer the call result type / enclosing return / payload evidence so
                    // `EndOfFile` → `VonTokenKind` (not `TokenKind`) and `Fail` → `VonParseResult`
                    // (not first-hit among many Fine/Fail unites).
                    let expected_sum = self.expected_sum_for_variant_ctor(path, instruction.output, arguments);
                    if let Some((sum_name, tag, is_unite)) = self.resolve_sum_variant_ctor(path, expected_sum.as_deref(), arguments) {
                        self.emit_sum_variant_ctor(&sum_name, tag, is_unite, arguments, instruction.output)?;
                        return Ok(());
                    }
                    // `T?` / erased Option: `None()` / `Some(x)` are Calls, not MethodDefs.
                    // Must run BEFORE ambiguous_sum_variant — many sums share `None`/`Some`, and
                    // `von_parse_take_fail` returns `VonDiagnostic?` as `Union(T, null)` → ldnull.
                    if self.try_emit_nullable_option_ctor(path, arguments, instruction.output)? {
                        return Ok(());
                    }
                    // Do not classify a call from the bare variant name. If
                    // the frontend did not carry enough structured sum/variant
                    // metadata for `resolve_sum_variant_ctor`, this remains an
                    // unresolved call and is rejected by the pre-emission
                    // verifier. Backends must never reconstruct language
                    // semantics by scanning variant names.
                    // Injected runtime `is_null` must not fall through to `c_str.is_null` (first
                    // `*.is_null` MethodDef) — that yields `call c_str_is_null(object)` on Fine
                    // valuetype payloads → InvalidProgramException.
                    let stub_parts: Vec<&str> = path.parts().iter().map(|part| part.as_str()).collect();
                    if is_injected_runtime_stub_symbol(&stub_parts) && stub_parts[0] == "is_null" && arguments.len() == 1 {
                        self.emit_clr_is_null(&arguments[0], instruction.output);
                        return Ok(());
                    }
                    // `tuple_get_N` → `ldfld Tuple::N` (fields are named "0","1",…). Never leave
                    // unresolved Call stubs that return null and poison nested FieldGets.
                    let method_name_early = path.parts().last().map(|part| part.as_str()).unwrap_or("");
                    if is_tuple_get_stub_name(method_name_early) && arguments.len() == 1 {
                        self.emit_tuple_get_field(&arguments[0], method_name_early, instruction.output)?;
                        return Ok(());
                    }
                }

                let has_by_address_receiver = matches!(receiver_kind, Some(ReceiverPassingKind::ByAddress));
                let arg_start = if has_by_address_receiver { 1 } else { 0 };
                let receiver_arg = if has_by_address_receiver { arguments.first() } else { None };

                if let Some(receiver) = receiver_arg {
                    let local = self.operand_local(receiver).expect("by-address receiver local");
                    self.emit_ldloca(local);
                }

                // Emit args with expected MIR parameter types so bare sum variants such as
                // `EndOfFile` resolve to `VonTokenKind` when the callee wants that sum
                // (never first-hit `TokenKind`). Align indices with stacked args (skip receiver).
                // Resolve call param MSIL types early so `object` slots get `box` for value
                // args (e.g. `format("{}", length)` — ldlen uint32 must not be passed raw).
                let param_expected_sums: Vec<Option<String>> = match callee {
                    MirOperand::Symbol(path) => self.resolve_callee_param_sum_names(path),
                    _ => Vec::new(),
                };
                // Injected stubs (`format`/`print`/…) win over MIR Unit→void and attached
                // `(string,i32)` so call sites box value args and keep `string` returns.
                let early_param_msil: Option<Vec<MsilType>> = match callee {
                    MirOperand::Symbol(path) => runtime_stub_signature(path)
                        .map(|(_, params)| params)
                        .or_else(|| {
                            parameter_types
                                .as_ref()
                                .map(|types| types.iter().map(|ty| nyar_type_to_msil(ty, &self.submission.aggregate_layouts)).collect())
                        })
                        .or_else(|| self.resolve_call_signature(path).map(|(_, params)| params)),
                    _ => parameter_types
                        .as_ref()
                        .map(|types| types.iter().map(|ty| nyar_type_to_msil(ty, &self.submission.aggregate_layouts)).collect()),
                };
                for (index, argument) in arguments.iter().skip(arg_start).enumerate() {
                    let expected = param_expected_sums.get(index + arg_start).and_then(|s| s.as_deref());
                    self.emit_operand_expecting(argument, expected);
                    let param_ty = early_param_msil.as_ref().and_then(|params| params.get(index + arg_start).or_else(|| params.get(index)));
                    if matches!(param_ty, Some(MsilType::Object)) {
                        // `EndOfFile` / enum sums leave a valuetype on the stack while
                        // `msil_type_of_operand(Symbol)` stays `object` — still need `box`
                        // or PEVerify / InvalidProgramException (e.g. lex_von / eof_von_token).
                        if let Some(type_operand) = self.box_token_for_object_param(argument, expected) {
                            self.instructions.push(MsilInstruction {
                                label: None,
                                opcode: MsilOpcode::Box,
                                operand: Some(MsilInstructionOperand::Type(type_operand)),
                            });
                        }
                    }
                }

                if let MirOperand::Symbol(path) = callee {
                    if let Some((owner, method_name, is_static, return_type, param_types)) = self.resolve_singleton_call(path, arguments) {
                        let returns_value = !matches!(return_type, MsilType::Void);
                        let signature = if is_static {
                            MsilMethodSignature::new(return_type.clone(), param_types)
                        }
                        else {
                            MsilMethodSignature::new_instance(return_type.clone(), param_types)
                        };
                        self.instructions.push(MsilInstruction {
                            label: None,
                            opcode: if is_static { MsilOpcode::Call } else { MsilOpcode::Callvirt },
                            operand: Some(MsilInstructionOperand::Method(MsilMethodRef { owner: Some(owner), name: method_name, signature })),
                        });
                        if let Some(output) = instruction.output {
                            if returns_value {
                                self.store_call_result(output, &return_type);
                            }
                        }
                        return Ok(());
                    }
                    let method_name = path.parts().last().map(|part| part.as_str()).unwrap_or_default();
                    // Instance / ordinal Array dispatch BEFORE fuzzy `[clr]` import match.
                    // Bare `get` must not bind to `HttpClient.GetStringAsync` (std.net.get).
                    if path.parts().len() == 1 {
                        let instance_method_name = path.parts()[0].as_str();
                        if let Some(receiver) = arguments.first() {
                            // Ordinal `suffix []` / `suffix []=` → Array.get / Array.set (virtual
                            // dispatch). Never bind ordinal sugar to `ldelem`/`stelem`.
                            let ordinal_dispatch = match instance_method_name {
                                "suffix []" => Some("get"),
                                "suffix []=" => Some("set"),
                                _ => None,
                            };
                            let receiver_is_array_for_ordinal = self.operand_is_std_array(receiver);
                            let resolved_instance =
                                self.resolve_instance_method_call(instance_method_name, receiver, has_by_address_receiver).or_else(|| {
                                    if receiver_is_array_for_ordinal {
                                        ordinal_dispatch.and_then(|get_or_set| {
                                            self.resolve_instance_method_call(get_or_set, receiver, has_by_address_receiver)
                                        })
                                    }
                                    else {
                                        None
                                    }
                                });
                            if let Some((method_symbol, return_type, param_types)) = resolved_instance {
                                let returns_value = !matches!(&return_type, MsilType::Void);
                                self.instructions.push(MsilInstruction {
                                    label: None,
                                    opcode: MsilOpcode::Call,
                                    operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                                        owner: None,
                                        name: method_symbol,
                                        signature: MsilMethodSignature::new(return_type.clone(), param_types),
                                    })),
                                });
                                if let Some(output) = instruction.output {
                                    if returns_value {
                                        self.store_call_result(output, &return_type);
                                    }
                                }
                                return Ok(());
                            }
                            if receiver_is_array_for_ordinal && matches!(instance_method_name, "suffix []" | "suffix []=" | "get" | "set") {
                                return Err(miette!(
                                    code = "nyar::clr::ordinal_array_index_unresolved",
                                    help = "序数 `x[i]` 必须虚派发到 `Array.get`/`Array.set`（host_contract→provider）；不要绑 ldelem",
                                    "CLR 无法解析函数 `{}` 中序数调用 `{path}` 到 Array.get/set MethodDef",
                                    self.operation
                                ));
                            }
                        }
                    }
                    // Bare `get`/`set`/`length` on Array must never fuzzy-match `[clr]` imports
                    // (`std.net.get` → HttpClient.GetStringAsync) or Utf8Text_length.
                    if path.parts().len() == 1
                        && matches!(path.parts()[0].as_str(), "get" | "set" | "length")
                        && arguments.first().is_some_and(|op| self.operand_is_std_array(op))
                    {
                        if let Some((method_symbol, return_type, param_types)) =
                            self.resolve_instance_method_call(path.parts()[0].as_str(), &arguments[0], has_by_address_receiver)
                        {
                            let returns_value = !matches!(&return_type, MsilType::Void);
                            self.instructions.push(MsilInstruction {
                                label: None,
                                opcode: MsilOpcode::Call,
                                operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                                    owner: None,
                                    name: method_symbol,
                                    signature: MsilMethodSignature::new(return_type.clone(), param_types),
                                })),
                            });
                            if let Some(output) = instruction.output {
                                if returns_value {
                                    self.store_call_result(output, &return_type);
                                }
                            }
                            return Ok(());
                        }
                    }
                    // `[clr(...)]` host imports after instance dispatch. Skip qualified
                    // `Array.get`/`Array.set` even if host_contract links were rebound to GetValue.
                    if !path_is_qualified_array_ordinal_host(path) {
                        if let Some((assembly, owner, method)) = self.resolve_external_import_link(path).and_then(|link| {
                            clr_host_method_target(link).map(|t| (t.assembly.to_string(), t.owner.to_string(), t.method.to_string()))
                        }) {
                            let target = ClrHostMethodTarget { assembly: assembly.as_str(), owner: owner.as_str(), method: method.as_str() };
                            let param_types: Vec<MsilType> = arguments
                                .iter()
                                .skip(arg_start)
                                .map(|arg| {
                                    self.lookup_value_type_from_operand(arg)
                                        .map(|ty| {
                                            let msil_type = nyar_type_to_msil(&ty, &self.submission.aggregate_layouts);
                                            match msil_type {
                                                MsilType::Named(_) => MsilType::Object,
                                                other => other,
                                            }
                                        })
                                        .unwrap_or(MsilType::Object)
                                })
                                .collect();
                            let desired_return = self
                                .resolve_call_signature(path)
                                .map(|(return_type, _)| return_type)
                                .or_else(|| {
                                    instruction.output.and_then(|output| {
                                        self.lookup_value_type(&output).map(|ty| nyar_type_to_msil(ty, &self.submission.aggregate_layouts))
                                    })
                                })
                                .unwrap_or(MsilType::Void);
                            self.emit_clr_host_method_call(&target, param_types, &desired_return, instruction.output);
                            return Ok(());
                        }
                    }
                    // 优先通过 witness 表元数据解析方法符号，而非按方法名猜测。
                    // `resolve_witness_method_symbol` 在无匹配 witness 表时返回 None，
                    // 自然回退到静态调用符号解析。
                    let symbol = self
                        .resolve_witness_method_symbol(method_name, witness.as_ref(), arguments)
                        .unwrap_or_else(|| self.resolve_static_call_symbol(path));
                    // `[clr(...)]` imports are in the executable map but intentionally have no
                    // local MethodDef — never emit `owner: None` for them.
                    if let Some(operation) = self.resolve_operation_for_static_path(path) {
                        if array_ordinal_host_kind(&operation).is_none() {
                            if let Some((assembly, owner, method)) = self.submission.external_import_links.get(&operation).and_then(|link| {
                                clr_host_method_target(link).map(|t| (t.assembly.to_string(), t.owner.to_string(), t.method.to_string()))
                            }) {
                                let target =
                                    ClrHostMethodTarget { assembly: assembly.as_str(), owner: owner.as_str(), method: method.as_str() };
                                let param_types: Vec<MsilType> = arguments
                                    .iter()
                                    .skip(arg_start)
                                    .map(|arg| {
                                        self.lookup_value_type_from_operand(arg)
                                            .map(|ty| {
                                                let msil_type = nyar_type_to_msil(&ty, &self.submission.aggregate_layouts);
                                                match msil_type {
                                                    MsilType::Named(_) => MsilType::Object,
                                                    other => other,
                                                }
                                            })
                                            .unwrap_or(MsilType::Object)
                                    })
                                    .collect();
                                let desired_return = self
                                    .resolve_call_signature(path)
                                    .map(|(return_type, _)| return_type)
                                    .or_else(|| {
                                        instruction.output.and_then(|output| {
                                            self.lookup_value_type(&output).map(|ty| nyar_type_to_msil(ty, &self.submission.aggregate_layouts))
                                        })
                                    })
                                    .unwrap_or(MsilType::Void);
                                self.emit_clr_host_method_call(&target, param_types, &desired_return, instruction.output);
                                return Ok(());
                            }
                        }
                    }
                    // `format`/`print` stubs before attached/MIR signatures: MIR often types
                    // `format` as Unit→void, which emits `call void format` + poisoned `add`
                    // (InvalidProgramException in emit_single_project_build).
                    let (return_type, params) = match runtime_stub_signature(path)
                        .or_else(|| self.signature_from_attached_parameter_types(instruction, parameter_types.as_ref(), arguments.len()))
                        .or_else(|| self.resolve_call_signature(path))
                        .or_else(|| self.infer_call_signature_from_types(instruction, arguments))
                        .or_else(|| self.resolve_witness_call_signature(method_name, witness.as_ref(), arguments))
                    {
                        Some(signature) => signature,
                        None => {
                            return Err(miette!(
                                code = "nyar::clr::unknown_call_signature",
                                help = "请由前端提交被调用函数的 MIR 签名，或通过 witness/基础 `[clr(...)]` FFI 显式声明签名",
                                "CLR 无法解析函数 `{}` 中调用 `{path}` 的真实签名；拒绝 Object(Object) 回退",
                                self.operation
                            ));
                        }
                    };
                    self.instructions.push(MsilInstruction {
                        label: None,
                        opcode: MsilOpcode::Call,
                        operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                            owner: None,
                            name: symbol,
                            signature: MsilMethodSignature::new(return_type.clone(), params),
                        })),
                    });
                    if let Some(output) = instruction.output {
                        self.store_call_result(output, &return_type);
                    }
                }
                else if matches!(callee, MirOperand::Value(_)) {
                    return Err(miette!(
                        code = "nyar::clr::unknown_call_signature",
                        help = "函数值调用需要 callee 带有 NyarType::Function（MIR 宜标记 DispatchKind::Indirect）",
                        "CLR 无法解析函数 `{}` 中对非 MethodDef / 非 Function 值的调用",
                        self.operation
                    ));
                }
            }
            MirInstructionKind::TupleNew { fields, storage, layout_id, .. } => {
                if *storage != StorageKind::Value {
                    return Ok(());
                }
                let layout_id = ExecutableLoweringContext::require_layout_id(*layout_id, "TupleNew");
                let layout = self.ctx.layout_by_id(layout_id).expect("tuple layout").clone();
                let qualified = self.ctx.clr_qualified_type_name(&layout.name);
                let local = instruction.output.and_then(|value| self.slots.value_locals.get(&value).copied()).expect("aggregate local");
                self.emit_ldloca(local);
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Initobj,
                    operand: Some(MsilInstructionOperand::Type(qualified.clone())),
                });
                for (index, value) in fields.iter().enumerate() {
                    let field = layout.fields.get(index).expect("aggregate field");
                    self.emit_ldloca(local);
                    self.emit_operand(value);
                    self.instructions.push(MsilInstruction {
                        label: None,
                        opcode: MsilOpcode::Stfld,
                        operand: Some(MsilInstructionOperand::Field(qualified.clone(), field.name.clone())),
                    });
                }
            }
            MirInstructionKind::ArrayLiteral { element_type, items, .. } => {
                self.emit_clr_array_literal(instruction.output, element_type, items);
            }
            MirInstructionKind::ArrayNew { element_type, length, .. } => {
                self.emit_clr_array_new(instruction.output, element_type, length);
            }
            MirInstructionKind::FixedArrayNew { items, storage, element_type, layout_id, .. } => {
                if *storage == StorageKind::Reference {
                    self.emit_clr_array_literal(instruction.output, element_type, items);
                }
                else {
                    let layout_id = ExecutableLoweringContext::require_layout_id(*layout_id, "FixedArrayNew");
                    let layout = self.ctx.layout_by_id(layout_id).expect("fixed-array layout").clone();
                    let qualified = self.ctx.clr_qualified_type_name(&layout.name);
                    let local = instruction.output.and_then(|value| self.slots.value_locals.get(&value).copied()).expect("aggregate local");
                    self.emit_ldloca(local);
                    self.instructions.push(MsilInstruction {
                        label: None,
                        opcode: MsilOpcode::Initobj,
                        operand: Some(MsilInstructionOperand::Type(qualified.clone())),
                    });
                    for (index, value) in items.iter().enumerate() {
                        let field = layout.fields.get(index).expect("aggregate field");
                        self.emit_ldloca(local);
                        self.emit_operand(value);
                        self.instructions.push(MsilInstruction {
                            label: None,
                            opcode: MsilOpcode::Stfld,
                            operand: Some(MsilInstructionOperand::Field(qualified.clone(), field.name.clone())),
                        });
                    }
                }
            }
            // pattern 无法 lowering：extractor 未 resolved 或类型推断失败，
            // 运行期 trap——调用永不返回的 helper 抛出异常。
            MirInstructionKind::PatternMatch { .. } => {
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Call,
                    operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                        owner: None,
                        name: "nyar_pattern_match_unreachable".to_string(),
                        signature: MsilMethodSignature::new(MsilType::Void, Vec::new()),
                    })),
                });
            }
            _ => {}
        }
        Ok(())
    }

    fn emit_terminator(&mut self, block: &MirBlock) {
        match &block.terminator {
            MirTerminator::Return { value } => {
                let returns_value = !matches!(self.mir_fn.return_type, NyarType::Bottom | NyarType::Unit);
                if returns_value {
                    if let Some(value) = value {
                        let expected = Self::sum_type_name_from_nyar(&self.mir_fn.return_type);
                        self.emit_operand_expecting(value, expected.as_deref());
                    }
                    else {
                        self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None });
                    }
                }
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None });
            }
            MirTerminator::Jump { target, arguments } => {
                self.emit_block_argument_copies(*target, arguments);
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Br,
                    operand: Some(MsilInstructionOperand::BranchTarget(block_label(*target))),
                });
            }
            MirTerminator::Branch { condition, then_target, else_target } => {
                self.emit_operand(condition);
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Brfalse,
                    operand: Some(MsilInstructionOperand::BranchTarget(block_label(*else_target))),
                });
                self.emit_block_argument_copies(*then_target, &[]);
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Br,
                    operand: Some(MsilInstructionOperand::BranchTarget(block_label(*then_target))),
                });
                let _ = else_target;
            }
            MirTerminator::StateDispatch { state, cases, default_target } => {
                let state_in_locals = self.slots.value_locals.get(state).copied();
                if let Some(state_local) = state_in_locals {
                    for (case_key, target) in cases {
                        self.emit_ldloc(state_local);
                        self.instructions.push(MsilInstruction {
                            label: None,
                            opcode: MsilOpcode::LdcI4,
                            operand: Some(MsilInstructionOperand::Integer(*case_key as i64)),
                        });
                        self.instructions.push(MsilInstruction {
                            label: None,
                            opcode: MsilOpcode::Beq,
                            operand: Some(MsilInstructionOperand::BranchTarget(block_label(*target))),
                        });
                    }
                }
                else {
                    for (case_key, target) in cases {
                        self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None });
                        self.instructions.push(MsilInstruction {
                            label: None,
                            opcode: MsilOpcode::LdcI4,
                            operand: Some(MsilInstructionOperand::Integer(*case_key as i64)),
                        });
                        self.instructions.push(MsilInstruction {
                            label: None,
                            opcode: MsilOpcode::Beq,
                            operand: Some(MsilInstructionOperand::BranchTarget(block_label(*target))),
                        });
                    }
                }
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Br,
                    operand: Some(MsilInstructionOperand::BranchTarget(block_label(*default_target))),
                });
            }
            MirTerminator::PerformEffect { .. } | MirTerminator::YieldToRuntime { .. } | MirTerminator::Unreachable => {
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None });
            }
        }
    }

    fn resolve_singleton_call(
        &self,
        path: &nyar::NamePath,
        arguments: &[MirOperand],
    ) -> Option<(String, String, bool, MsilType, Vec<MsilType>)> {
        if path.parts().len() != 2 {
            return None;
        }
        let type_name = path.parts()[0].as_str();
        let method_name = path.parts()[1].as_str();
        let plan = self.submission.singleton_instances.iter().find(|plan| plan.name == type_name)?;
        let owner = self.ctx.clr_qualified_type_name(type_name);
        if method_name == plan.accessor_method() && arguments.is_empty() {
            return Some((owner, method_name.to_string(), true, MsilType::Named(plan.name.clone()), Vec::new()));
        }
        let symbol = format!("{type_name}.{method_name}");
        let mir_fn = self.submission.executable.as_ref().and_then(|exec| exec.find_by_symbol(&symbol)).map(|view| view.function)?;
        let return_type = nyar_type_to_msil(&mir_fn.return_type, &self.submission.aggregate_layouts);
        let param_types = mir_fn.param_types.iter().skip(1).map(|ty| nyar_type_to_msil(ty, &self.submission.aggregate_layouts)).collect();
        Some((owner, method_name.to_string(), false, return_type, param_types))
    }

    /// 解析 `receiver.method()` 形式的实例方法调用（callee 仅为方法名）。
    fn resolve_instance_method_call(
        &self,
        method_name: &str,
        receiver: &MirOperand,
        by_address: bool,
    ) -> Option<(String, MsilType, Vec<MsilType>)> {
        let receiver_ty = match receiver {
            MirOperand::Value(value) => self.lookup_value_type(value)?.clone(),
            _ => return None,
        };
        // Prefer the CLR evaluation-stack type: `[utf8]` params lower to `string[]` even when
        // MIR value_types were confused with `Utf8Text`. Also treat `Array<T>` (`Apply`) and
        // Named(`std.collection.Array`) as array — otherwise bare `get`/`length` fuzzy-match
        // HttpClient.GetStringAsync / Utf8Text_length.
        let receiver_is_array = self.operand_is_std_array(receiver) || nyar_type_is_std_array(&receiver_ty);
        let receiver_is_clr_array = matches!(self.msil_type_of_operand(receiver), MsilType::SzArray(_));
        let exec = self.submission.executable.as_ref()?;
        let mut exact: Option<(String, MsilType, Vec<MsilType>)> = None;
        for operation in exec.operations() {
            if operation.parts().len() < 2 {
                continue;
            }
            if operation.parts().last().map(|part| part.as_str()) != Some(method_name) {
                continue;
            }
            // Namespaced `[clr(...)]` free FFI micros are not instance MethodDefs.
            // Exception: `Array.get`/`Array.set` keep local ordinal MethodDefs even when their
            // host_contract link was rebound to GetValue/SetValue.
            if self.submission.external_import_links.get(&operation).and_then(clr_host_method_target).is_some()
                && array_ordinal_host_kind(&operation).is_none()
            {
                continue;
            }
            let Some(view) = exec.get_function(&operation)
            else {
                continue;
            };
            let mir_fn = &view.function;
            let Some(param_ty) = mir_fn.param_types.first()
            else {
                continue;
            };
            let parts = operation.parts();
            let owner_type_name = parts.get(parts.len().saturating_sub(2))?.as_str();
            if receiver_is_clr_array && owner_type_name != "Array" {
                continue;
            }
            let receiver_name = receiver_type_name(&receiver_ty);
            let param_type_name = receiver_type_name(param_ty);
            // Penultimate segment must name the method's `self` type, not a namespace
            // (`std.adaptor.clr.net.__http_get` must not match utf8 receivers).
            let owner_is_self_type = matches!(param_ty, NyarType::Named(name) if name.as_str() == "Self")
                || matches!(param_ty, NyarType::Array(_))
                || param_type_name.is_some_and(|pn| pn == owner_type_name || pn.ends_with(owner_type_name) || owner_type_name.ends_with(pn));
            if !owner_is_self_type {
                continue;
            }
            let type_name_matches = receiver_is_clr_array && owner_type_name == "Array"
                || receiver_name.is_some_and(|name| name == owner_type_name || name.ends_with(owner_type_name));
            // Require a real type-name match against the method owner.
            if !type_name_matches {
                continue;
            }
            let return_type = nyar_type_to_msil(&mir_fn.return_type, &self.submission.aggregate_layouts);
            let param_types = mir_fn
                .param_types
                .iter()
                .skip(if by_address { 1 } else { 0 })
                .map(|ty| nyar_type_to_msil(ty, &self.submission.aggregate_layouts))
                .collect();
            let candidate = (sanitize_operation_symbol(&operation), return_type, param_types);
            if owner_type_name == "Array" {
                return Some(candidate);
            }
            exact.get_or_insert(candidate);
        }
        exact
    }

    /// True when the operand is `std.collection.Array` / `Array<T>` / CLR `T[]`.
    fn operand_is_std_array(&self, operand: &MirOperand) -> bool {
        if matches!(self.msil_type_of_operand(operand), MsilType::SzArray(_)) {
            return true;
        }
        if let MsilType::Named(name) = self.msil_type_of_operand(operand) {
            if name_is_std_array(name.as_str()) {
                return true;
            }
        }
        self.lookup_value_type_from_operand(operand).is_some_and(|ty| nyar_type_is_std_array(&ty))
    }

    fn emit_block_argument_copies(&mut self, target: MirBlockRef, arguments: &[MirOperand]) {
        let Some(target_block) = self.mir_fn.blocks.get(target.0 as usize)
        else {
            return;
        };
        for (index, parameter) in target_block.parameters.iter().enumerate() {
            let Some(argument) = arguments.get(index)
            else {
                continue;
            };
            let Some(param_local) = self.slots.block_param_locals.get(&(target, index)).copied()
            else {
                continue;
            };
            self.emit_operand(argument);
            // Block-param slots are often planned as `Unit`→`int32`. Retype from the
            // concrete argument (e.g. Call result already retyped to `string`) so
            // `stloc`/`ldloc` stay PEVerify-legal across CFG edges.
            let arg_ty = self.msil_type_of_operand(argument);
            if !matches!(arg_ty, MsilType::Void) {
                if let Some(slot) = self.slots.local_types.get_mut(param_local as usize) {
                    *slot = sanitize_clr_local_type(arg_ty);
                }
            }
            self.emit_stloc(param_local);
            self.slots.value_locals.insert(*parameter, param_local);
        }
    }

    fn emit_intrinsic_opcode(&mut self, opcode: IntrinsicOpcode, arguments: &[MirOperand], output: Option<MirValueRef>) {
        match opcode {
            IntrinsicOpcode::Binary(op) => {
                self.emit_operand(&arguments[0]);
                self.emit_operand(&arguments[1]);
                let msil = match op {
                    IntrinsicBinaryOp::Add => MsilOpcode::Add,
                    IntrinsicBinaryOp::Sub => MsilOpcode::Sub,
                    IntrinsicBinaryOp::Mul => MsilOpcode::Mul,
                    IntrinsicBinaryOp::Div => MsilOpcode::Div,
                    IntrinsicBinaryOp::Rem => MsilOpcode::Rem,
                };
                self.instructions.push(MsilInstruction { label: None, opcode: msil, operand: None });
                if let Some(output) = output {
                    // Integer arithmetic leaves int32 on the evaluation stack.
                    self.store_call_result(output, &MsilType::Int32 { signed: true });
                }
            }
            IntrinsicOpcode::Neg => {
                self.emit_operand(&arguments[0]);
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Neg, operand: None });
                if let Some(output) = output {
                    self.store_call_result(output, &MsilType::Int32 { signed: true });
                }
            }
            IntrinsicOpcode::ArrayLen => {
                self.emit_operand(&arguments[0]);
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ldlen, operand: None });
                if let Some(output) = output {
                    self.store_call_result(output, &MsilType::Int32 { signed: false });
                }
            }
            IntrinsicOpcode::Deref => {
                // Bound from `[intrinsic("ref.deref")]` / MIR `__ref_deref`: on CLR, class
                // handles are already object references — expand as identity copy.
                self.emit_operand(&arguments[0]);
                if let Some(output) = output {
                    self.store_to_value(output);
                }
            }
            IntrinsicOpcode::ArrayGet => {
                self.emit_operand(&arguments[0]);
                self.emit_operand(&arguments[1]);
                let element = self.array_element_msil_type(&arguments[0]);
                self.emit_ldelem_for_element(&element);
                if let Some(output) = output {
                    // Retype from known SzArray / MIR `[T]` evidence (not Object guesses).
                    if !matches!(element, MsilType::Object | MsilType::Void) {
                        if let Some(&local) = self.slots.value_locals.get(&output) {
                            if let Some(slot) = self.slots.local_types.get_mut(local as usize) {
                                *slot = sanitize_clr_local_type(element.clone());
                            }
                        }
                    }
                    self.store_to_value(output);
                }
            }
            IntrinsicOpcode::ArraySet => {
                self.emit_operand(&arguments[0]);
                self.emit_operand(&arguments[1]);
                self.emit_operand(&arguments[2]);
                let element = self.array_element_msil_type(&arguments[0]);
                self.emit_stelem_for_element(&element);
            }
            IntrinsicOpcode::ArrayPush => self.emit_intrinsic_array_push(arguments, output),
            IntrinsicOpcode::Utf8ScalarLength => {
                self.emit_operand(&arguments[0]);
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Call,
                    operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                        owner: None,
                        name: "__nyar_utf8_scalar_length".to_string(),
                        signature: MsilMethodSignature::new(MsilType::Int32 { signed: true }, vec![MsilType::String]),
                    })),
                });
                if let Some(output) = output {
                    self.store_call_result(output, &MsilType::Int32 { signed: true });
                }
            }
            IntrinsicOpcode::Utf8ContentEqual => self.emit_intrinsic_compare(IntrinsicCompareOp::Eq, arguments, output),
            IntrinsicOpcode::Utf8ContentNotEqual => self.emit_intrinsic_compare(IntrinsicCompareOp::Ne, arguments, output),
            IntrinsicOpcode::Utf8Trim => {
                if arguments.len() != 1 {
                    return;
                }
                self.emit_operand(&arguments[0]);
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Callvirt,
                    operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                        owner: Some("[mscorlib]System.String".to_string()),
                        name: "Trim".to_string(),
                        signature: MsilMethodSignature::new_instance(MsilType::String, Vec::new()),
                    })),
                });
                if let Some(output) = output {
                    self.store_call_result(output, &MsilType::String);
                }
            }
            IntrinsicOpcode::Utf8IndexOf | IntrinsicOpcode::Utf8Contains | IntrinsicOpcode::Utf8StartsWith | IntrinsicOpcode::Utf8EndsWith => {
                if arguments.len() != 2 {
                    return;
                }
                self.emit_operand(&arguments[0]);
                self.emit_operand(&arguments[1]);
                let (method, return_type) = match opcode {
                    IntrinsicOpcode::Utf8IndexOf => ("IndexOf", MsilType::Int32 { signed: true }),
                    IntrinsicOpcode::Utf8Contains => ("Contains", MsilType::Bool),
                    IntrinsicOpcode::Utf8StartsWith => ("StartsWith", MsilType::Bool),
                    IntrinsicOpcode::Utf8EndsWith => ("EndsWith", MsilType::Bool),
                    _ => unreachable!(),
                };
                let target = ClrHostMethodTarget { assembly: "mscorlib", owner: "System.String", method };
                self.emit_clr_host_method_call(&target, vec![MsilType::String], &return_type, output);
            }
            IntrinsicOpcode::SumVariantIs | IntrinsicOpcode::SumStructuralEqual => {
                return;
            }
            IntrinsicOpcode::Utf8ScalarSlice => {
                if arguments.len() < 3 {
                    return;
                }
                self.emit_operand(&arguments[0]);
                self.emit_operand(&arguments[1]);
                self.emit_operand(&arguments[2]);
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Callvirt,
                    operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                        owner: Some("[mscorlib]System.String".to_string()),
                        name: "Substring".to_string(),
                        signature: MsilMethodSignature::new_instance(
                            MsilType::String,
                            vec![MsilType::Int32 { signed: true }, MsilType::Int32 { signed: true }],
                        ),
                    })),
                });
                if let Some(output) = output {
                    self.store_call_result(output, &MsilType::String);
                }
            }
            IntrinsicOpcode::Compare(op) => self.emit_intrinsic_compare(op, arguments, output),
            IntrinsicOpcode::Not => {
                self.emit_operand(&arguments[0]);
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None });
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ceq, operand: None });
                if let Some(output) = output {
                    self.store_call_result(output, &MsilType::Bool);
                }
            }
            IntrinsicOpcode::Bitwise(op) => {
                self.emit_operand(&arguments[0]);
                self.emit_operand(&arguments[1]);
                let msil = match op {
                    IntrinsicBitwiseOp::And => MsilOpcode::And,
                    IntrinsicBitwiseOp::Or => MsilOpcode::Or,
                    IntrinsicBitwiseOp::Xor => MsilOpcode::Xor,
                    IntrinsicBitwiseOp::Shl => MsilOpcode::Shl,
                    IntrinsicBitwiseOp::Shr => MsilOpcode::Shr,
                };
                self.instructions.push(MsilInstruction { label: None, opcode: msil, operand: None });
                if let Some(output) = output {
                    self.store_call_result(output, &MsilType::Int32 { signed: true });
                }
            }
        }
    }

    /// Lower the standard `[vm("i64_to_i32")]` primitive directly to `conv.i4`.
    ///
    /// The executable collector can erase the source operand type across partitions, so the
    /// stable VM symbol plus its unary ABI is the available evidence here. A source-level
    /// user method with this reserved VM binding name is not a CLR MethodDef.
    fn try_emit_i64_to_i32_conversion(&mut self, path: &NamePath, arguments: &[MirOperand], output: Option<MirValueRef>) -> bool {
        let is_i64_to_i32 = path.parts().last().is_some_and(|part| part.as_str() == "i64_to_i32") && arguments.len() == 1;
        if !is_i64_to_i32 {
            return false;
        }

        self.emit_operand(&arguments[0]);
        self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::ConvI4, operand: None });
        if let Some(output) = output {
            self.store_call_result(output, &MsilType::Int32 { signed: true });
        }
        true
    }

    fn emit_intrinsic_compare(&mut self, op: IntrinsicCompareOp, arguments: &[MirOperand], output: Option<MirValueRef>) {
        let is_string_compare = arguments.iter().any(|arg| self.msil_type_of_operand(arg) == MsilType::String);
        if is_string_compare {
            self.emit_operand(&arguments[0]);
            self.emit_operand(&arguments[1]);
            if matches!(op, IntrinsicCompareOp::Eq | IntrinsicCompareOp::Ne) {
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Call,
                    operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                        owner: Some("[mscorlib]System.String".to_string()),
                        name: "op_Equality".to_string(),
                        signature: MsilMethodSignature::new(MsilType::Bool, vec![MsilType::String, MsilType::String]),
                    })),
                });
                if op == IntrinsicCompareOp::Ne {
                    self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None });
                    self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ceq, operand: None });
                }
            }
            else {
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Call,
                    operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                        owner: Some("[mscorlib]System.String".to_string()),
                        name: "Compare".to_string(),
                        signature: MsilMethodSignature::new(MsilType::Int32 { signed: true }, vec![MsilType::String, MsilType::String]),
                    })),
                });
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None });
                let compare_opcode = match op {
                    IntrinsicCompareOp::Lt => MsilOpcode::Clt,
                    IntrinsicCompareOp::Le => MsilOpcode::Cgt,
                    IntrinsicCompareOp::Gt => MsilOpcode::Cgt,
                    IntrinsicCompareOp::Ge => MsilOpcode::Clt,
                    _ => MsilOpcode::Ceq,
                };
                self.instructions.push(MsilInstruction { label: None, opcode: compare_opcode, operand: None });
                if matches!(op, IntrinsicCompareOp::Le | IntrinsicCompareOp::Ge) {
                    self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None });
                    self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ceq, operand: None });
                }
            }
            if let Some(output) = output {
                self.store_call_result(output, &MsilType::Bool);
            }
            return;
        }
        self.emit_operand(&arguments[0]);
        self.emit_operand(&arguments[1]);
        let unsigned = arguments.iter().any(|arg| {
            matches!(
                self.msil_type_of_operand(arg),
                MsilType::Int8 { signed: false }
                    | MsilType::Int16 { signed: false }
                    | MsilType::Int32 { signed: false }
                    | MsilType::Int64 { signed: false }
            )
        });
        let opcode = match (op, unsigned) {
            (IntrinsicCompareOp::Eq | IntrinsicCompareOp::Ne, _) => MsilOpcode::Ceq,
            (IntrinsicCompareOp::Lt, false) => MsilOpcode::Clt,
            (IntrinsicCompareOp::Lt, true) => MsilOpcode::CltUn,
            (IntrinsicCompareOp::Le, false) => MsilOpcode::Cgt,
            (IntrinsicCompareOp::Le, true) => MsilOpcode::CgtUn,
            (IntrinsicCompareOp::Gt, false) => MsilOpcode::Cgt,
            (IntrinsicCompareOp::Gt, true) => MsilOpcode::CgtUn,
            (IntrinsicCompareOp::Ge, false) => MsilOpcode::Clt,
            (IntrinsicCompareOp::Ge, true) => MsilOpcode::CltUn,
        };
        self.instructions.push(MsilInstruction { label: None, opcode, operand: None });
        if matches!(op, IntrinsicCompareOp::Ne | IntrinsicCompareOp::Le | IntrinsicCompareOp::Ge) {
            self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None });
            self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ceq, operand: None });
        }
        if let Some(output) = output {
            self.store_call_result(output, &MsilType::Bool);
        }
    }

    fn emit_load_constant(&mut self, constant: &MirConstant) {
        match constant {
            MirConstant::Int(value) => {
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::LdcI4,
                    operand: Some(MsilInstructionOperand::Integer(*value)),
                });
            }
            MirConstant::Float64(value) => {
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::LdcR8,
                    operand: Some(MsilInstructionOperand::Float(value.to_string())),
                });
            }
            MirConstant::Bool(value) => {
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: if *value { MsilOpcode::LdcI4_1 } else { MsilOpcode::LdcI4_0 },
                    operand: None,
                });
            }
            MirConstant::Utf8(text) | MirConstant::Utf16(text) => {
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Ldstr,
                    operand: Some(MsilInstructionOperand::StringLiteral(text.clone())),
                });
            }
            MirConstant::Unit => {
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None });
            }
        }
    }

    fn emit_operand(&mut self, operand: &MirOperand) {
        let _ = self.emit_operand_expecting(operand, None);
    }

    /// Like `emit_operand`, but errors when the operand cannot leave a value on the stack.
    fn emit_operand_required(&mut self, operand: &MirOperand, context: &str) -> Result<()> {
        if self.emit_operand_expecting(operand, None) {
            Ok(())
        }
        else {
            Err(miette!("CLR cannot materialize operand for {context} in `{}` (operand={:?})", self.operation, operand))
        }
    }

    /// Returns `true` when a value was pushed onto the evaluation stack.
    fn emit_operand_expecting(&mut self, operand: &MirOperand, expected_sum: Option<&str>) -> bool {
        match operand {
            MirOperand::Value(value) => {
                if let Some(local) = self.slots.value_locals.get(value).copied() {
                    self.emit_ldloc(local);
                    true
                }
                else {
                    false
                }
            }
            MirOperand::Constant(constant) => {
                self.emit_load_constant(constant);
                true
            }
            MirOperand::Symbol(path) => {
                if let Some(local) = self.slots.var_locals.get(&path.to_string()).copied() {
                    self.emit_ldloc(local);
                    true
                }
                else if let Some(value) = self.find_named_value(path) {
                    if let Some(local) = self.slots.value_locals.get(&value).copied() {
                        self.emit_ldloc(local);
                        true
                    }
                    else {
                        false
                    }
                }
                else if let Some((sum_name, tag, is_unite)) = self.resolve_sum_variant_ctor(path, expected_sum, &[]) {
                    // Nullary unite/enum variant used as a value (`LeftBrace`, `EndOfFile`, …).
                    self.emit_sum_variant_value_on_stack(&sum_name, tag, is_unite);
                    true
                }
                else {
                    false
                }
            }
        }
    }

    /// Leave a nullary (or already-arg-pushed) sum variant value on the evaluation stack.
    fn emit_sum_variant_value_on_stack(&mut self, sum_name: &str, tag: u32, is_unite: bool) {
        let qualified = self.ctx.clr_qualified_type_name(sum_name);
        if is_unite {
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Newobj,
                operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                    owner: Some(qualified.clone()),
                    name: ".ctor".to_string(),
                    signature: MsilMethodSignature::new(MsilType::Void, Vec::new()),
                })),
            });
            self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Dup, operand: None });
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::LdcI4,
                operand: Some(MsilInstructionOperand::Integer(tag as i64)),
            });
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Stfld,
                operand: Some(MsilInstructionOperand::Field(qualified.clone(), "tag".to_string())),
            });
            self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Dup, operand: None });
            self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ldnull, operand: None });
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Stfld,
                operand: Some(MsilInstructionOperand::Field(qualified, "payload".to_string())),
            });
        }
        else {
            // Value-type enum: allocate a temp, initobj + set tag, ldloc.
            let temp = self.alloc_temp_local(MsilType::Named(qualified.clone()));
            self.emit_ldloca(temp);
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Initobj,
                operand: Some(MsilInstructionOperand::Type(qualified.clone())),
            });
            self.emit_ldloca(temp);
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::LdcI4,
                operand: Some(MsilInstructionOperand::Integer(tag as i64)),
            });
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Stfld,
                operand: Some(MsilInstructionOperand::Field(qualified.clone(), "tag".to_string())),
            });
            self.emit_ldloc(temp);
        }
    }

    fn operand_local(&self, operand: &MirOperand) -> Option<u16> {
        match operand {
            MirOperand::Value(value) => self.slots.value_locals.get(value).copied(),
            MirOperand::Symbol(path) => self
                .slots
                .var_locals
                .get(&path.to_string())
                .copied()
                .or_else(|| self.find_named_value(path).and_then(|value| self.slots.value_locals.get(&value).copied())),
            _ => None,
        }
    }

    fn resolve_reference_field_owner(&self, field: &str, layout_id: Option<nyar_types::LayoutId>, object: &MirOperand) -> String {
        let inferred = self.infer_aggregate_name(object);
        let layout_has_field = |layout: &nyar_types::AggregateLayout| layout.fields.iter().any(|f| f.name == field);
        let layout_is_tagged_payload = |layout: &nyar_types::AggregateLayout| {
            layout.fields.iter().any(|f| f.name == "tag") && layout.fields.iter().any(|f| f.name == "payload")
        };
        // Never keep an inferred/layout owner that does not declare `field`. Tuple SSA values
        // (`__tuple_A_B`) were winning over `ExecutableBlockRef` for nested `id` FieldGets,
        // producing `ldfld __tuple_…::id` with no FieldDef (PE write abort after emit).
        let by_layout = layout_id.and_then(|lid| self.ctx.layout_by_id(lid)).and_then(|layout| {
            if !layout_has_field(layout) {
                return None;
            }
            // Unite `payload`/`tag` must not bind to aggregates that only share the field name
            // (`AlgebraicTerm.payload: utf8`) when MIR layout_id is stale / wrong.
            // But `value`/`error` are also legitimate struct field names (e.g. `VonParsedValue.value`);
            // only reject them if the layout looks like a tagged union AND lacks the literal field.
            if matches!(field, "payload" | "tag") && !layout_is_tagged_payload(layout) {
                return None;
            }
            Some(self.ctx.clr_qualified_type_name(&layout.name))
        });
        let by_field = self
            .ctx
            .find_type_name_by_field_with_hint(field, inferred.as_deref())
            .or_else(|| self.ctx.find_type_name_by_field(field))
            .and_then(|name| {
                let layout = self.ctx.layout_by_type_name(&name)?;
                if matches!(field, "payload" | "tag") && !layout_is_tagged_payload(layout) {
                    return None;
                }
                if matches!(field, "value" | "error") && !layout_has_field(layout) {
                    return None;
                }
                Some(self.ctx.clr_qualified_type_name(&name))
            });
        let by_inferred = inferred.as_ref().and_then(|name| {
            self.ctx.layout_by_type_name(name).and_then(|layout| {
                if !layout_has_field(layout) {
                    return None;
                }
                if matches!(field, "payload" | "tag") && !layout_is_tagged_payload(layout) {
                    return None;
                }
                Some(self.ctx.clr_qualified_type_name(name))
            })
        });
        let by_sum = self.sum_type_name_for_operand(object).map(|name| self.ctx.clr_qualified_type_name(&name));
        // `payload`/`tag` on unite sums: when the scrutinee is erased to `object`, do not
        // first-hit `PackResult`. Only apply this when we lack layout/sum evidence and the
        // stack type is `object` — never override real aggregates like
        // `LegionBackendExecutionResult.payload`.
        // A CLR object carrier contains no nominal-sum identity. In particular,
        // do not guess it from `payload`, `value`, `error`, a local name, or a
        // historical Result/Option convention. The Semantic MIR sum registry is
        // the only source of that identity.
        let by_local_sum = None;
        // Prefer owners that actually declare `field`. Inferred tuple SSA
        // (`__tuple_A_B`) must not win over `ExecutableBlockRef` for nested `id`.
        // Unite `payload`/`tag` still prefer sum evidence before fuzzy field search.
        if matches!(field, "payload" | "tag") {
            by_sum.or(by_local_sum).or(by_layout).or(by_inferred).or(by_field).unwrap_or_else(|| "[mscorlib]System.Object".to_string())
        }
        else if matches!(field, "value" | "error") {
            // Concrete aggregate metadata takes priority. No field spelling
            // implies a nominal sum when that metadata is absent.
            by_layout.or(by_inferred).or(by_field).or(by_sum).or(by_local_sum).unwrap_or_else(|| "[mscorlib]System.Object".to_string())
        }
        else {
            by_layout.or(by_inferred).or(by_field).or(by_sum).unwrap_or_else(|| "[mscorlib]System.Object".to_string())
        }
    }

    /// Project a declared unite payload field to its physical CLR slot. Field
    /// spellings alone never identify a sum or variant.
    fn unite_payload_field_name(&self, owner: &str, field_name: &str) -> String {
        if !matches!(field_name, "value" | "error") {
            return field_name.to_string();
        }
        let is_unite_owner = self.submission.sum_types.iter().any(|sum| sum.is_unite && self.ctx.clr_qualified_type_name(&sum.name) == owner);
        if is_unite_owner { "payload".to_string() } else { field_name.to_string() }
    }

    /// Look up an enum/unite variant constructor (`Fine`, `Fail`, `LeftBrace`, …).
    ///
    /// When several sums share a variant name (`EndOfFile` on `TokenKind` and `VonTokenKind`,
    /// `Fail` on many parse Results), resolve from evidence — never silent first-hit.
    fn resolve_sum_variant_ctor(&self, path: &NamePath, expected_sum: Option<&str>, arguments: &[MirOperand]) -> Option<(String, u32, bool)> {
        let variant_name = path.parts().last()?.as_str();
        let mut matches = Vec::new();
        for sum in &self.submission.sum_types {
            if let Some(variant) = sum.variants.iter().find(|variant| variant.name == variant_name) {
                matches.push((sum.name.clone(), variant.tag, sum.is_unite));
            }
        }
        if matches.is_empty() {
            return None;
        }
        if let Some(expected) = expected_sum {
            if let Some(exact) = Self::pick_sum_match(&matches, expected) {
                return Some(exact);
            }
        }
        // Qualified path `VonTokenKind.EndOfFile` / `TokenKind.EndOfFile`.
        if path.parts().len() >= 2 {
            let hint = path.parts()[path.parts().len() - 2].as_str();
            if let Some(exact) = Self::pick_sum_match(&matches, hint) {
                return Some(exact);
            }
        }
        // Payload-typed disambiguation: `Fail(VonDiagnostic)` → `VonParseResult`, not `Result`.
        // Also match argument arity to payload shape (nullary / single / tuple) so shared
        // variant names like `Field` (`HirExprKind` vs `MsilInstructionOperand`) resolve.
        let arg_arity = arguments.len();
        let mut by_arity = Vec::new();
        for (sum_name, tag, is_unite) in &matches {
            let Some(sum) = self.submission.sum_types.iter().find(|s| s.name == *sum_name)
            else {
                continue;
            };
            let Some(variant) = sum.variants.iter().find(|v| v.tag == *tag && v.name == variant_name)
            else {
                continue;
            };
            let payload_arity = match variant.payload_type.as_ref() {
                None => 0,
                Some(NyarType::Tuple(items)) => items.len(),
                Some(_) => 1,
            };
            if payload_arity == arg_arity {
                by_arity.push((sum_name.clone(), *tag, *is_unite));
            }
        }
        if by_arity.len() == 1 {
            return Some(by_arity.remove(0));
        }
        if by_arity.len() > 1 {
            matches = by_arity;
        }
        if let Some(arg) = arguments.first() {
            if let Some(arg_ty) = self.lookup_value_type_from_operand(arg) {
                let arg_name = Self::sum_type_name_from_nyar(&arg_ty)
                    .or_else(|| match &arg_ty {
                        NyarType::Named(name) => Some(name.to_string()),
                        NyarType::Apply(base, _) => match base.as_ref() {
                            NyarType::Named(name) => Some(name.to_string()),
                            _ => None,
                        },
                        _ => None,
                    })
                    .unwrap_or_default();
                if !arg_name.is_empty() {
                    let mut by_payload = Vec::new();
                    for (sum_name, tag, is_unite) in &matches {
                        let Some(sum) = self.submission.sum_types.iter().find(|s| s.name == *sum_name)
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
                        let payload_name = Self::sum_type_name_from_nyar(payload).or_else(|| match payload {
                            NyarType::Named(name) => Some(name.to_string()),
                            _ => None,
                        });
                        if let Some(payload_name) = payload_name {
                            if Self::type_name_matches(&payload_name, &arg_name) {
                                by_payload.push((sum_name.clone(), *tag, *is_unite));
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
        // Module-local evidence: `std::…::von::parse_von_value` prefers `VonValue` /
        // `VonTokenKind` over `TokenKind` / `WatToken` when payloads are identical (`utf8`).
        // `term_literal_string` / `term_literal_int` construct `LiteralExpression`, never
        // `MsilTokenKind.String` / `WatTokenKind.String` (shared bare variant names).
        // Ambiguous with no type evidence — refuse to guess (e.g. bare `EndOfFile` without callee type).
        None
    }

    /// Prefer `LiteralExpression` when an AST literal helper builds `String`/`Integer`/….
    ///
    /// Lexer token unites (`MsilTokenKind`, `WatTokenKind`, `WitTokenKind`) share those
    /// variant names with the same `utf8` payload arity, so path scoring alone can tie
    /// or miss when the seed binary / MIR types lack an expected sum.
    fn pick_literal_expression_sum(&self, variant_name: &str, matches: &[(String, u32, bool)]) -> Option<(String, u32, bool)> {
        if !matches!(variant_name, "String" | "Integer" | "Float" | "Bool" | "Null" | "Unit") {
            return None;
        }
        let op_flat = self.operation.to_string().to_ascii_lowercase().replace("::", "_").replace('.', "_");
        if !op_flat.contains("literal") {
            return None;
        }
        let mut literal_hits: Vec<(String, u32, bool)> = matches
            .iter()
            .filter(|(name, ..)| {
                let lower = name.to_ascii_lowercase();
                (lower.contains("literalexpression") || lower.ends_with("literal") || lower.contains("literal_expression"))
                    && !lower.contains("token")
            })
            .cloned()
            .collect();
        if literal_hits.is_empty() {
            literal_hits = matches
                .iter()
                .filter(|(name, ..)| {
                    let lower = name.to_ascii_lowercase();
                    lower.contains("literal") && !lower.contains("token")
                })
                .cloned()
                .collect();
        }
        if literal_hits.len() == 1 {
            return Some(literal_hits.remove(0));
        }
        None
    }

    /// Prefer the sum whose name overlaps the current operation's path (`von` → `VonValue`).
    ///
    /// Path parts are often a single snake_case identifier (`term_literal_string`); split on
    /// `_` so `literal` can score `LiteralExpression` over `MsilTokenKind`/`WatTokenKind`.
    fn pick_operation_local_sum(&self, matches: &[(String, u32, bool)]) -> Option<(String, u32, bool)> {
        let mut parts: Vec<String> = Vec::new();
        for part in self.operation.parts() {
            let lower = part.as_str().to_ascii_lowercase();
            if lower.len() >= 3 {
                parts.push(lower.clone());
            }
            for piece in lower.split('_') {
                if piece.len() >= 3 && !parts.iter().any(|existing| existing == piece) {
                    parts.push(piece.to_string());
                }
            }
        }
        let op_flat = self.operation.to_string().to_ascii_lowercase().replace("::", "_").replace('.', "_");
        if parts.is_empty() && op_flat.is_empty() {
            return None;
        }
        let mut best: Option<(usize, (String, u32, bool))> = None;
        let mut best_tied = false;
        for candidate in matches {
            let sum_lower = candidate.0.to_ascii_lowercase();
            let mut score = parts.iter().filter(|part| sum_lower.contains(part.as_str())).map(|part| part.len()).sum::<usize>();
            // `lower_jvm_opcode_to_flat` + `Label` → prefer `JvmFlatOp` over `JvmExecutableOpcode`.
            // `term_literal_string` + `String` → prefer `LiteralExpression` over `*TokenKind`.
            // Prefer concrete tokens (`flat`, `msil`, `literal`, …) over broad ones like `opcode`.
            for token in
                ["flatop", "flat", "msil", "wasm", "wit", "von", "operand", "instruction", "literal", "expression", "valkyrie", "token"]
            {
                if op_flat.contains(token) && sum_lower.contains(token) {
                    score = score.saturating_add(token.len().saturating_mul(4));
                }
            }
            // AST literal helpers must not lose to lexer TokenKinds that only share the
            // namespace segment `text` (e.g. `std.data.text.msil.MsilTokenKind`).
            if op_flat.contains("literal") && sum_lower.contains("literal") && !sum_lower.contains("token") {
                score = score.saturating_add(64);
            }
            if op_flat.contains("literal") && sum_lower.contains("token") && !op_flat.contains("token") {
                score = score.saturating_sub(score.min(32));
            }
            if score == 0 {
                continue;
            }
            match &best {
                None => {
                    best = Some((score, candidate.clone()));
                    best_tied = false;
                }
                Some((best_score, _)) if score > *best_score => {
                    best = Some((score, candidate.clone()));
                    best_tied = false;
                }
                Some((best_score, _)) if score == *best_score => {
                    best_tied = true;
                }
                _ => {}
            }
        }
        if best_tied {
            // Tie-break: prefer non-token sums when the operation is an AST literal helper.
            if op_flat.contains("literal") && !op_flat.contains("token") {
                let non_token: Vec<_> = matches.iter().filter(|(name, ..)| !name.to_ascii_lowercase().contains("token")).cloned().collect();
                if non_token.len() == 1 {
                    return Some(non_token[0].clone());
                }
                if let Some(literal) = non_token.iter().find(|(name, ..)| {
                    let lower = name.to_ascii_lowercase();
                    lower.contains("literalexpression") || lower.contains("literal")
                }) {
                    return Some(literal.clone());
                }
            }
            return None;
        }
        best.map(|(_, chosen)| chosen)
    }

    fn pick_sum_match(matches: &[(String, u32, bool)], expected: &str) -> Option<(String, u32, bool)> {
        if let Some(exact) = matches.iter().find(|(name, ..)| name == expected) {
            return Some(exact.clone());
        }
        matches.iter().find(|(name, ..)| Self::type_name_matches(name, expected)).cloned()
    }

    fn type_name_matches(a: &str, b: &str) -> bool {
        if a == b {
            return true;
        }
        // Require a `.` / `_` boundary so `Result` does not match `VonParseResult`.
        let boundary_suffix = |hay: &str, needle: &str| -> bool {
            if !hay.ends_with(needle) || hay.len() <= needle.len() {
                return false;
            }
            matches!(hay.as_bytes().get(hay.len() - needle.len() - 1).copied(), Some(b'.' | b'_'))
        };
        boundary_suffix(a, b) || boundary_suffix(b, a)
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

    /// Expected sum type name for an SSA value local (when it is a known sum / Apply(sum, …)).
    fn expected_sum_name_for_value(&self, value: MirValueRef) -> Option<String> {
        let ty = self.lookup_value_type(&value)?;
        self.expected_sum_name_from_nyar(ty)
    }

    fn expected_sum_name_from_nyar(&self, ty: &NyarType) -> Option<String> {
        let name = Self::sum_type_name_from_nyar(ty)?;
        // Prefer exact sum name; do not let `Result` bind `VonParseResult`.
        if let Some(sum) = self.submission.sum_types.iter().find(|sum| sum.name == name) {
            return Some(sum.name.clone());
        }
        self.submission.sum_types.iter().find(|sum| Self::type_name_matches(&sum.name, &name)).map(|sum| sum.name.clone())
    }

    /// Resolve expected sum for a variant ctor Call from output / return / payload evidence.
    fn expected_sum_for_variant_ctor(&self, path: &NamePath, output: Option<MirValueRef>, arguments: &[MirOperand]) -> Option<String> {
        let variant_name = path.parts().last()?.as_str();
        if let Some(value) = output {
            if let Some(name) = self.expected_sum_name_for_value(value) {
                if self.sum_has_variant(&name, variant_name) {
                    return Some(name);
                }
            }
        }
        if let Some(name) = self.expected_sum_name_from_nyar(&self.mir_fn.return_type) {
            if self.sum_has_variant(&name, variant_name) {
                return Some(name);
            }
        }
        let _ = arguments;
        None
    }

    /// Bare `None()` / `Some(x)` when sum resolution cannot pick a tagged Option.
    ///
    /// Used for `T?` (`Union(T, null)` → MSIL `object`) and erased `Option<T>`. Returns
    /// `Ok(true)` when the Call was lowered; `Ok(false)` to continue normal Call lowering.
    fn try_emit_nullable_option_ctor(&mut self, path: &NamePath, arguments: &[MirOperand], output: Option<MirValueRef>) -> Result<bool> {
        if path.parts().len() != 1 {
            return Ok(false);
        }
        let name = path.parts()[0].as_str();
        if !matches!(name, "None" | "Some") {
            return Ok(false);
        }
        if !self.nullable_option_ctor_context(output) {
            // After sum resolution fails, bare `None()` must still become `ldnull` for `T?`
            // (e.g. `von_parse_take_fail`) — never fall through to unknown_call_signature.
            if name != "None" || !arguments.is_empty() {
                return Ok(false);
            }
        }
        match name {
            "None" if arguments.is_empty() => {
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ldnull, operand: None });
                if let Some(output) = output {
                    self.store_call_result(output, &MsilType::Object);
                }
                Ok(true)
            }
            "Some" if arguments.len() == 1 => {
                self.emit_operand(&arguments[0]);
                let msil = self.msil_type_of_operand(&arguments[0]);
                if let Some(type_operand) = self.box_type_operand_for_payload(&msil) {
                    self.instructions.push(MsilInstruction {
                        label: None,
                        opcode: MsilOpcode::Box,
                        operand: Some(MsilInstructionOperand::Type(type_operand)),
                    });
                }
                if let Some(output) = output {
                    self.store_call_result(output, &MsilType::Object);
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// True when the Call result / enclosing return looks like `T?` or `Option<_>`.
    fn nullable_option_ctor_context(&self, output: Option<MirValueRef>) -> bool {
        if let Some(value) = output {
            if let Some(ty) = self.lookup_value_type(&value) {
                if Self::nyar_is_nullable_or_option(ty) {
                    return true;
                }
            }
            // Slot may already be planned as `object` for `T?`.
            if let Some(&local) = self.slots.value_locals.get(&value) {
                if matches!(self.slots.local_types.get(local as usize), Some(MsilType::Object)) {
                    return true;
                }
            }
        }
        Self::nyar_is_nullable_or_option(&self.mir_fn.return_type)
    }

    fn nyar_is_nullable_or_option(ty: &NyarType) -> bool {
        match ty {
            NyarType::Union(items) => items.iter().any(|item| matches!(item, NyarType::Named(name) if name.as_str() == "null")),
            NyarType::Apply(base, args) => matches!(base.as_ref(), NyarType::Named(name) if name.as_str() == "Option") && args.len() == 1,
            NyarType::Named(name) if matches!(name.as_str(), "Option" | "null") => true,
            _ => false,
        }
    }

    fn sum_has_variant(&self, sum_name: &str, variant_name: &str) -> bool {
        self.submission
            .sum_types
            .iter()
            .find(|sum| sum.name == sum_name || Self::type_name_matches(&sum.name, sum_name))
            .is_some_and(|sum| sum.variants.iter().any(|v| v.name == variant_name))
    }

    /// MIR parameter sum-type names for a callee path (aligned with stacked Call args).
    fn resolve_callee_param_sum_names(&self, path: &NamePath) -> Vec<Option<String>> {
        let simple = path.parts().last().map(|part| part.as_str());
        let Some(simple) = simple
        else {
            return Vec::new();
        };
        let Some(exec) = self.submission.executable.as_ref()
        else {
            return Vec::new();
        };
        for operation in exec.operations() {
            if operation.parts().last().map(|part| part.as_str()) != Some(simple) {
                continue;
            }
            let Some(view) = exec.get_function(&operation)
            else {
                continue;
            };
            return view.function.param_types.iter().map(|ty| self.expected_sum_name_from_nyar(ty)).collect();
        }
        Vec::new()
    }

    fn sum_type_name_for_operand(&self, operand: &MirOperand) -> Option<String> {
        if let Some(ty) = self.lookup_value_type_from_operand(operand) {
            if let Some(name) = Self::sum_type_name_from_nyar(&ty) {
                if let Some(sum) = self.submission.sum_types.iter().find(|sum| sum.name == name || Self::type_name_matches(&sum.name, &name)) {
                    return Some(sum.name.clone());
                }
            }
        }
        // Call results often stay `object` in locals while MIR still names the unite class.
        if let MsilType::Named(name) = self.msil_type_of_operand(operand) {
            if let Some(sum) = self.submission.sum_types.iter().find(|sum| {
                sum.name == *name || Self::type_name_matches(&sum.name, name.as_str()) || self.ctx.clr_qualified_type_name(&sum.name) == *name
            }) {
                return Some(sum.name.clone());
            }
        }
        None
    }

    /// Allocate a tagged sum (`tag` + `payload`) for a variant constructor call.
    fn emit_sum_variant_ctor(
        &mut self,
        sum_name: &str,
        tag: u32,
        is_unite: bool,
        arguments: &[MirOperand],
        output: Option<MirValueRef>,
    ) -> Result<()> {
        let qualified = self.ctx.clr_qualified_type_name(sum_name);
        if is_unite {
            // Reference class: `newobj` then set `tag` / `payload`.
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Newobj,
                operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                    owner: Some(qualified.clone()),
                    name: ".ctor".to_string(),
                    signature: MsilMethodSignature::new(MsilType::Void, Vec::new()),
                })),
            });
            self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Dup, operand: None });
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::LdcI4,
                operand: Some(MsilInstructionOperand::Integer(tag as i64)),
            });
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Stfld,
                operand: Some(MsilInstructionOperand::Field(qualified.clone(), "tag".to_string())),
            });
            self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Dup, operand: None });
            self.emit_sum_variant_payload(arguments);
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Stfld,
                operand: Some(MsilInstructionOperand::Field(qualified.clone(), "payload".to_string())),
            });
            if let Some(output) = output {
                // Slot planning often maps missing MIR types to Unit→int32; Fine/Fail must
                // retype the SSA local to the sum class or `ret` / later uses InvalidProgram.
                self.store_call_result(output, &MsilType::Named(qualified));
            }
        }
        else {
            let local = output
                .and_then(|value| self.slots.value_locals.get(&value).copied())
                .ok_or_else(|| miette!(code = "nyar::clr::missing_sum_local", "CLR sum variant ctor for `{sum_name}` needs an output local"))?;
            self.emit_ldloca(local);
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Initobj,
                operand: Some(MsilInstructionOperand::Type(qualified.clone())),
            });
            // Value-type sums keep the planned valuetype local; ensure slot type matches.
            if let Some(slot) = self.slots.local_types.get_mut(local as usize) {
                *slot = MsilType::Named(qualified.clone());
            }
            self.emit_ldloca(local);
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::LdcI4,
                operand: Some(MsilInstructionOperand::Integer(tag as i64)),
            });
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Stfld,
                operand: Some(MsilInstructionOperand::Field(qualified.clone(), "tag".to_string())),
            });
            self.emit_ldloca(local);
            self.emit_sum_variant_payload(arguments);
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Stfld,
                operand: Some(MsilInstructionOperand::Field(qualified, "payload".to_string())),
            });
        }
        Ok(())
    }

    fn emit_sum_variant_payload(&mut self, arguments: &[MirOperand]) {
        match arguments {
            [] => {
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ldnull, operand: None });
            }
            [only] => {
                self.emit_operand(only);
                // Payload slot is `object`; box value-typed payloads only when the Named
                // type has a real aggregate layout. Bare names like `T` are uninstantiated
                // generics (already object-erased on the evaluation stack).
                let msil = self.msil_type_of_operand(only);
                if let Some(type_operand) = self.box_type_operand_for_payload(&msil) {
                    self.instructions.push(MsilInstruction {
                        label: None,
                        opcode: MsilOpcode::Box,
                        operand: Some(MsilInstructionOperand::Type(type_operand)),
                    });
                }
            }
            many => {
                // Multi-field variants: keep first arg as payload for now (tuple packing is later).
                self.emit_operand(&many[0]);
            }
        }
    }

    /// Element type for `ldelem`/`stelem`: prefer CLR `SzArray` slots, else MIR `[T]`.
    fn array_element_msil_type(&self, array: &MirOperand) -> MsilType {
        match self.msil_type_of_operand(array) {
            MsilType::SzArray(inner) => *inner,
            _ => match self.lookup_value_type_from_operand(array) {
                Some(NyarType::Array(inner)) => nyar_type_to_msil(inner.as_ref(), &self.submission.aggregate_layouts),
                Some(NyarType::FixedArray { element, .. }) => nyar_type_to_msil(element.as_ref(), &self.submission.aggregate_layouts),
                _ => MsilType::Object,
            },
        }
    }

    fn operand_is_clr_utf8_string(&self, operand: &MirOperand) -> bool {
        if let Some(ty) = self.lookup_value_type_from_operand(operand) {
            if matches!(ty, NyarType::Utf16) {
                return false;
            }
            if matches!(ty, NyarType::Utf8) {
                return true;
            }
        }
        false
    }

    /// `utf8._repr` → `Encoding.UTF8.GetBytes(string)` when ABI is `System.String`.
    fn try_emit_utf8_repr_bytes(&mut self, object: &MirOperand, output: MirValueRef) -> bool {
        if !self.operand_is_clr_utf8_string(object) {
            return false;
        }
        let bytes_ty = MsilType::sz_array(MsilType::Int8 { signed: false });
        self.emit_call_encoding_utf8_get_bytes(object);
        self.store_call_result(output, &bytes_ty);
        true
    }

    fn emit_call_encoding_utf8_get_bytes(&mut self, string_operand: &MirOperand) {
        // call Encoding Encoding::get_UTF8()
        self.instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Call,
            operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                owner: Some("[mscorlib]System.Text.Encoding".to_string()),
                name: "get_UTF8".to_string(),
                signature: MsilMethodSignature::new(MsilType::Named("[mscorlib]System.Text.Encoding".to_string()), Vec::new()),
            })),
        });
        self.emit_operand(string_operand);
        self.instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Callvirt,
            operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                owner: Some("[mscorlib]System.Text.Encoding".to_string()),
                name: "GetBytes".to_string(),
                signature: MsilMethodSignature::new_instance(MsilType::sz_array(MsilType::Int8 { signed: false }), vec![MsilType::String]),
            })),
        });
    }

    fn emit_encoding_utf8_get_byte_count(&mut self, string_operand: &MirOperand, output: Option<MirValueRef>) {
        self.instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Call,
            operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                owner: Some("[mscorlib]System.Text.Encoding".to_string()),
                name: "get_UTF8".to_string(),
                signature: MsilMethodSignature::new(MsilType::Named("[mscorlib]System.Text.Encoding".to_string()), Vec::new()),
            })),
        });
        self.emit_operand(string_operand);
        self.instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Callvirt,
            operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                owner: Some("[mscorlib]System.Text.Encoding".to_string()),
                name: "GetByteCount".to_string(),
                signature: MsilMethodSignature::new_instance(MsilType::Int32 { signed: true }, vec![MsilType::String]),
            })),
        });
        if let Some(output) = output {
            self.store_call_result(output, &MsilType::Int32 { signed: true });
        }
    }

    /// Map common **UTF-16** / host-string instance methods onto `System.String`.
    ///
    /// `Utf8Text.length` / `slice` / indexing are Unicode **scalars** (std). They must
    /// **not** lower to `get_Length` / `Substring` without scalar↔code-unit conversion
    /// (see `std.adaptor.clr.text.Utf8Text`). Only `Utf16Text` owns the raw code-unit API.
    ///
    /// Returns `true` when the call was fully lowered.
    fn try_emit_string_method(&mut self, path: &NamePath, arguments: &[MirOperand], output: Option<MirValueRef>) -> bool {
        let name = path.parts().last().map(|part| part.as_str()).unwrap_or_default();
        let Some(receiver) = arguments.first()
        else {
            return false;
        };
        let receiver_ty = self.msil_type_of_operand(receiver);
        let nyar_ty = self.lookup_value_type_from_operand(receiver);
        let receiver_is_utf16 = nyar_ty.as_ref().is_some_and(|ty| matches!(ty, NyarType::Utf16));
        let receiver_is_utf8 = nyar_ty.as_ref().is_some_and(|ty| matches!(ty, NyarType::Utf8));
        let receiver_is_string = receiver_is_utf16 || receiver_is_utf8;
        if !receiver_is_string {
            return false;
        }
        // CLR bootstrap: wasm `env.utf8_*` stubs are repaired to BCL string ops in
        // `repair_wasm_utf8_host_stubs_on_clr`. Until `std.adaptor.clr.text` host_provider
        // is always linked, route utf8 `.length()` through the same BCL surface here.
        if name == "length" && arguments.len() == 1 && receiver_is_utf8 {
            self.emit_operand(receiver);
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Callvirt,
                operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                    owner: Some("[mscorlib]System.String".to_string()),
                    name: "get_Length".to_string(),
                    signature: MsilMethodSignature::new_instance(MsilType::Int32 { signed: true }, Vec::new()),
                })),
            });
            if let Some(output) = output {
                self.store_call_result(output, &MsilType::Int32 { signed: true });
            }
            return true;
        }
        // `Utf8Text.byte_length` is UTF-8 byte count — not UTF-16 `get_Length`.
        if name == "byte_length" && arguments.len() == 1 && receiver_is_utf8 {
            self.emit_encoding_utf8_get_byte_count(receiver, output);
            return true;
        }
        // Index-sensitive APIs: prefer host_provider for true UTF-8 scalar semantics.
        // Until `std.adaptor.clr.text` is linked, CLR maps utf8→System.String and the
        // repaired `env.utf8_*` stubs already use BCL code-unit ops — match that here
        // so we do not emit unresolved local `slice`/`index_of` MethodRefs.
        let index_sensitive = matches!(name, "length" | "slice" | "index_of" | "last_index_of" | "suffix []" | "suffix ⁅⁆");
        if index_sensitive && !receiver_is_utf16 && !receiver_is_utf8 {
            return false;
        }
        let string_owner = "[mscorlib]System.String".to_string();
        match name {
            "length" if arguments.len() == 1 => {
                self.emit_operand(receiver);
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Callvirt,
                    operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                        owner: Some(string_owner),
                        name: "get_Length".to_string(),
                        signature: MsilMethodSignature::new_instance(MsilType::Int32 { signed: true }, Vec::new()),
                    })),
                });
                if let Some(output) = output {
                    self.store_call_result(output, &MsilType::Int32 { signed: true });
                }
                true
            }
            // utf8/utf16 cardinal index → Substring(i, 1); ordinal → Substring(i-1, 1).
            "suffix []" if arguments.len() >= 2 => {
                self.emit_operand(receiver);
                self.emit_operand(&arguments[1]);
                if !matches!(self.msil_type_of_operand(&arguments[1]), MsilType::Int32 { signed: true }) {
                    self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::ConvI4, operand: None });
                }
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_1, operand: None });
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Sub, operand: None });
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_1, operand: None });
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Callvirt,
                    operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                        owner: Some(string_owner),
                        name: "Substring".to_string(),
                        signature: MsilMethodSignature::new_instance(
                            MsilType::String,
                            vec![MsilType::Int32 { signed: true }, MsilType::Int32 { signed: true }],
                        ),
                    })),
                });
                if let Some(output) = output {
                    self.store_call_result(output, &MsilType::String);
                }
                true
            }
            "suffix ⁅⁆" if arguments.len() >= 2 => {
                self.emit_operand(receiver);
                self.emit_operand(&arguments[1]);
                if !matches!(self.msil_type_of_operand(&arguments[1]), MsilType::Int32 { signed: true }) {
                    self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::ConvI4, operand: None });
                }
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_1, operand: None });
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Callvirt,
                    operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                        owner: Some(string_owner),
                        name: "Substring".to_string(),
                        signature: MsilMethodSignature::new_instance(
                            MsilType::String,
                            vec![MsilType::Int32 { signed: true }, MsilType::Int32 { signed: true }],
                        ),
                    })),
                });
                if let Some(output) = output {
                    self.store_call_result(output, &MsilType::String);
                }
                true
            }
            "last_index_of" | "index_of" if arguments.len() >= 2 => {
                let clr_name = if name == "last_index_of" { "LastIndexOf" } else { "IndexOf" };
                self.emit_operand(receiver);
                self.emit_operand(&arguments[1]);
                let needle_ty = self.msil_type_of_operand(&arguments[1]);
                let param_ty = match needle_ty {
                    MsilType::Char => MsilType::Char,
                    MsilType::String => MsilType::String,
                    other => {
                        // Value → box → Convert.ToString → LastIndexOf(string)
                        if !matches!(other, MsilType::Object | MsilType::String) {
                            let type_name = match &other {
                                MsilType::Int32 { signed: true } => Some("int32"),
                                MsilType::Int32 { signed: false } => Some("uint32"),
                                MsilType::Int64 { signed: true } => Some("int64"),
                                MsilType::Bool => Some("bool"),
                                MsilType::Named(n) if n.starts_with('[') => Some(n.as_str()),
                                MsilType::Named(n)
                                    if self.submission.aggregate_layouts.value_type_names.contains(n)
                                        || self.submission.aggregate_layouts.type_name_to_layout.contains_key(n) =>
                                {
                                    Some(n.as_str())
                                }
                                _ => None,
                            };
                            if let Some(type_name) = type_name {
                                self.instructions.push(MsilInstruction {
                                    label: None,
                                    opcode: MsilOpcode::Box,
                                    operand: Some(MsilInstructionOperand::Type(type_name.to_string())),
                                });
                            }
                        }
                        self.instructions.push(MsilInstruction {
                            label: None,
                            opcode: MsilOpcode::Call,
                            operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                                owner: Some("[mscorlib]System.Convert".to_string()),
                                name: "ToString".to_string(),
                                signature: MsilMethodSignature::new(MsilType::String, vec![MsilType::Object]),
                            })),
                        });
                        MsilType::String
                    }
                };
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Callvirt,
                    operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                        owner: Some(string_owner),
                        name: clr_name.to_string(),
                        signature: MsilMethodSignature::new_instance(MsilType::Int32 { signed: true }, vec![param_ty]),
                    })),
                });
                if let Some(output) = output {
                    self.store_call_result(output, &MsilType::Int32 { signed: true });
                }
                true
            }
            "contains" if arguments.len() >= 2 => {
                self.emit_operand(receiver);
                self.emit_operand(&arguments[1]);
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Callvirt,
                    operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                        owner: Some(string_owner),
                        name: "Contains".to_string(),
                        signature: MsilMethodSignature::new_instance(MsilType::Bool, vec![MsilType::String]),
                    })),
                });
                if let Some(output) = output {
                    self.store_call_result(output, &MsilType::Bool);
                }
                true
            }
            "trim" if arguments.len() == 1 => {
                self.emit_operand(receiver);
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Callvirt,
                    operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                        owner: Some(string_owner),
                        name: "Trim".to_string(),
                        signature: MsilMethodSignature::new_instance(MsilType::String, Vec::new()),
                    })),
                });
                if let Some(output) = output {
                    self.store_call_result(output, &MsilType::String);
                }
                true
            }
            "starts_with" | "ends_with" if arguments.len() >= 2 => {
                let clr_name = if name == "starts_with" { "StartsWith" } else { "EndsWith" };
                self.emit_operand(receiver);
                self.emit_operand(&arguments[1]);
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Callvirt,
                    operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                        owner: Some(string_owner),
                        name: clr_name.to_string(),
                        signature: MsilMethodSignature::new_instance(MsilType::Bool, vec![MsilType::String]),
                    })),
                });
                if let Some(output) = output {
                    self.store_call_result(output, &MsilType::Bool);
                }
                true
            }
            "replace" if arguments.len() >= 3 => {
                self.emit_operand(receiver);
                self.emit_operand(&arguments[1]);
                self.emit_operand(&arguments[2]);
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Callvirt,
                    operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                        owner: Some(string_owner),
                        name: "Replace".to_string(),
                        signature: MsilMethodSignature::new_instance(MsilType::String, vec![MsilType::String, MsilType::String]),
                    })),
                });
                if let Some(output) = output {
                    self.store_call_result(output, &MsilType::String);
                }
                true
            }
            "equals" if arguments.len() >= 2 => {
                self.emit_operand(receiver);
                self.emit_operand(&arguments[1]);
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Callvirt,
                    operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                        owner: Some(string_owner),
                        name: "Equals".to_string(),
                        signature: MsilMethodSignature::new_instance(MsilType::Bool, vec![MsilType::String]),
                    })),
                });
                if let Some(output) = output {
                    self.store_call_result(output, &MsilType::Bool);
                }
                true
            }
            "slice" if arguments.len() >= 3 => {
                // Utf16Text.slice(start, count) → String.Substring (code units).
                // Utf8Text.slice is scalar — rejected above via index_sensitive.
                self.emit_operand(receiver);
                self.emit_operand(&arguments[1]);
                self.emit_operand(&arguments[2]);
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Callvirt,
                    operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                        owner: Some(string_owner),
                        name: "Substring".to_string(),
                        signature: MsilMethodSignature::new_instance(
                            MsilType::String,
                            vec![MsilType::Int32 { signed: true }, MsilType::Int32 { signed: true }],
                        ),
                    })),
                });
                if let Some(output) = output {
                    self.store_call_result(output, &MsilType::String);
                }
                true
            }
            "concat" if arguments.len() >= 2 => {
                self.emit_string_concat(arguments, output);
                true
            }
            _ => false,
        }
    }

    fn operand_is_stringish(&self, operand: &MirOperand) -> bool {
        self.lookup_value_type_from_operand(operand).is_some_and(|ty| matches!(ty, NyarType::Utf8 | NyarType::Utf16))
    }

    fn is_string_concat_op(&self, opcode: IntrinsicOpcode, arguments: &[MirOperand]) -> bool {
        // Both sides must be stringish — `.any()` wrongly sent `index + 1` (int) through
        // `String.Concat` when a poisoned string-typed slot shared the CFG.
        matches!(opcode, IntrinsicOpcode::Binary(IntrinsicBinaryOp::Add))
            && arguments.len() >= 2
            && self.operand_is_stringish(&arguments[0])
            && self.operand_is_stringish(&arguments[1])
    }

    fn emit_string_concat(&mut self, arguments: &[MirOperand], output: Option<MirValueRef>) {
        // `String.Concat(string, string)` — binary path join / utf8 `+`.
        let left = &arguments[0];
        let right = &arguments[1];
        self.emit_operand(left);
        if !matches!(self.msil_type_of_operand(left), MsilType::String) {
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Castclass,
                operand: Some(MsilInstructionOperand::Type("[mscorlib]System.String".to_string())),
            });
        }
        self.emit_operand(right);
        if !matches!(self.msil_type_of_operand(right), MsilType::String) {
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Castclass,
                operand: Some(MsilInstructionOperand::Type("[mscorlib]System.String".to_string())),
            });
        }
        self.emit_string_concat_on_stack(output);
    }

    fn emit_string_concat_on_stack(&mut self, output: Option<MirValueRef>) {
        self.instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Call,
            operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                owner: Some("[mscorlib]System.String".to_string()),
                name: "Concat".to_string(),
                signature: MsilMethodSignature::new(MsilType::String, vec![MsilType::String, MsilType::String]),
            })),
        });
        if let Some(output) = output {
            self.store_call_result(output, &MsilType::String);
        }
    }

    fn infer_aggregate_name(&self, operand: &MirOperand) -> Option<String> {
        match operand {
            MirOperand::Value(value) => {
                if let Some(ty) = self.lookup_value_type(value) {
                    if let NyarType::Named(name) = ty {
                        return Some(name.to_string());
                    }
                    if let Some(layout) = self.ctx.layout_for_value_type(ty) {
                        return Some(layout.name.clone());
                    }
                }
                if let Some(local) = self.slots.value_locals.get(value).copied() {
                    if let Some(name) = self.aggregate_name_for_local(local) {
                        return Some(name);
                    }
                }
                self.infer_singleton_self_type_name(value)
            }
            MirOperand::Symbol(path) => {
                let local = self.slots.var_locals.get(&path.to_string()).copied()?;
                self.aggregate_name_for_local(local)
            }
            _ => None,
        }
    }

    /// Expand `tuple_get_N(tuple)` to `ldfld __tuple_…::N` and retype the result.
    fn emit_tuple_get_field(&mut self, tuple: &MirOperand, callee: &str, output: Option<MirValueRef>) -> Result<()> {
        let index = callee
            .strip_prefix("tuple_get_")
            .and_then(|suffix| suffix.parse::<usize>().ok())
            .ok_or_else(|| miette!(code = "nyar::clr::bad_tuple_get", "CLR 无法解析函数 `{}` 中的 `{callee}`", self.operation))?;
        let field_name = index.to_string();
        let debug_tuple_type = self.lookup_value_type_from_operand(tuple);
        let debug_tuple_name = self.infer_aggregate_name(tuple);
        let owner = self
            .infer_aggregate_name(tuple)
            .filter(|name| name.starts_with("__tuple_"))
            .or_else(|| {
                self.lookup_value_type_from_operand(tuple).and_then(|ty| self.ctx.layout_for_value_type(&ty).map(|layout| layout.name.clone()))
            })
            .ok_or_else(|| {
                miette!(
                    code = "nyar::clr::tuple_get_unknown_tuple",
                    help = "tuple_get_N 需要已知的 `__tuple_…` layout",
                    "CLR 无法降低函数 `{}` 中对未知元组的 `{callee}`",
                    self.operation
                )
            })?;
        let qualified = self.ctx.clr_qualified_type_name(&owner);
        let field_ty = self
            .ctx
            .layout_by_type_name(&owner)
            .and_then(|layout| layout.fields.get(index))
            .map(|field| nyar_type_to_msil(&field.ty, &self.submission.aggregate_layouts))
            .unwrap_or(MsilType::Object);
        // Value-type tuples: ldloca + ldfld; class / boxed: ldloc + ldfld.
        let is_valuetype = self.ctx.layout_by_type_name(&owner).is_some_and(|layout| layout.storage == StorageKind::Value);
        if is_valuetype {
            if let Some(local) = self.operand_local(tuple) {
                self.emit_ldloca(local);
            }
            else {
                self.emit_operand(tuple);
            }
        }
        else {
            self.emit_operand(tuple);
        }
        self.instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Ldfld,
            operand: Some(MsilInstructionOperand::Field(qualified, field_name)),
        });
        if let Some(output) = output {
            self.store_call_result(output, &field_ty);
        }
        Ok(())
    }

    /// 从 slot plan 的 local 类型中提取聚合类型名。
    ///
    /// 直接匹配 `MsilType::Named`；同时兼容 `MsilType::SzArray(Named(X))`——
    /// 当 slot plan 将单个结构体变量误记为数组类型时，仍能恢复内层元素类型名，
    /// 使 `FieldGet`/`FieldSet` 的 owner 解析到正确的 `FieldDef` token。
    fn aggregate_name_for_local(&self, local: u16) -> Option<String> {
        match self.slots.local_types.get(local as usize)? {
            MsilType::Named(name) => Some(name.clone()),
            MsilType::SzArray(inner) => match inner.as_ref() {
                MsilType::Named(name) => Some(name.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    /// 当 `value` 是入口块第一个参数（singleton 方法的 `self`）且操作名为
    /// `SingletonName.method` 形式时，返回 `SingletonName` 作为类型名。
    ///
    /// 这覆盖了 `self` 参数类型未被精确标注为 `Named` 的场景：singleton 实例方法
    /// 的 owner 类型可以直接从操作名前缀恢复。
    fn infer_singleton_self_type_name(&self, value: &MirValueRef) -> Option<String> {
        let entry_block = self.mir_fn.blocks.get(self.mir_fn.entry.0 as usize)?;
        let is_first_param = entry_block.parameters.first().is_some_and(|first| first == value);
        if !is_first_param {
            return None;
        }
        self.enclosing_type_name_from_operation()
    }

    /// Resolve MIR `Self` / bare construct names to a PE TypeDef owner.
    fn resolve_struct_type_name(&self, type_name: &str, layout_id: Option<u32>) -> String {
        if type_name != "Self" {
            return type_name.to_string();
        }
        if let Some(id) = layout_id {
            if let Some(layout) = self.ctx.layout_by_id(id) {
                return layout.name.clone();
            }
        }
        if let NyarType::Named(name) = &self.mir_fn.return_type {
            if name.as_str() != "Self" {
                return name.to_string();
            }
        }
        if let Some(owner) = self.enclosing_type_name_from_operation() {
            return owner;
        }
        type_name.to_string()
    }

    /// `…Type.method` → `Type` (penultimate path segment).
    fn enclosing_type_name_from_operation(&self) -> Option<String> {
        let parts = self.operation.parts();
        if parts.len() < 2 {
            return None;
        }
        Some(parts[parts.len() - 2].as_str().to_string())
    }

    fn lookup_value_type_from_operand(&self, operand: &MirOperand) -> Option<NyarType> {
        match operand {
            MirOperand::Value(value) => self.lookup_value_type(value).cloned(),
            MirOperand::Symbol(path) => self.find_named_value(path).and_then(|value| self.lookup_value_type(&value).cloned()),
            _ => None,
        }
    }

    /// Type of a call callee when it is a function value (parameter / local), not a MethodDef.
    fn callee_function_type(&self, callee: &MirOperand) -> Option<NyarFunctionType> {
        match self.lookup_value_type_from_operand(callee)? {
            NyarType::Function(func) => Some(*func),
            _ => None,
        }
    }

    fn find_named_value(&self, path: &NamePath) -> Option<MirValueRef> {
        if path.parts().len() != 1 {
            return None;
        }
        let name = path.parts()[0].as_str();
        self.mir_fn.values.iter().find_map(|value| match &value.origin {
            ValueOrigin::Parameter { name: n, .. }
            | ValueOrigin::LetBinding { name: n }
            | ValueOrigin::BlockParameter { name: n, .. }
            | ValueOrigin::MutRefBinding { name: n }
            | ValueOrigin::PinMutRefBinding { name: n }
                if n == name =>
            {
                Some(value.id)
            }
            _ => None,
        })
    }

    /// Indirect call ABI for `NyarType::Function` → `System.Delegate::DynamicInvoke`.
    fn emit_function_value_invoke(
        &mut self,
        callee: &MirOperand,
        arguments: &[MirOperand],
        output: Option<MirValueRef>,
        fn_ty: &NyarFunctionType,
    ) -> Result<()> {
        if matches!(callee, MirOperand::Symbol(_)) && self.operand_local(callee).is_none() {
            return Err(miette!(code = "nyar::clr::unknown_function_value", "CLR 无法加载函数值 callee `{callee:?}`（缺少局部槽）"));
        }
        if let MirOperand::Value(value) = callee {
            if !self.slots.value_locals.contains_key(value) {
                return Err(miette!(code = "nyar::clr::unknown_function_value", "CLR 无法加载函数值 SSA 局部（缺少 value_locals 映射）"));
            }
        }

        self.emit_operand(callee);
        self.instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Castclass,
            operand: Some(MsilInstructionOperand::Type("[mscorlib]System.Delegate".to_string())),
        });

        self.instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::LdcI4,
            operand: Some(MsilInstructionOperand::Integer(arguments.len() as i64)),
        });
        self.instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Newarr,
            operand: Some(MsilInstructionOperand::Type("object".to_string())),
        });
        for (index, argument) in arguments.iter().enumerate() {
            self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Dup, operand: None });
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::LdcI4,
                operand: Some(MsilInstructionOperand::Integer(index as i64)),
            });
            self.emit_boxed_object_operand(argument);
            self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::StelemRef, operand: None });
        }

        self.instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Callvirt,
            operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                owner: Some("[mscorlib]System.Delegate".to_string()),
                name: "DynamicInvoke".to_string(),
                signature: MsilMethodSignature::new_instance(MsilType::Object, vec![MsilType::sz_array(MsilType::Object)]),
            })),
        });

        if matches!(fn_ty.return_type, NyarType::Unit) {
            self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Pop, operand: None });
            if let Some(output) = output {
                self.emit_load_constant(&MirConstant::Unit);
                self.store_to_value(output);
            }
        }
        else if let Some(output) = output {
            self.store_call_result(output, &MsilType::Object);
        }
        else {
            self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Pop, operand: None });
        }
        Ok(())
    }

    /// Push `argument` as `object` (box value types) for `object[]` / DynamicInvoke.
    fn emit_boxed_object_operand(&mut self, argument: &MirOperand) {
        self.emit_operand(argument);
        let msil = self.msil_type_of_operand(argument);
        if let Some(type_operand) = self.box_type_operand_for_payload(&msil) {
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Box,
                operand: Some(MsilInstructionOperand::Type(type_operand)),
            });
        }
    }

    /// Type token for `box` of a sum/DynamicInvoke payload, or `None` when the value is
    /// already a reference (including erased type parameters with no aggregate layout).
    fn box_type_operand_for_payload(&self, msil: &MsilType) -> Option<String> {
        match msil {
            MsilType::Bool => Some("bool".to_string()),
            MsilType::Char => Some("char".to_string()),
            MsilType::Int8 { signed: true } => Some("int8".to_string()),
            MsilType::Int8 { signed: false } => Some("uint8".to_string()),
            MsilType::Int16 { signed: true } => Some("int16".to_string()),
            MsilType::Int16 { signed: false } => Some("uint16".to_string()),
            MsilType::Int32 { signed: true } => Some("int32".to_string()),
            MsilType::Int32 { signed: false } => Some("uint32".to_string()),
            MsilType::Int64 { signed: true } => Some("int64".to_string()),
            MsilType::Int64 { signed: false } => Some("uint64".to_string()),
            MsilType::Float32 => Some("float32".to_string()),
            MsilType::Float64 => Some("float64".to_string()),
            MsilType::Named(name) => {
                let plan = &self.submission.aggregate_layouts;
                if plan.value_type_names.contains(name) || plan.type_name_to_layout.contains_key(name) {
                    Some(self.ctx.clr_qualified_type_name(name))
                }
                else if self.clr_named_is_valuetype_sum(name) {
                    Some(self.ctx.clr_qualified_type_name(name))
                }
                else {
                    None
                }
            }
            _ => None,
        }
    }

    /// `box` token when passing a valuetype (incl. nullary enum sums like `EndOfFile`) to an
    /// `object` parameter. `msil_type_of_operand(Symbol)` is often `object` even though
    /// `emit_sum_variant_value_on_stack` left a valuetype on the evaluation stack.
    fn box_token_for_object_param(&self, argument: &MirOperand, expected_sum: Option<&str>) -> Option<String> {
        let msil = self.msil_type_of_operand(argument);
        if let Some(token) = self.box_type_operand_for_payload(&msil) {
            return Some(token);
        }
        if let MirOperand::Symbol(path) = argument {
            if let Some((sum_name, _tag, is_unite)) = self.resolve_sum_variant_ctor(path, expected_sum, &[]) {
                if !is_unite {
                    return Some(self.ctx.clr_qualified_type_name(&sum_name));
                }
            }
        }
        if let Some(sum) = expected_sum {
            if self.clr_named_is_valuetype_sum(sum) {
                return Some(self.ctx.clr_qualified_type_name(sum));
            }
        }
        None
    }

    /// Non-unite sum TypeDefs are emitted as CLR valuetypes (`initobj` + `ldfld tag`).
    fn clr_named_is_valuetype_sum(&self, name: &str) -> bool {
        let simple = name.rsplit(['.', '/']).next().unwrap_or(name);
        self.submission.sum_types.iter().any(|sum| {
            !sum.is_unite
                && (sum.name == name
                    || sum.name == simple
                    || Self::type_name_matches(&sum.name, name)
                    || Self::type_name_matches(&sum.name, simple))
        })
    }

    /// CLR null-test for injected bare `is_null` (not `c_str.is_null` / other MethodDefs).
    ///
    /// Evidence from the evaluation-stack / local slot type:
    /// - reference (`object` / `string` / `T[]` / class) → `ldnull` + `ceq`
    /// - valuetype / primitive → never null → `ldc.i4.0`
    /// - unite extractor payload tracked in `unite_payload_tag_ok` → `!tag_ok` (miss ⇒ "null")
    fn emit_clr_is_null(&mut self, argument: &MirOperand, output: Option<MirValueRef>) {
        if let MirOperand::Value(value) = argument {
            if let Some(&tag_ok) = self.unite_payload_tag_ok.get(value) {
                // Option-shaped probe: tag mismatch ⇒ treat as null so `!is_null` is the match.
                self.emit_ldloc(tag_ok);
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None });
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ceq, operand: None });
                if let Some(output) = output {
                    self.store_call_result(output, &MsilType::Bool);
                }
                return;
            }
        }
        let ty = self.msil_type_of_operand(argument);
        let is_reference = match &ty {
            MsilType::Object | MsilType::String | MsilType::SzArray(_) => true,
            MsilType::Named(name) => !self.submission.aggregate_layouts.value_type_names.contains(name),
            _ => false,
        };
        if is_reference {
            self.emit_operand(argument);
            self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ldnull, operand: None });
            self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ceq, operand: None });
        }
        else {
            self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None });
        }
        if let Some(output) = output {
            self.store_call_result(output, &MsilType::Bool);
        }
    }

    /// Unite Fine/Fail extractor: `ldfld tag` + compare, then `ldfld payload` + narrow only on hit.
    ///
    /// Returns `true` when this path handled the FieldGet.
    fn try_emit_unite_tagged_payload_get(
        &mut self,
        object: &MirOperand,
        storage: StorageKind,
        layout_id: Option<nyar_types::LayoutId>,
        output: MirValueRef,
    ) -> Result<bool> {
        let Some((sum_name, expected_tag, out_msil)) = self.resolve_unite_payload_extract(object, output)
        else {
            return Ok(false);
        };
        // Always prefer the resolved unite sum. Stale MIR `layout_id` often points at an
        // unrelated aggregate that also has `payload` (e.g. `AlgebraicTerm`) — using that
        // owner yields `castclass AlgebraicTerm` on a live `Result` → InvalidCastException.
        let sum_qualified = self.ctx.clr_qualified_type_name(&sum_name);
        let owner = if storage == StorageKind::Value {
            match layout_id.and_then(|lid| self.ctx.layout_by_id(lid)) {
                Some(layout)
                    if layout.fields.iter().any(|f| f.name == "tag")
                        && layout.fields.iter().any(|f| f.name == "payload")
                        && (layout.name == sum_name || Self::type_name_matches(&layout.name, &sum_name)) =>
                {
                    self.ctx.clr_qualified_type_name(&layout.name)
                }
                _ => sum_qualified,
            }
        }
        else {
            sum_qualified
        };
        let tag_ok = self.alloc_temp_local(MsilType::Bool);
        let miss = format!("unite_payload_miss_{}", output.0);
        let done = format!("unite_payload_done_{}", output.0);

        if storage == StorageKind::Value {
            let Some(object_local) = self.operand_local(object)
            else {
                return Err(miette!(
                    code = "nyar::clr::unite_payload_unmaterialized_object",
                    help = "unite payload 抽取的 object 无本地槽（自由变量/未绑定 SSA）。",
                    "CLR 拒绝降低函数 `{}` 中无法物化的 unite payload FieldGet",
                    self.operation
                ));
            };
            self.emit_ldloca(object_local);
            // Value-type unite: ldloca → ldfld tag needs address; dup address for payload path.
            self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Dup, operand: None });
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Ldfld,
                operand: Some(MsilInstructionOperand::Field(owner.clone(), "tag".to_string())),
            });
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::LdcI4,
                operand: Some(MsilInstructionOperand::Integer(expected_tag as i64)),
            });
            self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ceq, operand: None });
            self.emit_stloc(tag_ok);
            self.emit_ldloc(tag_ok);
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Brfalse,
                operand: Some(MsilInstructionOperand::BranchTarget(miss.clone())),
            });
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Ldfld,
                operand: Some(MsilInstructionOperand::Field(owner.clone(), "payload".to_string())),
            });
            self.store_unite_payload_on_hit(&out_msil, output);
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Br,
                operand: Some(MsilInstructionOperand::BranchTarget(done.clone())),
            });
            self.instructions.push(MsilInstruction { label: Some(miss), opcode: MsilOpcode::Pop, operand: None });
            self.emit_unite_payload_miss_default(&out_msil, output);
            self.instructions.push(MsilInstruction { label: Some(done), opcode: MsilOpcode::Nop, operand: None });
        }
        else {
            self.emit_operand(object);
            if matches!(self.msil_type_of_operand(object), MsilType::Object) && !owner.starts_with('[') {
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Castclass,
                    operand: Some(MsilInstructionOperand::Type(owner.clone())),
                });
            }
            self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Dup, operand: None });
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Ldfld,
                operand: Some(MsilInstructionOperand::Field(owner.clone(), "tag".to_string())),
            });
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::LdcI4,
                operand: Some(MsilInstructionOperand::Integer(expected_tag as i64)),
            });
            self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ceq, operand: None });
            self.emit_stloc(tag_ok);
            self.emit_ldloc(tag_ok);
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Brfalse,
                operand: Some(MsilInstructionOperand::BranchTarget(miss.clone())),
            });
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Ldfld,
                operand: Some(MsilInstructionOperand::Field(owner, "payload".to_string())),
            });
            self.store_unite_payload_on_hit(&out_msil, output);
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::Br,
                operand: Some(MsilInstructionOperand::BranchTarget(done.clone())),
            });
            self.instructions.push(MsilInstruction { label: Some(miss), opcode: MsilOpcode::Pop, operand: None });
            self.emit_unite_payload_miss_default(&out_msil, output);
            self.instructions.push(MsilInstruction { label: Some(done), opcode: MsilOpcode::Nop, operand: None });
        }
        self.unite_payload_tag_ok.insert(output, tag_ok);
        Ok(true)
    }

    /// After `ldfld payload`: narrow+store, or discard Unit/void payload (no stack leftover).
    fn store_unite_payload_on_hit(&mut self, out_msil: &MsilType, output: MirValueRef) {
        if matches!(out_msil, MsilType::Void) {
            // Nullary / Unit binding: payload is still `object` on the stack — discard it.
            // Never leave the stack unbalanced (`store_call_result(Void)` is a no-op).
            self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Pop, operand: None });
            if let Some(&local) = self.slots.value_locals.get(&output) {
                if let Some(slot) = self.slots.local_types.get_mut(local as usize) {
                    *slot = MsilType::Int32 { signed: true };
                }
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None });
                self.emit_stloc(local);
            }
            return;
        }
        self.emit_narrow_object_to_msil(out_msil);
        self.store_call_result(output, out_msil);
    }

    fn emit_unite_payload_miss_default(&mut self, out_msil: &MsilType, output: MirValueRef) {
        let Some(&local) = self.slots.value_locals.get(&output)
        else {
            return;
        };
        // Unit→Void must not enter LocalVarSig (ECMA-335 forbids ELEMENT_TYPE_VOID there).
        let out_msil = sanitize_clr_local_type(out_msil.clone());
        if let Some(slot) = self.slots.local_types.get_mut(local as usize) {
            *slot = out_msil.clone();
        }
        match &out_msil {
            MsilType::Named(name) if self.submission.aggregate_layouts.value_type_names.contains(name) => {
                let qualified = self.ctx.clr_qualified_type_name(name);
                self.emit_ldloca(local);
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Initobj,
                    operand: Some(MsilInstructionOperand::Type(qualified)),
                });
            }
            MsilType::Bool
            | MsilType::Char
            | MsilType::Int8 { .. }
            | MsilType::Int16 { .. }
            | MsilType::Int32 { .. }
            | MsilType::Int64 { .. }
            | MsilType::IntPtr { .. } => {
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None });
                self.emit_stloc(local);
            }
            MsilType::Float32 | MsilType::Float64 => {
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::LdcR8,
                    operand: Some(MsilInstructionOperand::Float("0".to_string())),
                });
                self.emit_stloc(local);
            }
            _ => {
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ldnull, operand: None });
                self.emit_stloc(local);
            }
        }
    }

    /// Resolve erased Fine/Fail/Option unite when the scrutinee local is typed `object`.
    ///
    /// `VonParseResult<T>` lowers as bare `Result`; `pick_operation_local_sum` alone fails for
    /// ops like `parse_von_tokens` (name tokens never appear in `Result`), and fuzzy `payload`
    /// FieldGet then picks `AlgebraicTerm` (valuetype with a `payload` field) → InvalidCast.
    fn resolve_erased_unite_sum_name(&self, out_ty: Option<&NyarType>) -> Option<String> {
        let candidates: Vec<&nyar_types::layout::SumTypeLayout> = self
            .submission
            .sum_types
            .iter()
            .filter(|sum| sum.is_unite && sum.variants.iter().any(|variant| matches!(variant.name.as_str(), "Fine" | "Fail" | "Some" | "None")))
            .collect();
        if candidates.is_empty() {
            return None;
        }

        if let Some(out_ty) = out_ty {
            let mut by_payload = Vec::new();
            for sum in &candidates {
                for variant in &sum.variants {
                    let Some(payload) = variant.payload_type.as_ref()
                    else {
                        continue;
                    };
                    if Self::nyar_payload_type_matches(payload, out_ty) {
                        by_payload.push(sum.name.clone());
                        break;
                    }
                }
            }
            by_payload.sort();
            by_payload.dedup();
            if by_payload.len() == 1 {
                return Some(by_payload.remove(0));
            }
            if by_payload.len() > 1 {
                let tuples: Vec<(String, u32, bool)> = by_payload.iter().map(|name| (name.clone(), 0, true)).collect();
                return None;
            }
        }

        let tuples: Vec<(String, u32, bool)> = candidates.iter().map(|sum| (sum.name.clone(), 0, true)).collect();

        let fine_fail: Vec<String> = candidates
            .iter()
            .filter(|sum| sum.variants.iter().any(|variant| matches!(variant.name.as_str(), "Fine" | "Fail")))
            .map(|sum| sum.name.clone())
            .collect();
        if fine_fail.len() == 1 {
            return Some(fine_fail[0].clone());
        }

        let option_like: Vec<String> = candidates
            .iter()
            .filter(|sum| {
                sum.variants.iter().any(|variant| matches!(variant.name.as_str(), "Some" | "None"))
                    && !sum.variants.iter().any(|variant| matches!(variant.name.as_str(), "Fine" | "Fail"))
            })
            .map(|sum| sum.name.clone())
            .collect();
        if let Some(name) = option_like.iter().find(|name| Self::type_name_matches(name, "Option")) {
            return Some(name.clone());
        }
        if option_like.len() == 1 {
            return Some(option_like[0].clone());
        }

        None
    }

    /// Map Fine/Fail extractor FieldGet to `(sum_name, expected_tag, payload_msil)`.
    fn resolve_unite_payload_extract(&self, object: &MirOperand, output: MirValueRef) -> Option<(String, u32, MsilType)> {
        let out_ty = self.lookup_value_type(&output);
        let out_msil = out_ty
            .map(|ty| nyar_type_to_msil(ty, &self.submission.aggregate_layouts))
            .or_else(|| self.slots.value_locals.get(&output).and_then(|&local| self.slots.local_types.get(local as usize).cloned()))?;
        let sum_name = self.sum_type_name_for_operand(object).or_else(|| {
            if !matches!(self.msil_type_of_operand(object), MsilType::Object | MsilType::Named(_)) {
                return None;
            }
            self.resolve_erased_unite_sum_name(out_ty)
        })?;
        let sum = self.submission.sum_types.iter().find(|sum| sum.name == sum_name || Self::type_name_matches(&sum.name, &sum_name))?;
        if !sum.is_unite {
            return None;
        }
        // Exact payload first (Fail/VonDiagnostic). Fine/Some carry erased `T`
        // (`TraitObject(__generic)` via concretize_type_lossy) — fall back only after
        // no concrete arm matches, else Fail extract would steal Fine's tag.
        let mut open_fine_tag = None;
        let mut open_fail_tag = None;
        for variant in &sum.variants {
            // Record Fine/Some tag even when payload_type is missing (nullary / erased).
            if open_fine_tag.is_none() && matches!(variant.name.as_str(), "Fine" | "Some") {
                open_fine_tag = Some(variant.tag);
            }
            if open_fail_tag.is_none() && matches!(variant.name.as_str(), "Fail" | "None") {
                open_fail_tag = Some(variant.tag);
            }
            let Some(payload) = variant.payload_type.as_ref()
            else {
                continue;
            };
            if let Some(out_ty) = out_ty {
                if Self::nyar_payload_type_matches(payload, out_ty) {
                    return Some((sum.name.clone(), variant.tag, out_msil));
                }
            }
            // Also match via MSIL shape when MIR still has an out type but payloads were
            // erased / namespace-skewed — otherwise Fail(VonDiagnostic) falls through to Fine's
            // open tag and `unbox.any VonDiagnostic` ICEs on a Fine payload.
            let payload_msil = nyar_type_to_msil(payload, &self.submission.aggregate_layouts);
            if Self::msil_payload_type_matches(&payload_msil, &out_msil) {
                return Some((sum.name.clone(), variant.tag, out_msil));
            }
        }
        // Error-like extractors (`VonDiagnostic`, `*Error`) prefer Fail/None when no arm
        // matched by type — never reuse Fine's tag (parse_von_tokens Fail path).
        if Self::out_msil_looks_like_fail_payload(&out_msil) {
            if let Some(tag) = open_fail_tag {
                return Some((sum.name.clone(), tag, out_msil));
            }
        }
        open_fine_tag.map(|tag| (sum.name.clone(), tag, out_msil))
    }

    fn out_msil_looks_like_fail_payload(out_msil: &MsilType) -> bool {
        match out_msil {
            MsilType::Named(name) => {
                let lower = name.to_ascii_lowercase();
                lower.contains("diagnostic") || lower.contains("error") || lower.ends_with("err")
            }
            _ => false,
        }
    }

    fn msil_payload_type_matches(payload: &MsilType, expected: &MsilType) -> bool {
        match (payload, expected) {
            (MsilType::Named(a), MsilType::Named(b)) => Self::type_name_matches(a, b),
            (MsilType::SzArray(a), MsilType::SzArray(b)) => Self::msil_payload_type_matches(a, b),
            (MsilType::String, MsilType::String) | (MsilType::Object, MsilType::Object) | (MsilType::Bool, MsilType::Bool) => true,
            (MsilType::Int32 { .. }, MsilType::Int32 { .. })
            | (MsilType::Int64 { .. }, MsilType::Int64 { .. })
            | (MsilType::Int8 { .. }, MsilType::Int8 { .. })
            | (MsilType::Int16 { .. }, MsilType::Int16 { .. }) => true,
            _ => payload == expected,
        }
    }

    fn nyar_payload_type_matches(payload: &NyarType, expected: &NyarType) -> bool {
        match (payload, expected) {
            (NyarType::Named(a), NyarType::Named(b)) => Self::type_name_matches(a.as_str(), b.as_str()),
            (NyarType::Apply(a, _), NyarType::Apply(b, _)) => match (a.as_ref(), b.as_ref()) {
                (NyarType::Named(a), NyarType::Named(b)) => Self::type_name_matches(a.as_str(), b.as_str()),
                _ => false,
            },
            (NyarType::Named(a), NyarType::Apply(b, _)) | (NyarType::Apply(b, _), NyarType::Named(a)) => match b.as_ref() {
                NyarType::Named(b) => Self::type_name_matches(a.as_str(), b.as_str()),
                _ => false,
            },
            (NyarType::Utf8, NyarType::Utf8) | (NyarType::Utf16, NyarType::Utf16) => true,
            _ => payload == expected,
        }
    }

    /// Element type token for `newarr` / `castclass T[]` — keep local aggregate names.
    fn clr_array_element_type_token(&self, element: &MsilType) -> String {
        match element {
            MsilType::Named(name) if !name.starts_with('[') => {
                let plan = &self.submission.aggregate_layouts;
                if plan.value_type_names.contains(name) || plan.type_name_to_layout.contains_key(name) {
                    self.ctx.clr_qualified_type_name(name)
                }
                else {
                    // Unresolved bare Named must not invent a PE TypeRef.
                    "object".to_string()
                }
            }
            other => clr_array_element_type_name(other),
        }
    }

    /// `ldelem.*` / `ldelem.any` chosen from element type evidence (valuetype → `.any`).
    fn emit_ldelem_for_element(&mut self, element: &MsilType) {
        match element {
            MsilType::Int32 { .. } | MsilType::Bool => {
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdelemI4, operand: None });
            }
            MsilType::Int8 { signed: false } => {
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdelemU1, operand: None });
            }
            MsilType::Named(name) if self.submission.aggregate_layouts.value_type_names.contains(name) => {
                let type_operand = self.ctx.clr_qualified_type_name(name);
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::LdelemAny,
                    operand: Some(MsilInstructionOperand::Type(type_operand)),
                });
            }
            MsilType::Char
            | MsilType::Int8 { .. }
            | MsilType::Int16 { .. }
            | MsilType::Int64 { .. }
            | MsilType::Float32
            | MsilType::Float64
            | MsilType::IntPtr { .. } => {
                let type_operand = self.clr_array_element_type_token(element);
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::LdelemAny,
                    operand: Some(MsilInstructionOperand::Type(type_operand)),
                });
            }
            _ => {
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdelemRef, operand: None });
            }
        }
    }

    /// `stelem.*` / `stelem.any` chosen from element type evidence (valuetype → `.any`).
    fn emit_stelem_for_element(&mut self, element: &MsilType) {
        match element {
            MsilType::Int32 { .. } | MsilType::Bool => {
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::StelemI4, operand: None });
            }
            MsilType::Int8 { .. } => {
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::StelemI1, operand: None });
            }
            MsilType::Named(name) if self.submission.aggregate_layouts.value_type_names.contains(name) => {
                let type_operand = self.ctx.clr_qualified_type_name(name);
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::StelemAny,
                    operand: Some(MsilInstructionOperand::Type(type_operand)),
                });
            }
            MsilType::Char
            | MsilType::Int16 { .. }
            | MsilType::Int64 { .. }
            | MsilType::Float32
            | MsilType::Float64
            | MsilType::IntPtr { .. } => {
                let type_operand = self.clr_array_element_type_token(element);
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::StelemAny,
                    operand: Some(MsilInstructionOperand::Type(type_operand)),
                });
            }
            _ => {
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::StelemRef, operand: None });
            }
        }
    }

    /// Narrow a sum `payload` (`object`) to the concrete Fine/Fail binding type.
    ///
    /// Value types → `unbox.any`; reference nominals / string / arrays → `castclass`.
    fn emit_narrow_object_to_msil(&mut self, target: &MsilType) {
        match target {
            MsilType::Object | MsilType::Void => {}
            MsilType::String => {
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Castclass,
                    operand: Some(MsilInstructionOperand::Type("[mscorlib]System.String".to_string())),
                });
            }
            MsilType::SzArray(inner) => {
                let element = self.clr_array_element_type_token(inner);
                self.instructions.push(MsilInstruction {
                    label: None,
                    opcode: MsilOpcode::Castclass,
                    operand: Some(MsilInstructionOperand::Type(format!("{element}[]"))),
                });
            }
            other => {
                if let Some(type_operand) = self.box_type_operand_for_payload(other) {
                    let is_valuetype = match other {
                        MsilType::Named(name) => self.submission.aggregate_layouts.value_type_names.contains(name),
                        MsilType::Bool
                        | MsilType::Char
                        | MsilType::Int8 { .. }
                        | MsilType::Int16 { .. }
                        | MsilType::Int32 { .. }
                        | MsilType::Int64 { .. }
                        | MsilType::Float32
                        | MsilType::Float64 => true,
                        _ => false,
                    };
                    self.instructions.push(MsilInstruction {
                        label: None,
                        opcode: if is_valuetype { MsilOpcode::UnboxAny } else { MsilOpcode::Castclass },
                        operand: Some(MsilInstructionOperand::Type(type_operand)),
                    });
                }
            }
        }
    }

    /// 推断操作数在 CLR 上的 MSIL 类型。
    fn msil_type_of_operand(&self, operand: &MirOperand) -> MsilType {
        match operand {
            MirOperand::Constant(MirConstant::Utf8(_) | MirConstant::Utf16(_)) => MsilType::String,
            MirOperand::Constant(MirConstant::Bool(_)) => MsilType::Bool,
            MirOperand::Constant(MirConstant::Int(_)) => MsilType::Int32 { signed: true },
            MirOperand::Constant(MirConstant::Float64(_)) => MsilType::Float64,
            MirOperand::Constant(MirConstant::Unit) => MsilType::Object,
            MirOperand::Value(value) => {
                // Prefer the refined CLR local slot type over MIR `value_types`.
                // Loop-carried temps are sometimes MIR-typed as `utf8` while the
                // backend emits integer `add`/`ldlen`; using MIR here re-poisons
                // destinations via Copy/block-arg retype (PEVerify Int32↔String).
                if let Some(&local) = self.slots.value_locals.get(value) {
                    if let Some(ty) = self.slots.local_types.get(local as usize) {
                        return ty.clone();
                    }
                }
                self.lookup_value_type_from_operand(operand)
                    .map(|ty| nyar_type_to_msil(&ty, &self.submission.aggregate_layouts))
                    .unwrap_or(MsilType::Object)
            }
            MirOperand::Symbol(_) => MsilType::Object,
        }
    }

    fn receiver_type_for_witness(&self, witness: Option<&MirOperand>, first_argument: Option<&MirOperand>) -> Option<NyarType> {
        witness.or(first_argument).and_then(|operand| match operand {
            MirOperand::Value(value) => self.lookup_value_type(value).cloned(),
            _ => None,
        })
    }

    /// 查找 SSA 值的类型，优先查 `value_types`，回退到入口块参数对应的 `param_types`。
    ///
    /// 入口块参数（如 singleton 方法中的 `self`）可能不在 `value_types` 中，
    /// 但其类型可以从 `param_types` 按参数索引取到。
    fn lookup_value_type(&self, value: &MirValueRef) -> Option<&NyarType> {
        if let Some(ty) = self.mir_fn.value_types.get(value) {
            return Some(ty);
        }
        let entry_block = self.mir_fn.blocks.get(self.mir_fn.entry.0 as usize)?;
        for (index, param) in entry_block.parameters.iter().enumerate() {
            if param == value {
                return self.mir_fn.param_types.get(index);
            }
        }
        None
    }

    fn resolve_external_import_link(&self, path: &NamePath) -> Option<&ExternalImportLink> {
        if path.parts().len() > 1 {
            let qualified = QualifiedName::new(path.parts().to_vec());
            if let Some(link) = self.submission.external_import_links.get(&qualified) {
                if clr_host_method_target(link).is_some() {
                    return Some(link);
                }
            }
        }
        let simple = path.parts().last()?;
        let mut matches = self
            .submission
            .external_import_links
            .iter()
            .filter(|(name, link)| name.parts().last() == Some(simple) && clr_host_method_target(link).is_some())
            .collect::<Vec<_>>();
        match matches.len() {
            0 => None,
            1 => Some(matches[0].1),
            _ => {
                matches.sort_by_key(|(name, _)| std::cmp::Reverse(qualified_name_common_prefix_len(name, &self.operation)));
                Some(matches[0].1)
            }
        }
    }

    fn resolve_witness_method_symbol(&self, method_name: &str, witness: Option<&MirOperand>, arguments: &[MirOperand]) -> Option<String> {
        let receiver_ty = self.receiver_type_for_witness(witness, arguments.first())?;

        // Primary path: look up witness table by receiver type name, then prefer
        // the real Valkyrie-implemented method symbol over the synthetic stub.
        // Trait dispatch must forward to the actual user code (e.g.
        // `ArrayIterator.next` lowered from `imply` blocks) instead of a Rust-side
        // mock. The MIR function symbol convention is `{type}.{method}` (see
        // `lower_impl_method_functions`), which `sanitize_operation_symbol` turns
        // into a valid MSIL MethodDef name.
        if let Some(type_name) = receiver_type_name(&receiver_ty) {
            if let Some(table) = self.submission.witness_tables.iter().find(|table| table.type_name == type_name) {
                if let Some(method) = table.methods.iter().find(|method| method.method_name == method_name) {
                    let real_operation = QualifiedName::new(vec![Identifier::new(&table.type_name), Identifier::new(&method.method_name)]);
                    // Witness metadata names the canonical MIR operation.  Derive
                    // the CLR spelling from that operation and never forward a
                    // synthetic or unsanitized witness string.
                    return Some(sanitize_operation_symbol(&real_operation));
                }
            }
        }

        // Fallback: resolve by trait name when the receiver type has no direct
        // witness table match (e.g. trait-object / erased receiver).
        witness_trait_name(&self.submission.witness_tables, &receiver_ty)
            .and_then(|trait_name| self.ctx.witness_impl_symbol(&trait_name, method_name))
            .map(|symbol| sanitize_symbol(&symbol))
    }

    /// Resolve witness Call signature from the registered table slot (not bare method name).
    fn resolve_witness_call_signature(
        &self,
        method_name: &str,
        witness: Option<&MirOperand>,
        arguments: &[MirOperand],
    ) -> Option<(MsilType, Vec<MsilType>)> {
        let receiver_ty = self.receiver_type_for_witness(witness, arguments.first())?;
        let receiver_name = receiver_type_name(&receiver_ty)?;
        let table = self.submission.witness_tables.iter().find(|table| table.type_name == receiver_name)?;
        let method = table.methods.iter().find(|method| method.method_name == method_name)?;
        Some(witness_slot_msil_signature(table, method))
    }

    fn resolve_static_call_symbol(&self, path: &nyar::NamePath) -> String {
        if let Some(operation) = self.resolve_operation_for_static_path(path) {
            return sanitize_operation_symbol(&operation);
        }
        sanitize_symbol(&path.to_string())
    }

    /// Resolve a Call path to the executable operation it names (if any).
    fn resolve_operation_for_static_path(&self, path: &nyar::NamePath) -> Option<QualifiedName> {
        if path.parts().len() > 1 {
            let qualified = QualifiedName::new(path.parts().to_vec());
            if self.submission.executable.as_ref().and_then(|exec| exec.get_function(&qualified)).is_some() {
                return Some(qualified);
            }
        }
        let simple = path.parts().last().map(|part| part.as_str())?;
        let mut candidates = Vec::new();
        for operation in &self.submission.exported_operations {
            if operation.parts().last().map(|part| part.as_str()) == Some(simple) {
                candidates.push(operation.clone());
            }
        }
        if let Some(exec) = &self.submission.executable {
            for operation in exec.operations() {
                if operation.parts().last().map(|part| part.as_str()) == Some(simple) {
                    if !candidates.iter().any(|candidate| candidate == &operation) {
                        candidates.push(operation.clone());
                    }
                }
            }
        }
        for (name, link) in &self.submission.external_import_links {
            if name.parts().last().map(|part| part.as_str()) == Some(simple) && clr_host_method_target(link).is_some() {
                if !candidates.iter().any(|candidate| candidate == name) {
                    candidates.push(name.clone());
                }
            }
        }
        select_best_qualified_name_candidate(&candidates, &self.operation)
    }

    /// Prefer call-site parameter types attached by HIR→SSA when present.
    ///
    /// Return type still comes from the SSA output `value_types` map (or Void).
    fn signature_from_attached_parameter_types(
        &self,
        instruction: &MirInstruction,
        parameter_types: Option<&Vec<NyarType>>,
        argument_count: usize,
    ) -> Option<(MsilType, Vec<MsilType>)> {
        let params = parameter_types?;
        // Empty attached list with non-empty args is incomplete — fall through.
        if params.is_empty() && argument_count > 0 {
            return None;
        }
        let return_type = instruction
            .output
            .and_then(|output| self.lookup_value_type(&output).map(|ty| nyar_type_to_msil(ty, &self.submission.aggregate_layouts)))
            .unwrap_or(MsilType::Void);
        let params = params.iter().map(|ty| nyar_type_to_msil(ty, &self.submission.aggregate_layouts)).collect();
        Some((return_type, params))
    }

    /// Resolve a local call signature from the canonical, fully-qualified
    /// Semantic MIR callee symbol.  A backend must not choose a declaration
    /// from a short method name or namespace suffix.
    fn resolve_call_signature(&self, path: &nyar::NamePath) -> Option<(MsilType, Vec<MsilType>)> {
        let exec = self.submission.executable.as_ref()?;
        let view = exec.find_by_symbol(&path.to_string())?;
        let mir_fn = &view.function;
        let return_type = nyar_type_to_msil(&mir_fn.return_type, &self.submission.aggregate_layouts);
        let param_types = mir_fn.param_types.iter().map(|ty| nyar_type_to_msil(ty, &self.submission.aggregate_layouts)).collect();
        Some((return_type, param_types))
    }

    /// 当 `resolve_call_signature` 无法找到 MIR 函数定义时（如 `tuple_get_N` 这类
    /// 符号化辅助调用），从指令的输出类型与参数操作数类型推断 MSIL 调用签名。
    ///
    /// 这确保值类型字段提取（extractor payload slot 解构）的 `Call` 指令返回正确
    /// 的 MSIL 类型，而不是退回到 `witness_call_signature` 的默认 `Int32` 签名——
    /// 后者会导致值类型字段按值拷贝到 binding 时出现类型/布局错位。
    fn infer_call_signature_from_types(&self, instruction: &MirInstruction, arguments: &[MirOperand]) -> Option<(MsilType, Vec<MsilType>)> {
        let msil_return = instruction
            .output
            .and_then(|output| self.lookup_value_type(&output).map(|ty| nyar_type_to_msil(ty, &self.submission.aggregate_layouts)))
            .unwrap_or(MsilType::Void);
        // Require at least one typed argument or a typed return — otherwise this
        // would invent Object(Object…) and hide missing MIR MethodDefs.
        let params: Vec<MsilType> = arguments
            .iter()
            .map(|arg| match arg {
                MirOperand::Value(v) => self.lookup_value_type(v).map(|ty| nyar_type_to_msil(ty, &self.submission.aggregate_layouts)),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()?;
        if params.is_empty() && matches!(msil_return, MsilType::Void) {
            return None;
        }
        // Reject all-Object guesses (same policy as unknown_call_signature).
        if params.iter().all(|ty| matches!(ty, MsilType::Object)) && matches!(msil_return, MsilType::Object | MsilType::Void) {
            return None;
        }
        Some((msil_return, params))
    }

    fn store_to_value(&mut self, value: MirValueRef) {
        if let Some(local) = self.slots.value_locals.get(&value).copied() {
            self.emit_stloc(local);
        }
    }

    /// Retype a local slot from the concrete MSIL type of `operand` (evidence on the stack).
    fn retype_local_from_operand(&mut self, local: u16, operand: &MirOperand) {
        let ty = self.msil_type_of_operand(operand);
        if matches!(ty, MsilType::Void) {
            return;
        }
        if let Some(slot) = self.slots.local_types.get_mut(local as usize) {
            *slot = sanitize_clr_local_type(ty);
        }
    }

    /// `stloc` of `output`, retyping the slot from `source`'s MSIL type first.
    fn store_typed_from_operand(&mut self, output: MirValueRef, source: &MirOperand) {
        if let Some(&local) = self.slots.value_locals.get(&output) {
            self.retype_local_from_operand(local, source);
        }
        self.store_to_value(output);
    }

    /// Store a Call/Callvirt result, retyping the SSA local to the callee return type.
    ///
    /// Slot planning maps missing MIR `value_types` to `Unit`→`Void`→`int32`. When the real
    /// callee returns `string`/`object`/…, that mismatch becomes PEVerify
    /// `found Int32, expected String` and runtime `InvalidProgramException`.
    fn emit_clr_host_method_call(
        &mut self,
        target: &ClrHostMethodTarget<'_>,
        param_types: Vec<MsilType>,
        desired_return: &MsilType,
        output: Option<MirValueRef>,
    ) {
        let (bcl_return, adapt) = clr_host_bcl_return(target, desired_return);
        let param_types = clr_host_bcl_param_types(target, param_types);
        self.instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Call,
            operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                owner: Some(format!("[{}]{}", target.assembly, target.owner)),
                name: target.method.to_string(),
                signature: MsilMethodSignature::new(bcl_return, param_types),
            })),
        });
        match adapt {
            ClrHostReturnAdapt::Identity => {}
            ClrHostReturnAdapt::DiscardRef { push_true } => {
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Pop, operand: None });
                // After discarding a ref return, restore a value if the V contract expects one.
                match desired_return {
                    MsilType::Bool if push_true => {
                        self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_1, operand: None });
                    }
                    MsilType::Bool => {
                        self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None });
                    }
                    MsilType::Int32 { .. } => {
                        self.instructions.push(MsilInstruction {
                            label: None,
                            opcode: if push_true { MsilOpcode::LdcI4_1 } else { MsilOpcode::LdcI4_0 },
                            operand: None,
                        });
                    }
                    _ => {}
                }
            }
            ClrHostReturnAdapt::PushTrue => {
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_1, operand: None });
            }
        }
        if let Some(output) = output {
            self.store_call_result(output, desired_return);
        }
    }

    fn store_call_result(&mut self, output: MirValueRef, return_type: &MsilType) {
        // True void calls omit this helper via `returns_value`. Unit-typed *payloads* that still
        // leave an object on the stack are handled by the unite FieldGet path (pop + int32 0),
        // not here — a blanket `pop` would underflow after real void calls.
        if matches!(return_type, MsilType::Void) {
            return;
        }
        // Prefer concrete MIR type when the MethodDef signature erased the return to `object`
        // (e.g. `parse_von_value` → `VonParseResult`), so later FieldGet/castclass stay on the
        // right unite class instead of first-hit `PackResult`.
        let effective = if matches!(return_type, MsilType::Object) {
            self.lookup_value_type(&output)
                .map(|ty| nyar_type_to_msil(ty, &self.submission.aggregate_layouts))
                .filter(|ty| !matches!(ty, MsilType::Void | MsilType::Object))
                .unwrap_or_else(|| return_type.clone())
        }
        else {
            return_type.clone()
        };
        let effective = sanitize_clr_local_type(effective);
        if let Some(&local) = self.slots.value_locals.get(&output) {
            if let Some(slot) = self.slots.local_types.get_mut(local as usize) {
                *slot = effective;
            }
        }
        self.store_to_value(output);
    }

    fn emit_ldloc(&mut self, local: u16) {
        let opcode = match local {
            0 => MsilOpcode::Ldloc0,
            1 => MsilOpcode::Ldloc1,
            2 => MsilOpcode::Ldloc2,
            3 => MsilOpcode::Ldloc3,
            _ => MsilOpcode::Ldloc,
        };
        self.instructions.push(MsilInstruction {
            label: None,
            opcode,
            operand: if local > 3 { Some(MsilInstructionOperand::Integer(local as i64)) } else { None },
        });
    }

    fn emit_ldarg(&mut self, index: u16) {
        let opcode = match index {
            0 => MsilOpcode::Ldarg0,
            1 => MsilOpcode::Ldarg1,
            2 => MsilOpcode::Ldarg2,
            3 => MsilOpcode::Ldarg3,
            _ => MsilOpcode::Ldarg,
        };
        self.instructions.push(MsilInstruction {
            label: None,
            opcode,
            operand: if index > 3 { Some(MsilInstructionOperand::Integer(index as i64)) } else { None },
        });
    }

    fn emit_ldloca(&mut self, local: u16) {
        self.instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Ldloca,
            operand: Some(MsilInstructionOperand::Integer(local as i64)),
        });
    }

    fn emit_stloc(&mut self, local: u16) {
        let opcode = match local {
            0 => MsilOpcode::Stloc0,
            1 => MsilOpcode::Stloc1,
            2 => MsilOpcode::Stloc2,
            3 => MsilOpcode::Stloc3,
            _ => MsilOpcode::Stloc,
        };
        self.instructions.push(MsilInstruction {
            label: None,
            opcode,
            operand: if local > 3 { Some(MsilInstructionOperand::Integer(local as i64)) } else { None },
        });
    }

    fn alloc_temp_local(&mut self, ty: MsilType) -> u16 {
        let index = self.slots.local_types.len() as u16;
        self.slots.local_types.push(sanitize_clr_local_type(ty));
        index
    }

    /// `push([T], T) -> [T]`: grow via `newarr` + element copy loop + `stelem`.
    ///
    /// Requires a known `SzArray` receiver (fail-closed otherwise — no invented element type).
    /// Uses an explicit copy loop instead of `System.Array::Copy` so PE emission does not
    /// depend on `Named("[mscorlib]System.Array")` token lowering (which was collapsing to
    /// `object` and yielding `MissingMethodException` at runtime).
    fn emit_intrinsic_array_push(&mut self, arguments: &[MirOperand], output: Option<MirValueRef>) {
        if arguments.len() < 2 {
            return;
        }
        let array = &arguments[0];
        let value = &arguments[1];
        let MsilType::SzArray(element) = self.msil_type_of_operand(array)
        else {
            return;
        };
        let element = *element;
        let array_ty = MsilType::SzArray(Box::new(element.clone()));
        let element_name = self.clr_array_element_type_token(&element);
        let len_local = self.alloc_temp_local(MsilType::Int32 { signed: true });
        let new_local = self.alloc_temp_local(array_ty.clone());
        let idx_local = self.alloc_temp_local(MsilType::Int32 { signed: true });
        let copy_loop = format!("array_push_copy_{}", new_local);
        let copy_cond = format!("array_push_cond_{}", new_local);
        let copy_done = format!("array_push_done_{}", new_local);

        self.emit_operand(array);
        self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ldlen, operand: None });
        self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::ConvI4, operand: None });
        self.emit_stloc(len_local);

        self.emit_ldloc(len_local);
        self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_1, operand: None });
        self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Add, operand: None });
        self.instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Newarr,
            operand: Some(MsilInstructionOperand::Type(element_name)),
        });
        self.emit_stloc(new_local);

        self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None });
        self.emit_stloc(idx_local);
        self.instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Br,
            operand: Some(MsilInstructionOperand::BranchTarget(copy_cond.clone())),
        });

        self.instructions.push(MsilInstruction { label: Some(copy_loop.clone()), opcode: MsilOpcode::Nop, operand: None });
        self.emit_ldloc(new_local);
        self.emit_ldloc(idx_local);
        self.emit_operand(array);
        self.emit_ldloc(idx_local);
        self.emit_ldelem_for_element(&element);
        self.emit_stelem_for_element(&element);
        self.emit_ldloc(idx_local);
        self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_1, operand: None });
        self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Add, operand: None });
        self.emit_stloc(idx_local);

        self.instructions.push(MsilInstruction { label: Some(copy_cond.clone()), opcode: MsilOpcode::Nop, operand: None });
        self.emit_ldloc(idx_local);
        self.emit_ldloc(len_local);
        self.instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Blt,
            operand: Some(MsilInstructionOperand::BranchTarget(copy_loop)),
        });
        self.instructions.push(MsilInstruction { label: Some(copy_done), opcode: MsilOpcode::Nop, operand: None });

        self.emit_ldloc(new_local);
        self.emit_ldloc(len_local);
        self.emit_operand(value);
        self.emit_stelem_for_element(&element);

        // Functional `push` returns a new array; Valkyrie sources commonly write
        // `push(tokens, item)` as a statement (see Array.v / lex_von). Write the
        // result back into the receiver local so mut arrays actually grow.
        self.emit_ldloc(new_local);
        if let Some(array_local) = self.operand_local(array) {
            self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Dup, operand: None });
            self.emit_stloc(array_local);
            if let Some(slot) = self.slots.local_types.get_mut(array_local as usize) {
                *slot = array_ty.clone();
            }
        }
        if let Some(output) = output {
            if let Some(&local) = self.slots.value_locals.get(&output) {
                if let Some(slot) = self.slots.local_types.get_mut(local as usize) {
                    *slot = array_ty;
                }
            }
            self.store_to_value(output);
        }
        else {
            self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Pop, operand: None });
        }
    }

    /// Emit `newarr` + element stores for a heap array literal / reference fixed-array.
    fn emit_clr_array_literal(&mut self, output: Option<MirValueRef>, element_type: &NyarType, items: &[MirOperand]) {
        let Some(output) = output
        else {
            return;
        };
        let Some(&local) = self.slots.value_locals.get(&output)
        else {
            return;
        };
        let element_msil = nyar_type_to_msil(element_type, &self.submission.aggregate_layouts);
        let element_name = self.clr_array_element_type_token(&element_msil);
        self.instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::LdcI4,
            operand: Some(MsilInstructionOperand::Integer(items.len() as i64)),
        });
        self.instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Newarr,
            operand: Some(MsilInstructionOperand::Type(element_name)),
        });
        for (index, item) in items.iter().enumerate() {
            self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Dup, operand: None });
            self.instructions.push(MsilInstruction {
                label: None,
                opcode: MsilOpcode::LdcI4,
                operand: Some(MsilInstructionOperand::Integer(index as i64)),
            });
            self.emit_operand(item);
            self.emit_stelem_for_element(&element_msil);
        }
        self.emit_stloc(local);
    }

    /// Emit `<length>` + `newarr` for a heap array allocation.
    fn emit_clr_array_new(&mut self, output: Option<MirValueRef>, element_type: &NyarType, length: &MirOperand) {
        let Some(output) = output
        else {
            return;
        };
        let Some(&local) = self.slots.value_locals.get(&output)
        else {
            return;
        };
        let element_msil = nyar_type_to_msil(element_type, &self.submission.aggregate_layouts);
        let element_name = self.clr_array_element_type_token(&element_msil);
        self.emit_operand(length);
        self.instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Newarr,
            operand: Some(MsilInstructionOperand::Type(element_name)),
        });
        self.emit_stloc(local);
    }

    /// Emit an operator intrinsic assuming operands are already on the evaluation stack.
    fn emit_operator_intrinsic_on_stack(&mut self, opcode: IntrinsicOpcode, output: Option<MirValueRef>) {
        match opcode {
            IntrinsicOpcode::Utf8ScalarSlice => {
                // Utf8ScalarSlice is a call intrinsic and must not arrive at
                // the operator stack path without its receiver/indices.
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Nop, operand: None });
            }
            IntrinsicOpcode::Binary(op) => {
                let msil = match op {
                    IntrinsicBinaryOp::Add => MsilOpcode::Add,
                    IntrinsicBinaryOp::Sub => MsilOpcode::Sub,
                    IntrinsicBinaryOp::Mul => MsilOpcode::Mul,
                    IntrinsicBinaryOp::Div => MsilOpcode::Div,
                    IntrinsicBinaryOp::Rem => MsilOpcode::Rem,
                };
                self.instructions.push(MsilInstruction { label: None, opcode: msil, operand: None });
            }
            IntrinsicOpcode::Neg => {
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Neg, operand: None });
            }
            IntrinsicOpcode::Compare(op) => {
                let compare = match op {
                    IntrinsicCompareOp::Eq | IntrinsicCompareOp::Ne => MsilOpcode::Ceq,
                    IntrinsicCompareOp::Lt => MsilOpcode::Clt,
                    IntrinsicCompareOp::Le => MsilOpcode::Cgt,
                    IntrinsicCompareOp::Gt => MsilOpcode::Cgt,
                    IntrinsicCompareOp::Ge => MsilOpcode::Clt,
                };
                self.instructions.push(MsilInstruction { label: None, opcode: compare, operand: None });
                if matches!(op, IntrinsicCompareOp::Ne | IntrinsicCompareOp::Le | IntrinsicCompareOp::Ge) {
                    self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None });
                    self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ceq, operand: None });
                }
            }
            IntrinsicOpcode::Not => {
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None });
                self.instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ceq, operand: None });
            }
            IntrinsicOpcode::Bitwise(op) => {
                let msil = match op {
                    IntrinsicBitwiseOp::And => MsilOpcode::And,
                    IntrinsicBitwiseOp::Or => MsilOpcode::Or,
                    IntrinsicBitwiseOp::Xor => MsilOpcode::Xor,
                    IntrinsicBitwiseOp::Shl => MsilOpcode::Shl,
                    IntrinsicBitwiseOp::Shr => MsilOpcode::Shr,
                };
                self.instructions.push(MsilInstruction { label: None, opcode: msil, operand: None });
            }
            IntrinsicOpcode::ArrayLen
            | IntrinsicOpcode::ArrayGet
            | IntrinsicOpcode::ArraySet
            | IntrinsicOpcode::ArrayPush
            | IntrinsicOpcode::Deref
            | IntrinsicOpcode::Utf8ScalarLength
            | IntrinsicOpcode::Utf8ContentEqual
            | IntrinsicOpcode::Utf8ContentNotEqual
            | IntrinsicOpcode::Utf8Trim
            | IntrinsicOpcode::Utf8IndexOf
            | IntrinsicOpcode::Utf8Contains
            | IntrinsicOpcode::Utf8StartsWith
            | IntrinsicOpcode::Utf8EndsWith
            | IntrinsicOpcode::SumVariantIs
            | IntrinsicOpcode::SumStructuralEqual => {}
        }
        if let Some(output) = output {
            match opcode {
                IntrinsicOpcode::Binary(_) | IntrinsicOpcode::Neg | IntrinsicOpcode::Bitwise(_) => {
                    self.store_call_result(output, &MsilType::Int32 { signed: true });
                }
                IntrinsicOpcode::Compare(_) | IntrinsicOpcode::Not => {
                    self.store_call_result(output, &MsilType::Bool);
                }
                _ => self.store_to_value(output),
            }
        }
    }
}

/// 从接收者类型提取 trait 名，并通过 witness 表元数据验证。
///
/// 替代原先硬编码的 trait 名列表（"Iterator" | "Future" | ...），
/// 改为检查 `witness_tables` 中是否存在该 trait 名的注册记录。
fn witness_trait_name(witness_tables: &[nyar::WitnessSubmission], ty: &NyarType) -> Option<String> {
    let name = match ty {
        NyarType::TraitObject(object) => object.trait_path.as_str(),
        NyarType::Apply(base, _) => match base.as_ref() {
            NyarType::Named(name) => name.as_str(),
            _ => return None,
        },
        NyarType::Named(name) => name.as_str(),
        _ => return None,
    };
    witness_tables.iter().any(|table| table.trait_name == name).then(|| name.to_string())
}

/// Runtime stub signatures for **explicitly injected** helpers only.
///
/// Requires a bare symbol path (`print`, not `Foo.print`) that is in
/// [`INJECTED_RUNTIME_STUBS`]. Signatures must match `ensure_runtime_stubs`.
fn runtime_stub_signature(path: &NamePath) -> Option<(MsilType, Vec<MsilType>)> {
    use super::witness_abi::{is_injected_runtime_stub_symbol, is_tuple_get_stub_name};

    let parts: Vec<&str> = path.parts().iter().map(|part| part.as_str()).collect();
    if !is_injected_runtime_stub_symbol(&parts) {
        // The self-hosted JVM emitter passes its executable opcode and several
        // aggregate planning records through erased host values.  This is an
        // explicit contract for that one helper, not a general Object(Object)
        // fallback: unknown calls must still fail closed below.
        match parts.last().copied() {
            Some("lower_jvm_opcode_to_flat") => {
                return Some((MsilType::Object, vec![MsilType::Object; 6]));
            }
            Some(name) if name.starts_with("jvm_opcode_") || name.starts_with("jvm_bytes_") => {
                return Some((MsilType::Object, Vec::new()));
            }
            _ => {}
        }
        return None;
    }
    match parts[0] {
        "panic" => Some((MsilType::Void, vec![MsilType::Object])),
        // `@unimplemented` → Call with zero args; returns `object` so match-arm SSA stores
        // type-check (body throws and never returns).
        "unimplemented" => Some((MsilType::Object, vec![])),
        "is_null" => Some((MsilType::Bool, vec![MsilType::Int64 { signed: true }])),
        "unwrap_null" => Some((MsilType::Int64 { signed: true }, vec![MsilType::Int64 { signed: true }])),
        "print" => Some((MsilType::Int32 { signed: true }, vec![MsilType::Object])),
        // `format("{}", x)` — template + value; stub ignores template and stringifies value.
        "format" => Some((MsilType::String, vec![MsilType::Object, MsilType::Object])),
        name if is_tuple_get_stub_name(name) => Some((MsilType::Object, vec![MsilType::Object])),
        _ => None,
    }
}

/// `…Array.get` / `…Array.set` — ordinal host contracts (1-based index).
fn array_ordinal_host_kind(operation: &QualifiedName) -> Option<&'static str> {
    let parts = operation.parts();
    if parts.len() < 2 || parts[parts.len() - 2].as_str() != "Array" {
        return None;
    }
    match parts.last().map(|part| part.as_str()) {
        Some("get") => Some("get"),
        Some("set") => Some("set"),
        _ => None,
    }
}

fn path_is_qualified_array_ordinal_host(path: &NamePath) -> bool {
    let parts = path.parts();
    parts.len() >= 2 && parts[parts.len() - 2].as_str() == "Array" && matches!(parts.last().map(|part| part.as_str()), Some("get" | "set"))
}

fn mir_function_is_empty_host(mir_fn: &MirFunction) -> bool {
    mir_fn.intrinsic.is_none() && mir_fn.blocks.iter().all(|block| block.instructions.is_empty())
}

/// Synthesize ordinal `Array.get` / `Array.set` when the host_contract MIR body is empty.
///
/// Semantics match `std.adaptor.clr.collection.array_host_get/set`:
/// - ordinal `0` or `> length` → None / no-op
/// - otherwise offset `ordinal - 1` via `ldelem` / `stelem` (cardinal/offset layer only).
fn synthesize_array_ordinal_host_method(
    submission: &FragmentSubmission,
    operation: &QualifiedName,
    mir_fn: &MirFunction,
    kind: &str,
) -> MsilMethodBody {
    let parameter_types: Vec<MsilType> = mir_fn
        .param_types
        .iter()
        .map(|ty| match nyar_type_to_msil(ty, &submission.aggregate_layouts) {
            // Match existing Array_* MethodDef convention (object receiver / element).
            MsilType::Named(_) | MsilType::SzArray(_) => MsilType::Object,
            other => other,
        })
        .collect();
    synthesize_array_ordinal_host_with_params(operation, kind, parameter_types)
}

/// Emit ordinal `Array.get` / `Array.set` when MIR is missing (externalized host_contract).
pub(crate) fn synthesize_array_ordinal_host_standalone(operation: &QualifiedName, kind: &str) -> MsilMethodBody {
    let parameter_types = match kind {
        "get" => vec![MsilType::Object, MsilType::Int32 { signed: false }],
        _ => vec![MsilType::Object, MsilType::Int32 { signed: false }, MsilType::Object],
    };
    synthesize_array_ordinal_host_with_params(operation, kind, parameter_types)
}

fn synthesize_array_ordinal_host_with_params(operation: &QualifiedName, kind: &str, parameter_types: Vec<MsilType>) -> MsilMethodBody {
    let (return_type, locals, instructions) = match kind {
        "get" => (MsilType::Object, vec![MsilType::Object], synthesize_array_ordinal_get_instructions()),
        _ => (MsilType::Void, Vec::new(), synthesize_array_ordinal_set_instructions()),
    };
    MsilMethodBody {
        method: MsilMethodRef {
            owner: None,
            name: sanitize_operation_symbol(operation),
            signature: MsilMethodSignature::new(return_type, parameter_types),
        },
        locals,
        instructions,
        max_stack: 8,
        is_entry_point: false,
        is_async: false,
    }
}

fn synthesize_array_ordinal_get_instructions() -> Vec<MsilInstruction> {
    // Array_get(array, ordinal) -> Option
    // if ordinal == 0 || ordinal > ldlen(array) -> None; else Some(ldelem(array, ordinal-1))
    // local 0 = element payload while building Option
    let none = "array_get_none".to_string();
    vec![
        MsilInstruction { label: None, opcode: MsilOpcode::Ldarg1, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::Brfalse, operand: Some(MsilInstructionOperand::BranchTarget(none.clone())) },
        MsilInstruction { label: None, opcode: MsilOpcode::Ldarg1, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::Ldlen, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::BgtUn, operand: Some(MsilInstructionOperand::BranchTarget(none.clone())) },
        // payload = array[ordinal - 1]
        MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::Ldarg1, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::LdcI4, operand: Some(MsilInstructionOperand::Integer(1)) },
        MsilInstruction { label: None, opcode: MsilOpcode::Sub, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::LdelemRef, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::Stloc0, operand: None },
        // Option Some (tag = 0)
        MsilInstruction {
            label: None,
            opcode: MsilOpcode::Newobj,
            operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                owner: Some("Option".to_string()),
                name: ".ctor".to_string(),
                signature: MsilMethodSignature::new(MsilType::Void, Vec::new()),
            })),
        },
        MsilInstruction { label: None, opcode: MsilOpcode::Dup, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::LdcI4, operand: Some(MsilInstructionOperand::Integer(0)) },
        MsilInstruction {
            label: None,
            opcode: MsilOpcode::Stfld,
            operand: Some(MsilInstructionOperand::Field("Option".to_string(), "tag".to_string())),
        },
        MsilInstruction { label: None, opcode: MsilOpcode::Dup, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::Ldloc0, operand: None },
        MsilInstruction {
            label: None,
            opcode: MsilOpcode::Stfld,
            operand: Some(MsilInstructionOperand::Field("Option".to_string(), "payload".to_string())),
        },
        MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
        // None (tag = 1, null payload)
        MsilInstruction {
            label: Some(none),
            opcode: MsilOpcode::Newobj,
            operand: Some(MsilInstructionOperand::Method(MsilMethodRef {
                owner: Some("Option".to_string()),
                name: ".ctor".to_string(),
                signature: MsilMethodSignature::new(MsilType::Void, Vec::new()),
            })),
        },
        MsilInstruction { label: None, opcode: MsilOpcode::Dup, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::LdcI4, operand: Some(MsilInstructionOperand::Integer(1)) },
        MsilInstruction {
            label: None,
            opcode: MsilOpcode::Stfld,
            operand: Some(MsilInstructionOperand::Field("Option".to_string(), "tag".to_string())),
        },
        MsilInstruction { label: None, opcode: MsilOpcode::Dup, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::Ldnull, operand: None },
        MsilInstruction {
            label: None,
            opcode: MsilOpcode::Stfld,
            operand: Some(MsilInstructionOperand::Field("Option".to_string(), "payload".to_string())),
        },
        MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
    ]
}

fn synthesize_array_ordinal_set_instructions() -> Vec<MsilInstruction> {
    let done = "array_set_done".to_string();
    vec![
        MsilInstruction { label: None, opcode: MsilOpcode::Ldarg1, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::Brfalse, operand: Some(MsilInstructionOperand::BranchTarget(done.clone())) },
        MsilInstruction { label: None, opcode: MsilOpcode::Ldarg1, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::Ldlen, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::BgtUn, operand: Some(MsilInstructionOperand::BranchTarget(done.clone())) },
        MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::Ldarg1, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::LdcI4, operand: Some(MsilInstructionOperand::Integer(1)) },
        MsilInstruction { label: None, opcode: MsilOpcode::Sub, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::Ldarg2, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::StelemRef, operand: None },
        MsilInstruction { label: Some(done), opcode: MsilOpcode::Ret, operand: None },
    ]
}

fn clr_array_element_type_name(element: &MsilType) -> String {
    match element {
        MsilType::Bool => "bool".to_string(),
        MsilType::Char => "char".to_string(),
        MsilType::Int8 { signed: true } => "int8".to_string(),
        MsilType::Int8 { signed: false } => "uint8".to_string(),
        MsilType::Int16 { signed: true } => "int16".to_string(),
        MsilType::Int16 { signed: false } => "uint16".to_string(),
        MsilType::Int32 { signed: true } => "int32".to_string(),
        MsilType::Int32 { signed: false } => "uint32".to_string(),
        MsilType::Int64 { signed: true } => "int64".to_string(),
        MsilType::Int64 { signed: false } => "uint64".to_string(),
        MsilType::Float32 => "float32".to_string(),
        MsilType::Float64 => "float64".to_string(),
        MsilType::String => "string".to_string(),
        MsilType::Object => "object".to_string(),
        // Unqualified Named without a resolved layout must not reach PE TypeRef.
        MsilType::Named(name) if name.starts_with('[') => name.clone(),
        MsilType::Named(_) => "object".to_string(),
        MsilType::SzArray(inner) => format!("{}[]", clr_array_element_type_name(inner)),
        MsilType::IntPtr { signed: true } => "native int".to_string(),
        MsilType::IntPtr { signed: false } => "native unsigned int".to_string(),
        MsilType::Void => "object".to_string(),
    }
}

/// Map Valkyrie-desired host-call returns onto real BCL MSIL signatures.
///
/// Host-contract flatten copies the contract return type onto the `[clr(...)]` MemberRef.
/// That is wrong for APIs such as `Directory.CreateDirectory` → `DirectoryInfo` and
/// `File.WriteAllText` → `void` when the contract is `bool`.
fn clr_host_bcl_return(target: &ClrHostMethodTarget<'_>, desired: &MsilType) -> (MsilType, ClrHostReturnAdapt) {
    match (target.owner, target.method) {
        ("System.IO.Directory", "CreateDirectory") => {
            let actual = MsilType::Named(format!("[{}]System.IO.DirectoryInfo", target.assembly));
            let adapt = match desired {
                MsilType::Bool => ClrHostReturnAdapt::DiscardRef { push_true: true },
                MsilType::Void => ClrHostReturnAdapt::DiscardRef { push_true: false },
                // Keep DirectoryInfo on the stack when the callee actually wants the object.
                MsilType::Named(_) | MsilType::Object => ClrHostReturnAdapt::Identity,
                _ => ClrHostReturnAdapt::DiscardRef { push_true: false },
            };
            (actual, adapt)
        }
        ("System.IO.File", "WriteAllText") => {
            let adapt = match desired {
                MsilType::Bool => ClrHostReturnAdapt::PushTrue,
                _ => ClrHostReturnAdapt::Identity,
            };
            (MsilType::Void, adapt)
        }
        // `Process.Start(string, string)` returns `Process`, not void/i32.
        // Emitting `call void|int32 Start(...)` is InvalidProgramException.
        ("System.Diagnostics.Process", "Start") => {
            let actual = MsilType::Named(format!("[{}]System.Diagnostics.Process", target.assembly));
            let adapt = match desired {
                MsilType::Void => ClrHostReturnAdapt::DiscardRef { push_true: false },
                // Legacy contracts typed Start as i32/bool — discard Process, push 0/1.
                MsilType::Int32 { .. } | MsilType::Bool => ClrHostReturnAdapt::DiscardRef { push_true: false },
                MsilType::Named(_) | MsilType::Object => ClrHostReturnAdapt::Identity,
                _ => ClrHostReturnAdapt::DiscardRef { push_true: false },
            };
            (actual, adapt)
        }
        _ => (desired.clone(), ClrHostReturnAdapt::Identity),
    }
}

/// Map host-call parameter MemberRef types onto real BCL signatures.
///
/// `std.io.get_files(..., recursive: bool)` is often bound straight to `Directory.GetFiles`
/// (host_provider elides the bool→SearchOption wrapper). BCL's third parameter is
/// `System.IO.SearchOption` (enum valuetype), not `Boolean` — a Bool MemberRef becomes
/// MissingMethodException. Bool/I4 share the evaluation-stack representation, so only the
/// MemberRef type token needs to name `SearchOption`.
fn clr_host_bcl_param_types(target: &ClrHostMethodTarget<'_>, mut param_types: Vec<MsilType>) -> Vec<MsilType> {
    if target.owner == "System.IO.Directory" && target.method == "GetFiles" && param_types.len() >= 3 {
        param_types[2] = MsilType::Named("[System.Runtime]System.IO.SearchOption".to_string());
    }
    param_types
}

/// Locals may not use `ELEMENT_TYPE_VOID` (ECMA-335 LocalVarSig). Slot planning already maps
/// `Unit`→`int32`; retype/miss paths must not re-poison a slot back to `Void`.
fn sanitize_clr_local_type(ty: MsilType) -> MsilType {
    if matches!(ty, MsilType::Void) { MsilType::Int32 { signed: true } } else { ty }
}

fn msil_type_is_numeric(ty: &MsilType) -> bool {
    matches!(
        ty,
        MsilType::Bool
            | MsilType::Char
            | MsilType::Int8 { .. }
            | MsilType::Int16 { .. }
            | MsilType::Int32 { .. }
            | MsilType::Int64 { .. }
            | MsilType::Float32
            | MsilType::Float64
    )
}

fn instance_receiver_types_compatible(expected: &NyarType, actual: &NyarType) -> bool {
    if matches!(expected, NyarType::Named(name) if name.as_str() == "Self") {
        return true;
    }
    if expected == actual {
        return true;
    }
    false
}

fn receiver_type_name(ty: &NyarType) -> Option<&str> {
    match ty {
        NyarType::Utf8 => Some("utf8"),
        NyarType::Utf16 => Some("utf16"),
        NyarType::Array(_) => Some("Array"),
        NyarType::Named(name) => Some(name.as_str()),
        NyarType::Apply(base, _) => receiver_type_name(base),
        _ => None,
    }
}

fn name_is_std_array(name: &str) -> bool {
    name == "array"
        || name == "Array"
        || name.ends_with(".Array")
        || name.ends_with("::Array")
        // Keep ends_with("Array") for `std.collection.Array` / mangled owners, but reject
        // `ArrayIterator` / `ArrayList` which are not the ordinal host.
        || (name.ends_with("Array") && !name.contains("Iterator") && !name.contains("List") && !name.contains("Map"))
}

fn nyar_type_is_std_array(ty: &NyarType) -> bool {
    match ty {
        NyarType::Array(_) => true,
        NyarType::Named(name) => name_is_std_array(name.as_str()),
        NyarType::Apply(base, _) => nyar_type_is_std_array(base),
        _ => false,
    }
}

fn qualified_name_common_prefix_len(left: &QualifiedName, right: &QualifiedName) -> usize {
    left.parts().iter().zip(right.parts().iter()).take_while(|(lhs, rhs)| lhs.as_str() == rhs.as_str()).count()
}

fn select_best_qualified_name_candidate(candidates: &[QualifiedName], context: &QualifiedName) -> Option<QualifiedName> {
    candidates.iter().max_by_key(|candidate| qualified_name_common_prefix_len(candidate, context)).cloned()
}
