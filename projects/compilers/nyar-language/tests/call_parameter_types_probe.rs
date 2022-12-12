use nyar_language::{MirLowerer, MirOperand, MirOperation, ValkyrieCompiler, types::SourceID};

#[test]
fn literal_u32_call_attaches_parameter_types() {
    let source = r#"
namespace std.data.binary.wasm;

enums WasmValueType {
    I32
}

micro wasm_i32_types(count: u32) -> [WasmValueType] {
    let mut out: [WasmValueType] = []
    return out
}

namespace nyar.emitter.wasi;

micro caller() -> [WasmValueType] {
    return wasm_i32_types(4)
}
"#;
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9611 }).compile_source(source).expect("compile");
    let mir = MirLowerer::lower_module_semantic(&hir);
    let caller = mir.functions.iter().find(|f| f.symbol.contains("caller")).expect("caller");
    let mut found = false;
    for block in &caller.blocks {
        for instruction in &block.instructions {
            if let MirOperation::Call { callee: MirOperand::Symbol(path), .. } = &instruction.kind {
                if path.parts().last().is_some_and(|p| p.as_str() == "wasm_i32_types") {
                    found = true;
                    eprintln!("literal4 path={path} parameter_types={parameter_types:?} output_ty={output_ty:?}");
                    assert!(
                        parameter_types.as_ref().is_some_and(|p| !p.is_empty()),
                        "missing parameter_types: {parameter_types:?} output={output_ty:?}"
                    );
                }
            }
        }
    }
    assert!(found, "call missing");
}
