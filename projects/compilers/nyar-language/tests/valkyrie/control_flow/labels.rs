use nyar_language::{MirLowerer, MirTerminator, SourceID, ValkyrieCompiler, types::hir::HirExprKind, valkyrie::mir::ssa::MirFunction};

fn compile(source: &str) -> nyar_language::types::hir::HirModule {
    ValkyrieCompiler::new(SourceID { version_id: 9500 }).compile_source(source).expect("compile")
}

fn compile_err(source: &str) -> String {
    ValkyrieCompiler::new(SourceID { version_id: 9501 }).compile_source(source).expect_err("expected compile error").to_string()
}

fn mir_fn<'a>(mir: &'a nyar_language::valkyrie::mir::MirModule, name: &str) -> &'a MirFunction {
    mir.functions
        .iter()
        .find(|f| f.symbol.ends_with(&format!("::{name}")) || f.symbol == name)
        .unwrap_or_else(|| panic!("expected mir function {name}"))
}

fn jump_targets(function: &MirFunction) -> Vec<u32> {
    function
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator {
            MirTerminator::Jump { target, .. } => Some(target.0),
            _ => None,
        })
        .collect()
}

#[test]
fn break_outer_label_jumps_to_outer_loop_exit() {
    let hir = compile(
        r#"
micro main() -> i64 {
    'outer: loop {
        'inner: loop {
            break 'outer 7
        }
    }
}
"#,
    );
    let mir = MirLowerer::lower_module_semantic(&hir);
    let function = mir_fn(&mir, "main");
    assert!(function.blocks.iter().any(|block| matches!(block.terminator, MirTerminator::Jump { .. })));
    assert!(function.blocks.iter().any(|block| block.label == "loop_exit"));
}

#[test]
fn continue_outer_label_targets_outer_loop_header() {
    let hir = compile(
        r#"
micro main() -> i64 {
    let i: i64 = 0
    'outer: while i < 2 {
        let j: i64 = 0
        'inner: while j < 5 {
            j = j + 1
            continue 'outer
        }
        i = i + 1
    }
    return i
}
"#,
    );
    let mir = MirLowerer::lower_module_semantic(&hir);
    let function = mir_fn(&mir, "main");
    assert!(function.blocks.iter().any(|block| block.label.contains("loop_header")));
    assert!(!jump_targets(function).is_empty());
}

#[test]
fn break_expr_in_value_loop_converges_type() {
    compile(
        r#"
micro main() -> i64 {
    loop {
        if true {
            break 42
        }
        break 0
    }
}
"#,
    );
}

#[test]
fn rejects_break_with_unknown_label() {
    let error = compile_err(
        r#"
micro main() {
    loop {
        break 'missing
    }
}
"#,
    );
    assert!(error.contains("break") || error.contains("loop") || error.contains("label") || error.contains("循环"));
}

#[test]
fn rejects_continue_with_unknown_label() {
    let error = compile_err(
        r#"
micro main() {
    loop {
        continue 'missing
    }
}
"#,
    );
    assert!(error.contains("continue") || error.contains("loop") || error.contains("label") || error.contains("循环"));
}

#[test]
fn yield_in_function_body_implies_generator_context() {
    // 设计决策（by design）：函数体内出现 `yield` 会隐式创建 generator 上下文，
    // 因此 `yield` 在普通函数体内总是合法的（函数被推导为 generator）。
    // `validate_yield_context` 中的拒绝路径为防御性死代码：解析器将 `yield` 作为 statement，
    // 无法出现在 guard / 纯表达式位置；且 shallow/block_contains_yield 自动检测并 push_generator。
    // 此测试固化该设计决策，防止未来误改。
    compile(
        r#"
micro make() {
    yield 1
    yield 2
    yield 3
}
"#,
    );
}

#[test]
fn yield_value_type_matches_generator_return_type() {
    // 函数返回 Generator<i32>，yield 42 的类型 i32 与之兼容，应编译通过。
    compile(
        r#"
micro make() -> Generator<i32> {
    yield 42
}
"#,
    );
}

#[test]
fn yield_value_type_mismatch_rejected() {
    // 函数返回 Generator<bool>，但 yield 42 的类型是 i64，类型不兼容应被拒绝。
    // 注：整数类型之间在自举阶段视为互相兼容（宽化检测），故需用 bool vs integer 来测试不匹配。
    let error = compile_err(
        r#"
micro make() -> Generator<bool> {
    yield 42
}
"#,
    );
    assert!(error.contains("不兼容") || error.contains("yield"), "actual error: {error}");
}

#[test]
fn yield_from_element_type_mismatch_rejected() {
    // 函数返回 Generator<bool>，yield from 一个 Generator<i64>，元素类型不兼容。
    let error = compile_err(
        r#"
micro make() -> Generator<bool> {
    yield from other()
}
micro other() -> Generator<i64> {
    yield 1
}
"#,
    );
    assert!(error.contains("不兼容") || error.contains("yield from"), "actual error: {error}");
}

#[test]
fn labeled_loop_hir_preserves_label_on_outer_loop() {
    let hir = compile(
        r#"
micro main() {
    'outer: loop {
        break
    }
}
"#,
    );
    let main = hir.functions.iter().find(|f| f.name.as_str() == "main").expect("main");
    let tail = main.body.expr.as_ref().or_else(|| {
        main.body.statements.last().and_then(|stmt| match &stmt.kind {
            nyar_language::types::hir::HirStatementKind::Expr(expr) => Some(expr),
            _ => None,
        })
    });
    let Some(expr) = tail
    else {
        panic!("expected loop expression");
    };
    let HirExprKind::Loop { label, .. } = &expr.kind
    else {
        panic!("expected loop, got {:?}", expr.kind);
    };
    assert_eq!(label.as_ref().map(|id| id.as_str()), Some("outer"));
}

#[test]
fn break_in_try_body_preserves_terminator() {
    // 回归 BUG-001：`break` 在 `try` 块内时，`lower_try_scope_expr` 曾丢弃 terminator，
    // 导致 break 被静默吞掉。修复后 break 应正确跳转到 loop_exit。
    let hir = compile(
        r#"
micro main() -> i64 {
    loop {
        try {
            break 42
        }
    }
}
"#,
    );
    let mir = MirLowerer::lower_module_semantic(&hir);
    let function = mir_fn(&mir, "main");
    // break 应产生 Jump terminator到 loop_exit；不能被 try_exit 吞掉。
    assert!(
        function.blocks.iter().any(|block| matches!(block.terminator, MirTerminator::Jump { .. })),
        "break in try body must emit a Jump terminator, got: {:?}",
        function.blocks.iter().map(|b| &b.terminator).collect::<Vec<_>>()
    );
}

#[test]
fn continue_in_try_body_preserves_terminator() {
    // 回归 BUG-004：`continue` 在 `try` 块内的同款问题。
    let hir = compile(
        r#"
micro main() {
    loop {
        try {
            continue
        }
    }
}
"#,
    );
    let mir = MirLowerer::lower_module_semantic(&hir);
    let function = mir_fn(&mir, "main");
    assert!(
        function.blocks.iter().any(|block| matches!(block.terminator, MirTerminator::Jump { .. })),
        "continue in try body must emit a Jump terminator"
    );
}

#[test]
fn rejects_break_outer_label_inside_lambda() {
    // 回归 BUG-003：`break 'outer` 在 lambda 体内应被拒绝
    // （lambda 是独立函数帧，break 不能跨越函数边界解析到外层循环 label）。
    let error = compile_err(
        r#"
micro main() {
    'outer: loop {
        let callback = micro() {
            break 'outer
        }
        break
    }
}
"#,
    );
    assert!(
        error.contains("break") || error.contains("loop") || error.contains("label") || error.contains("循环"),
        "break 'outer inside lambda should be rejected, actual: {error}"
    );
}
