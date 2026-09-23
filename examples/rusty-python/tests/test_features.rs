use rusty_python::PythonFrontend;
use nyar_types::NyarFrontend;
use oak_vfs::MemoryVfs;

#[test]
fn test_import_support() {
    let frontend = PythonFrontend::default();
    let source = r#"
import math
from os import path as ospath
"#;
    let ast = frontend.parse(source).expect("Failed to parse import statements");
    let vfs = MemoryVfs::new();
    let tree = frontend.lower(&ast, &vfs).expect("Failed to lower AST to UIR");
    
    // Check if the tree contains the expected extensions
    let tree_str = format!("{:?}", tree);
    assert!(tree_str.contains("import"));
    assert!(tree_str.contains("import_from"));
    assert!(tree_str.contains("math"));
    assert!(tree_str.contains("os"));
    assert!(tree_str.contains("path"));
}

#[test]
fn test_scope_control() {
    let frontend = PythonFrontend::default();
    let source = r#"
x = 1
def foo():
    global x
    x = 2

def bar():
    y = 1
    def inner():
        nonlocal y
        y = 2
"#;
    let ast = frontend.parse(source).expect("Failed to parse scope control statements");
    let vfs = MemoryVfs::new();
    let tree = frontend.lower(&ast, &vfs).expect("Failed to lower AST to UIR");
    
    // Check if the tree contains variable assignments
    let tree_str = format!("{:?}", tree);
    assert!(tree_str.contains("x"));
    assert!(tree_str.contains("y"));
}
