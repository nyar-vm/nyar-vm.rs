use rusty_valkyrie::{
    types::{
        SourceID,
        hir::{HirExpr, HirExprKind},
    },
    {
        frontend::{self, ast::Statement},
        hir::lowering::lower_term_expression,
    },
};

fn lower(source: &str) -> HirExpr {
    let root = frontend::parse_source(source).expect("完整 AST");
    let frontend::ast::StatementNode::Micro(function) = &root.items[0]
    else {
        panic!("缺少函数")
    };
    let Statement::Expression(statement) = &function.body.statements[0]
    else {
        panic!("缺少表达式")
    };
    lower_term_expression(&statement.expr, SourceID::default(), 0..source.len()).expect("表达式 lowering")
}

#[test]
fn assignment_and_field_write_keep_structured_targets() {
    assert!(matches!(lower("micro update() { value = 7 }").kind, HirExprKind::Assign { .. }));
    assert!(matches!(lower("micro update() { object.field = 7 }").kind, HirExprKind::StoreField { .. }));
}

#[test]
fn iteration_source_is_not_a_boolean_condition() {
    assert!(matches!(
        lower("micro visit() { loop item in values { consume(item) } }").kind,
        HirExprKind::Loop { pattern: Some(_), iterator: Some(_), condition: None, .. }
    ));
    assert!(matches!(
        lower("micro visit() { while ready { consume() } }").kind,
        HirExprKind::Loop { pattern: None, iterator: None, condition: Some(_), .. }
    ));
}

/// 成员调用的 callee 名字（HIR 只保留接收者成员名，不保留运算符语义类别）。
fn callee_member(callee: &HirExpr) -> String {
    let HirExprKind::Path(path) = &callee.kind
    else {
        panic!("成员调用的 callee 必须是名字路径")
    };
    path.to_string()
}

#[test]
fn bitwise_sugar_lowers_to_plain_member_calls() {
    for (source, member) in [
        ("micro bits() { left | right }", "infix |"),
        ("micro bits() { left & right }", "infix &"),
        ("micro bits() { left << right }", "infix <<"),
        ("micro bits() { left >> right }", "infix >>"),
    ] {
        let HirExprKind::Call { callee, args, resolved } = lower(source).kind
        else {
            panic!("按位表达式必须降低为普通成员调用")
        };
        assert!(resolved.is_none(), "语法糖展开阶段不得伪造已解析 callable 身份");
        assert_eq!(callee_member(&callee), member);
        assert_eq!(args.len(), 2);
    }
}

#[test]
fn subscript_sugar_lowers_to_plain_member_calls() {
    for (source, member, arity) in [
        ("micro read() { values[1] }", "suffix []", 2),
        ("micro read() { values⁅0⁆ }", "suffix ⁅⁆", 2),
        ("micro read() { values::[0] }", "suffix ⁅⁆", 2),
        ("micro write() { values[1] = 7 }", "suffix []=", 3),
        ("micro write() { values⁅0⁆ = 7 }", "suffix ⁅⁆=", 3),
    ] {
        let HirExprKind::Call { callee, args, resolved } = lower(source).kind
        else {
            panic!("下标表达式必须降低为普通成员调用")
        };
        assert!(resolved.is_none(), "语法糖展开阶段不得伪造已解析 callable 身份");
        assert_eq!(callee_member(&callee), member);
        assert_eq!(args.len(), arity);
    }
}
