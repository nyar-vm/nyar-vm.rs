fn main() {
    use nyar_language::{ValkyrieCompiler, types::SourceID};
    let hir = ValkyrieCompiler::new(SourceID { version_id: 99002 })
        .compile_source(
            r#"
unite Kind { A B C }
micro main(k: Kind) -> i32 {
    match k {
        case A: return 1
        case B: return 2
        else: return 0
    }
}
"#,
        )
        .expect("compile");
    let f = hir.functions.iter().find(|f| f.name.as_str().contains("main") || f.name.as_str() == "main").unwrap_or(&hir.functions[0]);
    println!("fn {}", f.name);
    // print match arms patterns via debug of body
    println!("{:#?}", f.body);
}
