use rusty_python::RustyPythonFrontend;
use nyar_types::NyarFrontend;

#[test]
fn test_new_features_lowering() {
    let source = r#"
import math
from os import path

x = 10

def test_global():
    global x
    x = 20

def test_nonlocal():
    y = 30
    def inner():
        nonlocal y
        y = 40
    inner()
    return y

test_global()
res = test_nonlocal()
"#;
    let frontend = RustyPythonFrontend::new();
    let ast = frontend.parse(source).expect("Failed to parse source");
    
    let mut egraph = chomsky_uir::EGraph::new();
    let vfs = oak_vfs::MemoryVfs::new();
    let mut ctx = nyar_types::NyarContext::new(&mut egraph, &vfs, 1);
    
    let root_id = frontend.lower_unified(&ast, &mut ctx);
    
    assert!(root_id != chomsky_uir::Id::from(0usize));
    println!("Successfully lowered AST with import, global and nonlocal");
}
