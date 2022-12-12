use rusty_fsharp::RustyFSharpFrontend;
use nyar_types::NyarFrontend;

#[test]
fn test_parse_and_translate() {
    let frontend = RustyFSharpFrontend::default();
    let source = r#"
namespace TestNamespace
module TestModule
open System
let x = 42
"#;
    
    let ast = frontend.parse(source).expect("Failed to parse");
    assert_eq!(ast.items.len(), 4);
}

#[test]
fn test_function_definition() {
    let frontend = RustyFSharpFrontend::default();
    let source = r#"
let add x y = 42
"#;
    
    let ast = frontend.parse(source).expect("Failed to parse");
    assert_eq!(ast.items.len(), 1);
    if let oak_fsharp::ast::Item::Binding(b) = &ast.items[0] {
        assert_eq!(b.name, "add");
        assert_eq!(b.parameters, vec!["x", "y"]);
        assert_eq!(b.expression, oak_fsharp::ast::Expression::Simple("42".to_string()));
    } else {
        panic!("Expected binding");
    }
}

#[test]
fn test_if_expression() {
    let frontend = RustyFSharpFrontend::default();
    let source = r#"
let check x = if x then 1 else 0
"#;
    
    let ast = frontend.parse(source).expect("Failed to parse");
    assert_eq!(ast.items.len(), 1);
    if let oak_fsharp::ast::Item::Binding(b) = &ast.items[0] {
        assert_eq!(b.name, "check");
        assert_eq!(b.parameters, vec!["x"]);
        match &b.expression {
            oak_fsharp::ast::Expression::If { .. } => {},
            _ => panic!("Expected if expression, got {:?}", b.expression),
        }
    } else {
        panic!("Expected binding");
    }
}
