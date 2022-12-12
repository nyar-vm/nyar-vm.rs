//! Sum construction / payload projection / variant test from exact MIR contracts (ADR 0008).
//!
//! `SumNew` / `SumPayloadGet` / `SumVariantIs` carry `sum_type` + `type_args` + `variant`
//! (`NominalInstanceKey`). This module maps that identity onto a registered
//! Wasm GC type index (`RepresentationId` / `sum_representation_key`) and
//! encodes struct ops.
//!
//! Known scalar payloads (`i32`/`bool`/…) use typed GC fields. Unknown / reference
//! payloads keep transitional `anyref` (nullary empty slot = `ref.null`).

#![allow(deprecated)]
use super::super::*;
use nyar_types::{NominalInstanceKey, sum_representation_key};

#[allow(deprecated)]
impl<'a> WasmMirLowerer<'a> {
    /// Emit `SumNew` from exact MIR `NominalInstanceKey` + variant / optional payload.
    pub(crate) fn emit_sum_new(
        &mut self,
        sum_type: &str,
        type_args: &[NyarType],
        variant: &str,
        payload_type: Option<&NyarType>,
        payload: Option<&MirOperand>,
        output: Option<MirValueRef>,
    ) {
        let instance = NominalInstanceKey::new(sum_type, type_args.to_vec());
        let repr = instance.representation_id();
        let Some(type_index) = self.gc_sum_type_indices.get(repr.as_str()).copied()
        else {
            panic!(
                "WASM emit fail-closed: SumNew missing RepresentationId `{}` in `{}` (ADR 0008 / M3)",
                repr, self.mir_fn.symbol
            );
        };
        let tag = self
            .ctx
            .submission
            .iter()
            .find(|s| s.name == sum_type)
            .and_then(|s| s.variants.iter().find(|v| v.name == variant).map(|v| v.tag))
            .unwrap_or_else(|| {
                panic!(
                    "WASM emit fail-closed: SumNew unknown variant `{sum_type}::{variant}` in `{}` (ADR 0008)",
                    self.mir_fn.symbol
                )
            });
        let payload_field = super::super::type_registry::sum_payload_field_valtype(payload_type);

        self.emit_struct_new_default(type_index);
        let tmp = self.alloc_anyref_local();
        self.emit_local_set(tmp);
        self.emit_local_get(tmp);
        self.emit_ref_cast_struct(type_index);
        self.emit_i32_const(tag as i32);
        self.emit_struct_set(type_index, 0);

        match payload {
            None => {
                if payload_field == VALTYPE_ANYREF {
                    self.emit_ref_null_anyref();
                }
                else if payload_field == VALTYPE_I64 {
                    self.emit_i64_const(0);
                }
                else if payload_field == VALTYPE_F64 {
                    self.code.push(0x44);
                    self.code.extend_from_slice(&0f64.to_le_bytes());
                }
                else {
                    self.emit_i32_const(0);
                }
            }
            Some(operand) => {
                if payload_field == VALTYPE_ANYREF {
                    self.emit_operand_as_unite_payload_contract(operand);
                }
                else {
                    // Typed payload field: no anyref box invent (ADR 0008).
                    self.emit_operand_coerced(operand, payload_field);
                }
            }
        }
        let payload_tmp = match payload_field {
            VALTYPE_ANYREF => self.alloc_anyref_local(),
            VALTYPE_I64 => self.alloc_i64_local(),
            VALTYPE_F64 => self.alloc_f64_local(),
            _ => self.alloc_i32_local(),
        };
        self.emit_local_set(payload_tmp);
        self.emit_local_get(tmp);
        self.emit_ref_cast_struct(type_index);
        self.emit_local_get(payload_tmp);
        self.emit_struct_set(type_index, 1);

        if let Some(out) = output {
            self.emit_local_get(tmp);
            self.force_output_local_for_stack_type(out, VALTYPE_ANYREF);
            self.assign_output_local(out);
        }
    }

    /// Emit `SumPayloadGet` from exact MIR sum instance + variant identity.
    pub(crate) fn emit_sum_payload_get(
        &mut self,
        sum_type: &str,
        type_args: &[NyarType],
        _variant: &str,
        payload_type: Option<&NyarType>,
        object: &MirOperand,
        output: Option<MirValueRef>,
    ) {
        let repr = sum_representation_key(sum_type, type_args);
        let Some(type_index) = self.gc_sum_type_indices.get(&repr).copied()
        else {
            panic!(
                "WASM emit fail-closed: SumPayloadGet missing RepresentationId `{}` in `{}` (ADR 0008 / M3)",
                repr, self.mir_fn.symbol
            );
        };
        let Some(object_local) = self.operand_reference_local(object)
        else {
            panic!(
                "WASM emit fail-closed: SumPayloadGet object is not a reference local in `{}` (ADR 0008)",
                self.mir_fn.symbol
            );
        };
        if self.wasm_local_value_type(object_local) == VALTYPE_I32 {
            panic!(
                "WASM emit fail-closed: SumPayloadGet object local is i32, not GC ref in `{}` (ADR 0008)",
                self.mir_fn.symbol
            );
        }
        let payload_field = super::super::type_registry::sum_payload_field_valtype(payload_type);

        self.emit_local_get(object_local);
        self.emit_ref_cast_struct(type_index);
        self.emit_struct_get(type_index, 1);

        if let Some(out) = output {
            if payload_field == VALTYPE_ANYREF {
                let wants_i32 = self.mir_fn.value_types.get(&out).is_some_and(|ty| self.unite_payload_wants_i32_unbox(ty));
                if wants_i32 {
                    self.emit_unbox_i32_payload();
                    self.force_output_local_for_stack_type(out, VALTYPE_I32);
                }
                else {
                    self.force_output_local_for_stack_type(out, VALTYPE_ANYREF);
                }
            }
            else {
                self.force_output_local_for_stack_type(out, payload_field);
            }
            self.assign_output_local(out);
        }
        else {
            WasmOpcode::Drop.encode(&mut self.code);
        }
    }

    /// Emit `SumVariantIs`: cast to RepresentationId struct, load tag field, compare to variant tag.
    pub(crate) fn emit_sum_variant_is(
        &mut self,
        sum_type: &str,
        type_args: &[NyarType],
        variant: &str,
        object: &MirOperand,
        output: Option<MirValueRef>,
    ) {
        let repr = sum_representation_key(sum_type, type_args);
        let Some(type_index) = self.gc_sum_type_indices.get(&repr).copied()
        else {
            panic!(
                "WASM emit fail-closed: SumVariantIs missing RepresentationId `{}` in `{}` (ADR 0008 / M3)",
                repr, self.mir_fn.symbol
            );
        };
        let tag = self
            .ctx
            .submission
            .iter()
            .find(|s| s.name == sum_type)
            .and_then(|s| s.variants.iter().find(|v| v.name == variant).map(|v| v.tag))
            .unwrap_or_else(|| {
                panic!(
                    "WASM emit fail-closed: SumVariantIs unknown variant `{sum_type}::{variant}` in `{}` (ADR 0008)",
                    self.mir_fn.symbol
                )
            });
        let Some(object_local) = self.operand_reference_local(object)
        else {
            panic!(
                "WASM emit fail-closed: SumVariantIs object is not a reference local in `{}` (ADR 0008)",
                self.mir_fn.symbol
            );
        };

        self.emit_local_get(object_local);
        self.emit_ref_cast_struct(type_index);
        self.emit_struct_get(type_index, 0);
        self.emit_i32_const(tag as i32);
        WasmOpcode::I32Eq.encode(&mut self.code);

        if let Some(out) = output {
            self.force_output_local_for_stack_type(out, VALTYPE_I32);
            self.assign_output_local(out);
        }
        else {
            WasmOpcode::Drop.encode(&mut self.code);
        }
    }

    /// Payload encoding for unite `[tag, anyref]`. Unknown stack widths fail closed.
    pub(crate) fn emit_operand_as_unite_payload_contract(&mut self, operand: &MirOperand) {
        if self.operand_reference_local(operand).is_some() {
            self.emit_operand(operand);
            return;
        }
        let actual = self.operand_wasm_stack_type(operand);
        if actual == WASM_GC_ANYREF || actual == WASM_GC_EXTERNREF {
            self.emit_operand_coerced(operand, VALTYPE_ANYREF);
            return;
        }
        if actual == VALTYPE_I32 {
            self.emit_box_i32_payload(operand);
            return;
        }
        panic!(
            "WASM emit fail-closed: unite payload has no Representation for stack type {actual:#x} in `{}` (ADR 0008)",
            self.mir_fn.symbol
        );
    }
}
