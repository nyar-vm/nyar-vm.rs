use oak_core::{Builder, ParseSession, SourceText};
use oak_valkyrie::{ValkyrieBuilder, ValkyrieLanguage};

fn main() {
    let src = "micro run(value: i32) -> i32 { let result: i32 = Alpha.answer(value); return result }";
    let language = ValkyrieLanguage::default();
    let builder = ValkyrieBuilder::new(&language);
    let out = builder.build(&SourceText::new(src), &[], &mut ParseSession::default());
    let root = out.result.expect("parse");
    let oak_valkyrie::ast::StatementNode::Micro(micro) = &root.items[0]
    else {
        panic!()
    };
    let oak_valkyrie::ast::Statement::Let(let_stmt) = &micro.body.statements[0]
    else {
        panic!()
    };
    println!("{:#?}", let_stmt.expr);
}
