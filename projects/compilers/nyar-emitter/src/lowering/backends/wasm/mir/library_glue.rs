//! 库模式 JSON ABI 只消费已绑定实例的明确标量类型合同。

use crate::{
    FragmentSubmission,
    nyar_backend_wasi::{WasmBinaryModule, WasmPackageKind, WasmSection},
};
use miette::{Result, miette};
use nyar::NyarType;

fn abi_kind_for_type(ty: &NyarType) -> Option<&'static str> {
    match ty {
        NyarType::Integer64 { .. } => Some("i64"),
        _ => None,
    }
}

pub(super) fn append_library_mode_glue(
    wasm_package_kind: WasmPackageKind,
    submission: &FragmentSubmission,
    module: &mut WasmBinaryModule,
) -> Result<()> {
    if wasm_package_kind != WasmPackageKind::Library {
        return Ok(());
    }
    let mut invoke_exports = serde_json::Map::new();
    for (instance, public_name) in submission.backend_plan.wasm_export_names() {
        let function =
            submission.backend_plan.get_function(instance).ok_or_else(|| miette!("库导出 `{instance}` 缺少 Compiler 函数体"))?.function;
        let parameters = function
            .param_types
            .iter()
            .map(abi_kind_for_type)
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| miette!("库导出 `{instance}` 参数缺少明确 JSON ABI 合同"))?;
        let returns = abi_kind_for_type(&function.return_type)
            .ok_or_else(|| miette!("库导出 `{instance}` 结果缺少明确 JSON ABI 合同；禁止按类型名或布局猜测集合桥"))?;
        if invoke_exports
            .insert(
                public_name.clone(),
                serde_json::json!({
                    "params": parameters,
                    "returns": returns,
                }),
            )
            .is_some()
        {
            return Err(miette!("库导出公开名 `{public_name}` 重复"));
        }
    }
    if invoke_exports.is_empty() {
        return Err(miette!("库 Wasm 产物没有可表达的导出合同"));
    }
    let payload = serde_json::json!({ "exports": invoke_exports, "glue": {} });
    module.sections.push(WasmSection { id: 0, name: Some("nyar.library_invoke".to_owned()), bytes: payload.to_string().into_bytes() });
    Ok(())
}
