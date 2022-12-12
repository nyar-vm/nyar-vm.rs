use rusty_mojo::{RustyMojoFrontend, MojoLanguage};
use nyar_types::{NyarFrontend, Vfs};
use oak_vfs::MemoryVfs;
use oak_mojo::ast::{MojoStatement, MojoExpression, MojoLiteral};

#[test]
fn test_mojo_codegen() {
    let frontend = RustyMojoFrontend::new();
    let vfs = MemoryVfs::new();
    
    // Create a simple Mojo AST: x = 1 + 2
    let ast = vec![
        MojoStatement::Variable {
            name: "x".to_string(),
            ty: None,
            value: Some(MojoExpression::Binary {
                left: Box::new(MojoExpression::Literal(MojoLiteral::Int(1))),
                op: "+".to_string(),
                right: Box::new(MojoExpression::Literal(MojoLiteral::Int(2))),
            }),
            is_let: false,
        }
    ];
    
    let result = frontend.lower(&ast, &vfs);
    assert!(result.is_ok());
    let tree = result.unwrap();
    println!("Extracted Tree: {:?}", tree);
}

#[test]
fn test_mojo_control_flow() {
    let frontend = RustyMojoFrontend::new();
    let vfs = MemoryVfs::new();
    
    // if x > 0: return 1 else: return 0
    let ast = vec![
        MojoStatement::If {
            condition: MojoExpression::Binary {
                left: Box::new(MojoExpression::Identifier("x".to_string())),
                op: ">".to_string(),
                right: Box::new(MojoExpression::Literal(MojoLiteral::Int(0))),
            },
            then_body: vec![MojoStatement::Return(Some(MojoExpression::Literal(MojoLiteral::Int(1))))],
            else_body: Some(vec![MojoStatement::Return(Some(MojoExpression::Literal(MojoLiteral::Int(0))))]),
        }
    ];
    
    let result = frontend.lower(&ast, &vfs);
    assert!(result.is_ok());
}
