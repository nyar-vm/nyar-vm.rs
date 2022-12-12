use rusty_lua::RustyLuaFrontend;
use nyar_types::{NyarFrontend, NyarContext, EGraph};
use oak_vfs::MemoryVfs;
use std::path::Path;

#[test]
fn test_hello_lua() {
    let frontend = RustyLuaFrontend::new();
    let source = include_str!("hello.lua");
    let ast = frontend.parse(source).expect("Failed to parse Lua source");
    
    let mut egraph = EGraph::new();
    let vfs = MemoryVfs::new();
    let mut ctx = NyarContext::new(&mut egraph, &vfs, 0);
    
    let id = frontend.lower_unified(&ast, &mut ctx);
    assert!(id > 0);
    
    println!("Lowered ID: {:?}", id);
}
