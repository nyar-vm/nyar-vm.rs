use rusty_dart::RustyDartFrontend;
use nyar_types::NyarFrontend;
use oak_vfs::MemoryVfs;

#[test]
fn test_basic_parsing() {
    let frontend = RustyDartFrontend::default();
    let source = r#"
class MyClass {}
void myFunc() {}
int myVar = 0;
int anotherFunc() {}
"#;
    let ast = frontend.parse(source).expect("Failed to parse basic Dart code");
    let vfs = MemoryVfs::new();
    let tree = frontend.lower(&ast, &vfs).expect("Failed to lower AST to UIR");
    
    let tree_str = format!("{:#?}", tree);
    println!("{}", tree_str);
    assert!(tree_str.contains("MyClass"));
    assert!(tree_str.contains("myFunc"));
    assert!(tree_str.contains("myVar"));
    assert!(tree_str.contains("anotherFunc"));
}
