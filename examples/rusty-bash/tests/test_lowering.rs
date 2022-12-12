use rusty_bash::RustyBashFrontend;
use nyar_types::{NyarContext, NyarFrontend};
use oak_vfs::MemoryVfs;

#[test]
fn test_lower_hello() {
    let frontend = RustyBashFrontend::new();
    let source = "echo hello";
    let ast = frontend.parse(source).expect("Failed to parse Bash");
    
    let mut ctx = NyarContext::new(MemoryVfs::new());
    let _id = frontend.lower_unified(&ast, &mut ctx);
    
    let dot = ctx.egraph.dot().to_string();
    println!("{}", dot);
}
