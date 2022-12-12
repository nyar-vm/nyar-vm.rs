use rusty_tcl::RustyTclFrontend;
use nyar_types::{NyarContext, NyarFrontend};
use oak_vfs::MemoryVfs;

#[test]
fn test_lower_hello() {
    let frontend = RustyTclFrontend::new();
    let source = "set a 10; puts $a";
    let ast = frontend.parse(source).expect("Failed to parse TCL");
    
    let mut ctx = NyarContext::new(MemoryVfs::new());
    let _id = frontend.lower_unified(&ast, &mut ctx);
    
    // Check if the lowered graph contains what we expect
    // For now, just ensure it doesn't panic and produces something.
    let dot = ctx.egraph.dot().to_string();
    println!("{}", dot);
}
