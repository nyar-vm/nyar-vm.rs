//! Library-mode wasm glue exports and `nyar.library_invoke` ABI metadata.
//!
//! Emits hidden `__nyar_glue.*` exports for Node `callExport` JSON↔GC marshaling.
//! ABI kinds are derived from MIR `NyarType` structure, not runtime string dispatch.

use std::collections::BTreeMap;

use miette::{Result, miette};
use nyar::NyarType;

use crate::{
    backend_plan_views::ExecutableFunction as MirFunction,
    nyar_backend_wasi::{WasmPackageKind, WasmBinaryModule, WasmSection},
};
use std_data::binary::wasm::{
    VALTYPE_ANYREF, VALTYPE_I32, VALTYPE_I64, WasmExternalKind, WasmOpcode, encode_array_get, encode_i64_const, encode_local_get,
    encode_local_tee, encode_ref_cast_type_index, encode_ref_is_null, encode_return, encode_struct_get,
};

use super::{
    ExecutableLoweringContext, FragmentSubmission, LayoutId,
    representation::wasm_gc_field_type_byte_for_glue,
    type_registry::wasm_array_element_type_key,
};
use crate::lowering::backends::wasm::{
    gc::wasm_gc_array_type,
    sections::{encode_uleb128, wasm_function_body, wasm_function_type},
};

const GLUE_LIST_NEW: &str = "__nyar_glue.ArrayList.new";
const GLUE_LIST_PUSH: &str = "__nyar_glue.ArrayList.push";
const GLUE_LIST_LENGTH: &str = "__nyar_glue.ArrayList.length";
const GLUE_LIST_AT: &str = "__nyar_glue_list_i64_at";

fn abi_kind_for_type(ty: &NyarType) -> Option<&'static str> {
    if is_i64_type(ty) {
        return Some("i64");
    }
    if is_array_list_of_i64(ty) {
        return Some("list_i64");
    }
    None
}

fn is_i64_type(ty: &NyarType) -> bool {
    matches!(ty, NyarType::Integer64 { .. }) || matches!(ty, NyarType::Named(name) if name.as_str() == "i64")
}

fn is_array_list_of_i64(ty: &NyarType) -> bool {
    match ty {
        NyarType::Apply(base, args) if args.len() == 1 && is_i64_type(&args[0]) => {
            matches!(base.as_ref(), NyarType::Named(name) if name.as_str() == "ArrayList")
        }
        NyarType::Named(name) if name.as_str() == "ArrayList" => true,
        _ => false,
    }
}

fn array_element_type_from_field_ty(field_ty: &NyarType) -> Option<&NyarType> {
    match field_ty {
        NyarType::Array(element) | NyarType::FixedArray { element, .. } => Some(element.as_ref()),
        _ => None,
    }
}

fn resolve_array_list_items_array_type(
    ctx: &ExecutableLoweringContext,
    gc_array_type_indices: &BTreeMap<String, u32>,
    js_glue_utf8_as_anyref: bool,
) -> Option<(u32, u8)> {
    let layout = ctx.layout_by_type_name("ArrayList")?;
    let items_field = layout.fields.iter().find(|field| field.name == "_items")?;
    let element_type = array_element_type_from_field_ty(&items_field.ty)?;
    let key = wasm_array_element_type_key(element_type);
    let type_index = gc_array_type_indices.get(&key).copied()?;
    let element_valtype = wasm_gc_field_type_byte_for_glue(element_type, js_glue_utf8_as_anyref);
    Some((type_index, element_valtype))
}

fn resolve_array_list_struct_type_index(ctx: &ExecutableLoweringContext, gc_struct_type_indices: &BTreeMap<LayoutId, u32>) -> Option<u32> {
    for layout in &ctx.layouts.layouts {
        let simple = layout.name.rsplit("::").next().unwrap_or(layout.name.as_str());
        if simple == "ArrayList" {
            if let Some(index) = gc_struct_type_indices.get(&layout.id) {
                return Some(*index);
            }
        }
    }
    ctx.layout_by_type_name("ArrayList")
        .and_then(|layout| gc_struct_type_indices.get(&layout.id).copied())
}

fn encode_unbox_i64_payload(box_type: u32, tmp_local: u32, body: &mut Vec<u8>) {
    encode_local_tee(tmp_local, body);
    encode_ref_is_null(body);
    body.push(0x04);
    body.push(VALTYPE_I64);
    encode_i64_const(0, body);
    body.push(0x05);
    encode_local_get(tmp_local, body);
    encode_ref_cast_type_index(box_type, body);
    encode_struct_get(box_type, 0, body);
    body.push(0x0B);
}

fn glue_list_i64_at_body(
    array_list_struct_type: u32,
    items_array_type: u32,
    items_element_valtype: u8,
    gc_i64_box_type_index: u32,
) -> Vec<u8> {
    let mut body = Vec::new();
    encode_uleb128(1, &mut body);
    encode_uleb128(1, &mut body);
    body.push(VALTYPE_ANYREF);
    encode_local_get(0, &mut body);
    encode_ref_cast_type_index(array_list_struct_type, &mut body);
    encode_struct_get(array_list_struct_type, 0, &mut body);
    encode_ref_cast_type_index(items_array_type, &mut body);
    encode_local_get(1, &mut body);
    encode_array_get(items_array_type, &mut body);
    if items_element_valtype == VALTYPE_ANYREF {
        encode_unbox_i64_payload(gc_i64_box_type_index, 2, &mut body);
    }
    encode_return(&mut body);
    WasmOpcode::End.encode(&mut body);
    body
}

pub(super) fn append_library_mode_glue(
    wasm_package_kind: WasmPackageKind,
    submission: &FragmentSubmission,
    ctx: &ExecutableLoweringContext,
    module: &mut WasmBinaryModule,
    exports: &mut Vec<(&str, u8, u32)>,
    type_indices: &mut Vec<Vec<u8>>,
    function_indices: &mut Vec<u32>,
    code_bodies: &mut Vec<Vec<u8>>,
    import_count: u32,
    function_index_by_name: &BTreeMap<String, u32>,
    gc_struct_type_indices: &BTreeMap<LayoutId, u32>,
    gc_array_type_indices: &BTreeMap<String, u32>,
    gc_i64_box_type_index: u32,
    js_glue_utf8_as_anyref: bool,
) -> Result<()> {
    if wasm_package_kind != WasmPackageKind::Library {
        return Ok(());
    }

    let mut invoke_exports = serde_json::Map::new();
    let executable = &submission.backend_plan;

    for (operation, public_name) in &submission.wasm_export_names {
        let Some(mir_fn) = executable.get_function(operation).map(|view| view.function.clone())
        else {
            return Err(miette!("library export `{operation}` 缺少 Compiler-owned 函数体"));
        };

        let params = mir_param_abi_kinds(&mir_fn);
        let returns = mir_return_abi_kind(&mir_fn);
        if params.iter().any(|kind| kind.is_none()) || returns.is_none() {
            return Err(miette!("library export `{operation}` 的 JSON ABI 尚无完整类型合同"));
        }
        invoke_exports.insert(
            public_name.clone(),
            serde_json::json!({
                "params": params.into_iter().map(|kind| kind.unwrap()).collect::<Vec<_>>(),
                "returns": returns.unwrap(),
            }),
        );
    }

    if invoke_exports.is_empty() {
        return Err(miette!("library wasm package 没有可表达的导出合同"));
    }

    for (glue_name, source_name) in [
        (GLUE_LIST_NEW, "ArrayList::new"),
        (GLUE_LIST_PUSH, "ArrayList::push"),
        (GLUE_LIST_LENGTH, "ArrayList::length"),
    ] {
        let function_index = function_index_by_name
            .get(source_name)
            .copied()
            .ok_or_else(|| miette!("library glue `{glue_name}` 缺少已规划的 `{source_name}` 函数"))?;
        exports.push((glue_name, WasmExternalKind::Func.as_u8(), function_index));
    }

    let array_list_struct_type = resolve_array_list_struct_type_index(ctx, gc_struct_type_indices)
        .ok_or_else(|| miette!("library glue requires the Compiler-planned ArrayList layout"))?;
    let (items_array_type, items_element_valtype) = resolve_array_list_items_array_type(ctx, gc_array_type_indices, js_glue_utf8_as_anyref)
        .ok_or_else(|| miette!("library glue requires the Compiler-planned ArrayList items array layout"))?;
        let type_index = u32::try_from(type_indices.len()).expect("type index overflow");
        type_indices.push(wasm_function_type(&[VALTYPE_ANYREF, VALTYPE_I32], &[VALTYPE_I64]));
        function_indices.push(type_index);
        let function_index = import_count + u32::try_from(code_bodies.len()).expect("function index overflow");
        code_bodies.push(wasm_function_body(glue_list_i64_at_body(
            array_list_struct_type,
            items_array_type,
            items_element_valtype,
            gc_i64_box_type_index,
        )));
        exports.push((GLUE_LIST_AT, WasmExternalKind::Func.as_u8(), function_index));

    let payload = serde_json::json!({
        "exports": invoke_exports,
        "glue": {
            "list_i64": {
                "new": GLUE_LIST_NEW,
                "push": GLUE_LIST_PUSH,
                "length": GLUE_LIST_LENGTH,
                "at": GLUE_LIST_AT,
            }
        }
    });
    module.sections.push(WasmSection {
        id: 0,
        name: Some("nyar.library_invoke".to_string()),
        bytes: payload.to_string().into_bytes(),
    });
    Ok(())
}

fn mir_param_abi_kinds(mir_fn: &MirFunction) -> Vec<Option<&'static str>> {
    mir_fn.param_types.iter().map(|ty| abi_kind_for_type(ty)).collect()
}

fn mir_return_abi_kind(mir_fn: &MirFunction) -> Option<&'static str> {
    if matches!(mir_fn.return_type, NyarType::Unit | NyarType::Bottom) {
        return None;
    }
    abi_kind_for_type(&mir_fn.return_type)
}
