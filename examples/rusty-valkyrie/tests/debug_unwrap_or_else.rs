use rusty_valkyrie::{
    ValkyrieCompiler,
    types::hir::{HirExprKind, HirStatementKind},
};

#[test]
fn debug_unwrap_or_else_f_call() {
    let source = r#"
namespace core::types;
unite Option<T> { Some { value: T }, None }
imply Option<T> {
    micro unwrap_or_else(self, f: micro() -> T): T {
        match self {
            case Some(value):
                value
            case None:
                f()
        }
    }
}
"#;
    let root = rusty_valkyrie::frontend::parse_source(source).expect("parse");
    let impl_method = root
        .items
        .iter()
        .filter_map(|item| match item {
            rusty_valkyrie::frontend::ast::StatementNode::Imply(imply) => Some(imply),
            _ => None,
        })
        .flat_map(|imply| &imply.methods)
        .find(|method| method.name.name == "unwrap_or_else")
        .expect("ast method");
    eprintln!("ast f param ty: {:?}", impl_method.parameters[1].ty);
    let compiler = ValkyrieCompiler::default();
    ValkyrieCompiler::default().compile_source(source).expect("unwrap_or_else must compile without SMIR003");
    let hir = compiler.lower_root_with_semantic_exports_and_name(&root, &[], None).expect("lower");
    let method = hir.impls.iter().flat_map(|impl_| &impl_.methods).find(|m| m.name.as_str() == "unwrap_or_else").expect("method");
    eprintln!("f param type: {:?}", method.params[1].ty);
    let mut calls = Vec::new();
    fn walk(expr: &rusty_valkyrie::types::hir::HirExpr, out: &mut Vec<String>) {
        match &expr.kind {
            HirExprKind::Call { callee, resolved, .. } => {
                let name = match &callee.kind {
                    HirExprKind::Variable(id) => id.name.to_string(),
                    HirExprKind::Path(p) => p.to_string(),
                    _ => format!("{:?}", callee.kind),
                };
                let status = resolved.as_ref().map(|c| format!("resolved {:?}", c.domain)).unwrap_or_else(|| "unresolved".to_string());
                out.push(format!("{name} => {status}"));
            }
            HirExprKind::Match { arms, .. } => {
                for arm in arms {
                    walk(&arm.body, out);
                }
            }
            _ => {}
        }
    }
    for stmt in &method.body.statements {
        if let HirStatementKind::Expr(expr) = &stmt.kind {
            walk(expr, &mut calls);
        }
    }
    eprintln!("calls: {:?}", calls);
}
