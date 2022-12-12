fn main() {
    use nyar_language::{MirConstant, MirLowerer, MirOperand, MirOperation, ValkyrieCompiler, types::SourceID};
    let hir = ValkyrieCompiler::new(SourceID { version_id: 99001 })
        .compile_source(
            r#"
unite Kind { A B C }
micro main(k: Kind) -> i32 {
    match k {
        case A: return 1
        case B: return 2
        case C: return 3
        else: return 0
    }
}
"#,
        )
        .expect("compile");
    let mir = MirLowerer::lower_module_semantic(&hir);
    let f = mir.functions.iter().find(|f| f.symbol.contains("main")).expect("main");
    for (bi, b) in f.blocks.iter().enumerate() {
        println!("block {} {}:", bi, b.label);
        for ins in &b.instructions {
            println!("  {:?}", ins);
        }
        println!("  term: {:?}", b.terminator);
    }
}
