use rusty_haskell::codegen::{NyarTranslator, TranslatorContext};
use chomsky_uir::{ConstraintAnalysis, EGraph, IKun};
use rusty_haskell::RustyHaskellFrontend;
use nyar_types::NyarFrontend;
use oak_haskell::ast::*;

#[test]
fn test_parse_and_translate() {
    let mut items = Vec::new();
    items.push(Item::Function(Function {
        name: Identifier { name: "x".to_string(), span: (0..1).into() },
        type_signature: None,
        equations: vec![Equation {
            patterns: vec![],
            body: Expression::Literal(Literal::Integer(42)),
        }],
        span: (0..1).into(),
    }));
    let ast = HaskellRoot {
        module_name: Some(Identifier { name: "TestModule".to_string(), span: (0..1).into() }),
        items,
    };
    
    let translator = NyarTranslator::new();
    let mut egraph = EGraph::<IKun, ConstraintAnalysis>::new();
    let result = translator.translate_to_graph(&ast, &mut egraph);
    assert!(result.is_ok());
}

#[test]
fn test_function_definition() {
    let mut equations = Vec::new();
    equations.push(Equation {
        patterns: vec![
            Pattern::Variable(Identifier { name: "x".to_string(), span: (0..1).into() }),
            Pattern::Variable(Identifier { name: "y".to_string(), span: (0..1).into() }),
        ],
        body: Expression::Literal(Literal::Integer(42)),
    });
    
    let f = Function {
        name: Identifier { name: "add".to_string(), span: (0..1).into() },
        type_signature: None,
        equations,
        span: (0..1).into(),
    };
    
    let translator = NyarTranslator::new();
    let mut egraph = EGraph::<IKun, ConstraintAnalysis>::new();
    let mut ctx = TranslatorContext::new(&mut egraph);
    let result = translator.translate_function(&f, &mut ctx);
    assert!(result.is_ok());
}

#[test]
fn test_lambda_expression() {
    let expr = Expression::Lambda(
        vec![Pattern::Variable(Identifier { name: "x".to_string(), span: (0..1).into() })],
        Box::new(Expression::Variable(Identifier { name: "x".to_string(), span: (0..1).into() }))
    );
    
    let translator = NyarTranslator::new();
    let mut egraph = EGraph::<IKun, ConstraintAnalysis>::new();
    let mut ctx = TranslatorContext::new(&mut egraph);
    let result = translator.translate_expression(&expr, &mut ctx);
    assert!(result.is_ok());
}
