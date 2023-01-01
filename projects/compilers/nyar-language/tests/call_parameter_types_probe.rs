use nyar_language::{MirLowerer, MirOperand, MirOperation, MirValueRef, ValkyrieCompiler, types::{SourceID, hir::ValkyrieType}};

#[test]
fn literal_u32_call_preserves_argument_type_in_value_types() {
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
            if let MirOperation::Call { callee: MirOperand::Symbol(path), arguments } = &instruction.kind {
                if path.parts().last().is_some_and(|p| p.as_str() == "wasm_i32_types") {
                    found = true;
                    assert_eq!(arguments.len(), 1);
                    if let MirOperand::Value(arg_ref) = &arguments[0] {
                        assert_eq!(caller.value_types.get(arg_ref), Some(&ValkyrieType::Integer32 { signed: false }));
                    }
                }
            }
        }
    }
    assert!(found, "call missing");
}
