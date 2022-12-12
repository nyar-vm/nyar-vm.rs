use nyar_language::{MirLowerer, MirOperand, MirOperation, ValkyrieCompiler, types::SourceID};

#[test]
fn cross_namespace_call_attaches_parameter_types() {
    let source = r#"
namespace std.data.binary.wasm;

unite WasmValueType {
    I32
}

micro wasm_i32_types(count: u32) -> [WasmValueType] {
    let mut out: [WasmValueType] = []
    return out
}

namespace nyar.emitter.wasi;

structure WitWasiCoreImportSig {
    param_i32_count: u32
}

micro wasi_core_sig_to_functype(sig: WitWasiCoreImportSig) -> [WasmValueType] {
    return wasm_i32_types(sig.param_i32_count)
}
"#;
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9610 }).compile_source(source).expect("compile");
    let mir = MirLowerer::lower_module_semantic(&hir);
    let caller = mir
        .functions
        .iter()
        .find(|function| function.symbol.contains("wasi_core_sig_to_functype"))
        .unwrap_or_else(|| panic!("caller missing; functions={:?}", mir.functions.iter().map(|f| &f.symbol).collect::<Vec<_>>()));
    let mut found = false;
    for block in &caller.blocks {
        for instruction in &block.instructions {
            let MirOperation::Call { callee: MirOperand::Symbol(path), .. } = &instruction.kind
            else {
                continue;
            };
            let is_target = path.parts().last().is_some_and(|part| part.as_str() == "wasm_i32_types");
            if !is_target {
                continue;
            }
            found = true;
            assert!(
                parameter_types.as_ref().is_some_and(|params| !params.is_empty()),
                "expected attached parameter_types on wasm_i32_types call; got parameter_types={parameter_types:?} output_ty={output_ty:?} path={path}"
            );
        }
    }
    assert!(found, "wasm_i32_types call not found; functions={:?}", mir.functions.iter().map(|f| &f.symbol).collect::<Vec<_>>());
}
