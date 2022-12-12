use rusty_python::RustyPythonFrontend;
use nyar_types::NyarFrontend;

#[test]
fn test_exception_handling_lowering() {
    let source = r#"
try:
    raise Exception("error")
except ValueError as e:
    print(e)
except:
    print("generic error")
finally:
    print("cleanup")
"#;
    let frontend = RustyPythonFrontend::new();
    let ast = frontend.parse(source).expect("Failed to parse source");
    
    let mut egraph = chomsky_uir::EGraph::new();
    let vfs = oak_vfs::MemoryVfs::new();
    let mut ctx = nyar_types::NyarContext::new(&mut egraph, &vfs, 1);
    
    let root_id = frontend.lower_unified(&ast, &mut ctx);
    
    assert!(root_id != chomsky_uir::Id::from(0usize));
    println!("Successfully lowered AST with try-except-finally-raise");
}

#[test]
fn test_assert_lowering() {
    let source = r#"
assert x > 0, "x must be positive"
assert y == 10
"#;
    let frontend = RustyPythonFrontend::new();
    let ast = frontend.parse(source).expect("Failed to parse source");
    
    let mut egraph = chomsky_uir::EGraph::new();
    let vfs = oak_vfs::MemoryVfs::new();
    let mut ctx = nyar_types::NyarContext::new(&mut egraph, &vfs, 1);
    
    let root_id = frontend.lower_unified(&ast, &mut ctx);
    
    assert!(root_id != chomsky_uir::Id::from(0usize));
    println!("Successfully lowered AST with assert");
}

#[test]
fn test_with_statement_lowering() {
    let source = r#"
with open("test.txt") as f:
    data = f.read()

with a() as x, b() as y:
    pass
"#;
    let frontend = RustyPythonFrontend::new();
    let ast = frontend.parse(source).expect("Failed to parse source");
    
    let mut egraph = chomsky_uir::EGraph::new();
    let vfs = oak_vfs::MemoryVfs::new();
    let mut ctx = nyar_types::NyarContext::new(&mut egraph, &vfs, 1);
    
    let root_id = frontend.lower_unified(&ast, &mut ctx);
    
    assert!(root_id != chomsky_uir::Id::from(0usize));
    println!("Successfully lowered AST with with-statement");
}

#[test]
fn test_backend_lowering() {
    let source = r#"
try:
    assert x > 0
    with open("test.txt") as f:
        raise Exception("error")
except Exception as e:
    print(e)
finally:
    print("done")
"#;
    let frontend = RustyPythonFrontend::new();
    let ast = frontend.parse(source).expect("Failed to parse source");
    
    let vfs = oak_vfs::MemoryVfs::new();
    let ikun = frontend.lower(&ast, &vfs).expect("Failed to lower to IKunTree");
    
    use nyar_vm::bytecode::compiler::NyarBackend;
    let mut backend = NyarBackend::new();
    let _code = backend.lower_tree(&ikun).expect("Failed to lower to bytecode");
    let module = backend.finish();
    
    assert!(!module.chunks.is_empty());
    println!("Successfully lowered to Nyar bytecode ({} chunks)", module.chunks.len());
}
