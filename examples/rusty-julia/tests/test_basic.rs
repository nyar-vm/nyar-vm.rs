use rusty_julia::RustyJuliaFrontend;
use nyar_types::{EGraph, NyarContext, NyarFrontend};
use oak_vfs::MemoryVfs;
use chomsky_uir::ConstraintAnalysis;

#[test]
fn test_hello() {
    let source = include_str!("hello.jl");
    let frontend = RustyJuliaFrontend::new();
    let ast = frontend.parse(source).expect("Failed to parse");
    
    let mut egraph = EGraph::new();
    let vfs = MemoryVfs::default();
    let mut ctx = NyarContext::<MemoryVfs, ConstraintAnalysis>::new(&mut egraph, &vfs, 0);
    
    let id = frontend.lower_unified(&ast, &mut ctx);
    assert!(id > 0);
}

#[test]
fn test_loop_and_if() {
    let source = r#"
    sum = 0
    for i in 1:10
        if i > 5
            sum = sum + i
        end
    end
    println("Sum is: ", sum)
    "#;
    let frontend = RustyJuliaFrontend::new();
    let ast = frontend.parse(source).expect("Failed to parse");
    
    let mut egraph = EGraph::new();
    let vfs = MemoryVfs::default();
    let mut ctx = NyarContext::<MemoryVfs, ConstraintAnalysis>::new(&mut egraph, &vfs, 0);
    
    let id = frontend.lower_unified(&ast, &mut ctx);
    assert!(id > 0);
}
