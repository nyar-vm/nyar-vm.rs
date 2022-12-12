use nyar_language::{MirLowerer, ValkyrieCompiler, types::SourceID};

fn main() {
    let src = r#"
micro apply(pred: micro(i32) -> bool, x: i32): bool {
    pred(x)
}

micro main(): bool {
    apply(micro(v: i32) -> bool { v > 0 }, 1)
}
"#;
    let hir = ValkyrieCompiler::new(SourceID { version_id: 1 }).compile_source(src).expect("hir");
    let mir = MirLowerer::lower_module_semantic(&hir);
    for f in &mir.functions {
        println!("=== {} ret={:?} params={:?} ===", f.symbol, f.return_type, f.param_types);
        for (k, v) in &f.value_types {
            println!("  v{:?} : {:?}", k, v);
        }
        for b in &f.blocks {
            println!("  block {} params={:?}", b.label, b.parameters);
            for i in &b.instructions {
                println!("    results={:?} {:?}", i.results, i.kind);
            }
            println!("    term={:?}", b.terminator);
        }
    }
}
