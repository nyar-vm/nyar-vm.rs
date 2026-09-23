use rusty_python::RustyPythonFrontend;
use nyar_types::NyarFrontend;

#[test]
fn test_function_def_with_defaults_lowering() {
    let source = r#"
def foo(a, b=1, c=2):
    return a + b + c
"#;
    let frontend = RustyPythonFrontend::new();
    let ast = frontend.parse(source).expect("Failed to parse source");
    
    let mut egraph = chomsky_uir::EGraph::new();
    let vfs = oak_vfs::MemoryVfs::new();
    let mut ctx = nyar_types::NyarContext::new(&mut egraph, &vfs, 1);
    
    let root_id = frontend.lower_unified(&ast, &mut ctx);
    
    assert!(root_id != chomsky_uir::Id::from(0usize));
    println!("Successfully lowered function definition with defaults");
}

#[test]
fn test_function_def_with_varargs_lowering() {
    let source = r#"
def bar(a, *args, **kwargs):
    pass
"#;
    let frontend = RustyPythonFrontend::new();
    let ast = frontend.parse(source).expect("Failed to parse source");
    
    let mut egraph = chomsky_uir::EGraph::new();
    let vfs = oak_vfs::MemoryVfs::new();
    let mut ctx = nyar_types::NyarContext::new(&mut egraph, &vfs, 1);
    
    let root_id = frontend.lower_unified(&ast, &mut ctx);
    
    assert!(root_id != chomsky_uir::Id::from(0usize));
    println!("Successfully lowered function definition with *args and **kwargs");
}

#[test]
fn test_function_call_with_keywords_lowering() {
    let source = r#"
foo(1, b=2, c=3)
"#;
    let frontend = RustyPythonFrontend::new();
    let ast = frontend.parse(source).expect("Failed to parse source");
    
    let mut egraph = chomsky_uir::EGraph::new();
    let vfs = oak_vfs::MemoryVfs::new();
    let mut ctx = nyar_types::NyarContext::new(&mut egraph, &vfs, 1);
    
    let root_id = frontend.lower_unified(&ast, &mut ctx);
    
    assert!(root_id != chomsky_uir::Id::from(0usize));
    println!("Successfully lowered function call with keywords");
}

#[test]
fn test_function_call_with_starred_lowering() {
    let source = r#"
foo(*[1, 2], **{'c': 3})
"#;
    let frontend = RustyPythonFrontend::new();
    let ast = frontend.parse(source).expect("Failed to parse source");
    
    let mut egraph = chomsky_uir::EGraph::new();
    let vfs = oak_vfs::MemoryVfs::new();
    let mut ctx = nyar_types::NyarContext::new(&mut egraph, &vfs, 1);
    
    let root_id = frontend.lower_unified(&ast, &mut ctx);
    
    assert!(root_id != chomsky_uir::Id::from(0usize));
    println!("Successfully lowered function call with *args and **kwargs");
}
