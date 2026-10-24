use rusty_valkyrie::{
    ValkyrieCompiler,
    types::hir::{HirExprKind, HirStatementKind},
};

fn compile(source: &str) -> Result<(), String> {
    ValkyrieCompiler::default().compile_source(source).map(|_| ()).map_err(|e| format!("{e:?}"))
}

const REAL_COLLECT: &str = r#"
namespace std.iterator;
unite Option<T> { Some { value: T }, None }
imply Option<T> {
    micro unwrap(self) -> T { match self { case Some(value): value case None: return None } }
}
trait Iterator { type Item; micro has_next(self) -> bool; micro next(self) -> Option<Item>; }
micro collect_array<I, T>(self: I) -> [T]
    where I: Iterator<Item = T>
{
    let mut result: [T] = []
    let mut iter: I = self
    while iter.has_next() {
        result = push(result, iter.next().unwrap())
    }
    return result
}
"#;

fn walk_calls(expr: &rusty_valkyrie::types::hir::HirExpr, out: &mut Vec<String>) {
    match &expr.kind {
        HirExprKind::Call { callee, args, resolved, .. } => {
            for arg in args {
                walk_calls(&arg.value, out);
            }
            let name = match &callee.kind {
                HirExprKind::FieldAccess { field, .. } => field.to_string(),
                HirExprKind::Path(p) => p.to_string(),
                HirExprKind::Variable(v) => v.name.to_string(),
                _ => "<computed>".into(),
            };
            out.push(format!(
                "{name} => {}",
                resolved.as_ref().map(|c| format!("resolved return={:?}", c.return_type)).unwrap_or_else(|| "unresolved".into())
            ));
            if let HirExprKind::FieldAccess { object, .. } = &callee.kind {
                walk_calls(object, out);
            }
        }
        HirExprKind::Loop { body, .. } => {
            for stmt in &body.statements {
                if let HirStatementKind::Expr(expr) = &stmt.kind {
                    walk_calls(expr, out);
                }
            }
            if let Some(expr) = &body.expr {
                walk_calls(expr, out);
            }
        }
        HirExprKind::Block(block) => {
            for stmt in &block.statements {
                if let HirStatementKind::Expr(expr) = &stmt.kind {
                    walk_calls(expr, out);
                }
            }
            if let Some(expr) = &block.expr {
                walk_calls(expr, out);
            }
        }
        HirExprKind::Assign { value, .. } => {
            out.push(format!("assign value kind={:?}", std::mem::discriminant(&value.kind)));
            walk_calls(value, out);
        }
        HirExprKind::Return(Some(value)) => walk_calls(value, out),
        _ => {}
    }
}

#[test]
fn diagnose_collect_array_call_chain() {
    let compiler = ValkyrieCompiler::default();
    let root = rusty_valkyrie::frontend::parse_source(REAL_COLLECT).expect("parse");
    let hir = compiler.lower_root_with_semantic_exports_and_name(&root, &[], None).expect("lower");
    let func = hir.functions.iter().find(|f| f.name.as_str() == "collect_array").expect("fn");
    eprintln!("stmts: {}", func.body.statements.len());
    for (i, stmt) in func.body.statements.iter().enumerate() {
        eprintln!("stmt {i}: {:?}", std::mem::discriminant(&stmt.kind));
    }
    let mut calls = Vec::new();
    for stmt in &func.body.statements {
        if let HirStatementKind::Expr(expr) = &stmt.kind {
            walk_calls(expr, &mut calls);
        }
    }
    if let Some(expr) = &func.body.expr {
        walk_calls(expr, &mut calls);
    }
    eprintln!("calls: {calls:?}");
}

#[test]
#[ignore = "contract-gate: collect_array push/unwrap closure pending SMIR003 resolution"]
fn collect_array_with_option_unwrap_resolves() {
    compile(REAL_COLLECT).expect("real collect_array shape must resolve");
}
