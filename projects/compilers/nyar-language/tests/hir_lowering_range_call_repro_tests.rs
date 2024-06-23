use nyar_language::{
    valkyrie::{
        frontend::ParseError,
        hir::{lowering::DEFAULT_COMPILE_ARCH, *},
        types::hir::*,
    },
    *,
};

// 对齐 `valkyrie.v` core：`Range`/`TextSpan` 字段为 `usize`，跨 namespace 构造须冻结 Nominal。
// 禁止用 `i32` 规避或只 `eprintln` 打印 HIR 冒充通过。

fn compile(source: &str) -> Result<HirModule, ParseError> {
    ValkyrieCompiler::new(SourceID::default()).compile_source(source)
}

fn find_struct<'a>(hir: &'a HirModule, name: &str) -> &'a HirStruct {
    hir.structs
        .iter()
        .find(|item| item.name.as_str() == name)
        .or_else(|| hir.submodules.iter().flat_map(|module| &module.structs).find(|item| item.name.as_str() == name))
        .unwrap_or_else(|| panic!("缺少结构 `{name}`"))
}

fn assert_nominal_return(hir: &HirModule, function_name: &str, owner_name: &str) {
    let owner = find_struct(hir, owner_name);
    let declaration = owner.constructor_declaration.expect("结构须已分配 constructor_declaration");
    let function = hir
        .functions
        .iter()
        .chain(hir.impls.iter().flat_map(|item| item.methods.iter()))
        .find(|item| item.name.as_str() == function_name)
        .unwrap_or_else(|| panic!("缺少函数 `{function_name}`"));
    assert_eq!(
        function.return_type,
        ValkyrieType::Nominal { declaration, arguments: Vec::new() },
        "`{function_name}` 返回类型须是 Nominal{{ItemId}}，不得残留 Named/限定名字符串"
    );
}

fn assert_resolved_range_construct(hir: &HirModule, function_name: &str) {
    let function = hir
        .functions
        .iter()
        .chain(hir.impls.iter().flat_map(|item| item.methods.iter()))
        .find(|item| item.name.as_str() == function_name)
        .unwrap_or_else(|| panic!("缺少函数 `{function_name}`"));
    let return_expr = match function.body.statements.first().map(|statement| &statement.kind) {
        Some(HirStatementKind::Expr(statement)) => statement.as_ref(),
        _ => function.body.expr.as_deref().expect("函数体须有 return"),
    };
    let HirExprKind::Return(Some(expression)) = &return_expr.kind
    else {
        panic!("期望 return 表达式");
    };
    match &expression.kind {
        HirExprKind::Call { resolved: Some(contract), .. } | HirExprKind::Construct { resolved: Some(contract), .. } => {
            assert!(contract.symbol.to_string().contains("Range"), "构造合同符号须指向 Range，got {}", contract.symbol);
            assert!(matches!(contract.return_type, ValkyrieType::Nominal { .. }), "Range 构造返回须是 Nominal，got {:?}", contract.return_type);
        }
        other => panic!("期望已解析的 Range 构造/调用，got {other:?}"),
    }
}

#[test]
fn range_usize_literals_bind_nominal_and_resolve() {
    let source = r#"
structure Range {
    start: usize,
    end: usize,
}
micro make() -> Range {
    return Range(1, 2)
}
"#;
    let hir = compile(source).unwrap_or_else(|error| panic!("usize Range 构造必须通过 Compiler contract: {error}"));
    assert_nominal_return(&hir, "make", "Range");
    assert_resolved_range_construct(&hir, "make");
}

#[test]
fn textspan_as_range_same_namespace_binds_nominal() {
    // 先验收字段投影入参（无 `+`）；加法是独立 overload 合同。
    let source = r#"
structure Range {
    start: usize,
    end: usize,
}
structure TextSpan {
    offset: usize,
    length: usize
}
imply TextSpan {
    micro as_range(self) -> Range {
        return Range(self.offset, self.length)
    }
}
"#;
    let hir = compile(source).unwrap_or_else(|error| panic!("同 namespace TextSpan.as_range 必须通过: {error}"));
    assert_nominal_return(&hir, "as_range", "Range");
    assert_resolved_range_construct(&hir, "as_range");
}

#[test]
fn textspan_as_range_with_usize_add_binds_nominal() {
    // 对齐生产 core：`structure usize` + `infix +` + `Range(self.offset, self.offset + self.length)`。
    // 禁止省略 usize 运算符声明后仍期望 `+` 魔法通过。
    let source = r#"
[primitive("core::primitive::usize")]
structure usize { }

imply usize {
    infix `+`(self, rhs: Self): Self {
        return self
    }
}

structure Range {
    start: usize,
    end: usize,
}
structure TextSpan {
    offset: usize,
    length: usize
}
imply TextSpan {
    micro as_range(self) -> Range {
        return Range(self.offset, self.offset + self.length)
    }
}
"#;
    let hir = compile(source).unwrap_or_else(|error| panic!("usize 加法 + Range 构造必须通过: {error}"));
    assert_nominal_return(&hir, "as_range", "Range");
    assert_resolved_range_construct(&hir, "as_range");
    let usize_decl = find_struct(&hir, "usize").constructor_declaration.expect("usize declaration");
    let add = hir.impls.iter().flat_map(|item| item.methods.iter()).find(|method| method.name.as_str() == "infix +").expect("infix +");
    assert!(
        matches!(add.params[0].ty, ValkyrieType::Nominal { declaration, .. } if declaration == usize_decl)
            || matches!(
                &hir.impls.iter().find(|item| matches!(&item.target, ValkyrieType::Nominal { declaration, .. } if *declaration == usize_decl)),
                Some(_)
            ),
        "usize imply / Self 须冻结为 Nominal{{ItemId}}，不得残留 Named(\"usize\")"
    );
}

#[test]
fn textspan_as_range_cross_namespace_binds_nominal() {
    let source = r#"
[primitive("core::primitive::usize")]
structure usize { }

imply usize {
    infix `+`(self, rhs: Self): Self {
        return self
    }
}

namespace core.types
structure Range {
    start: usize,
    end: usize,
}
namespace core.text
structure TextSpan {
    offset: usize,
    length: usize
}
imply TextSpan {
    micro as_range(self) -> Range {
        return Range(self.offset, self.offset + self.length)
    }
}
"#;
    let hir = compile(source).unwrap_or_else(|error| panic!("跨 namespace TextSpan.as_range 必须通过: {error}"));
    assert_nominal_return(&hir, "as_range", "Range");
    assert_resolved_range_construct(&hir, "as_range");
}

#[test]
fn imply_option_unwrap_resolves_instance_method_contract() {
    let source = r#"
unite Option<T> { Some { value: T }, None }
imply Option<T> {
    micro unwrap(self) -> T { match self { case Some(value): value case None: return None } }
}
micro test(x: Option<i64>) -> i64 { return x.unwrap() }
"#;
    compile(source).expect("imply Option::unwrap 必须通过调用合同");
}

#[test]
fn option_or_else_some_construct_binds_nominal_without_short_name() {
    // 对齐生产 `core/source/types/Option.v`：`unite` 变体 + 自由函数 `Some` 同名并存时，
    // `or_else` 臂内 `Some(value)` 必须冻结 Constructor 合同，不得 SMIR003。
    let source = r#"
namespace core::types;
unite Option<T> {
    Some { value: T },
    None,
}
micro Some<T>(value: T) -> Option<T> {
    let wrapped: Option<T> = Some(value)
    return wrapped
}
imply Option<T> {
    micro or_else(self, f: micro() -> Option<T>): Option<T> {
        match self {
            case Some(value):
                Some(value)
            case None:
                f()
        }
    }
}
"#;
    let hir = compile(source).unwrap_or_else(|error| panic!("Option.or_else 中 Some(value) 必须通过 Compiler contract: {error}"));
    let option = hir
        .enums
        .iter()
        .find(|item| item.name.as_str() == "Option")
        .or_else(|| hir.submodules.iter().flat_map(|module| &module.enums).find(|item| item.name.as_str() == "Option"))
        .expect("Option");
    let declaration = option.declaration.expect("Option declaration");
    let or_else = hir.impls.iter().flat_map(|item| item.methods.iter()).find(|item| item.name.as_str() == "or_else").expect("or_else");
    assert!(
        matches!(&or_else.return_type, ValkyrieType::Nominal { declaration: id, .. } if *id == declaration),
        "or_else 返回须是 Nominal{{Option ItemId}}，got {:?}",
        or_else.return_type
    );
    let mut saw_resolved_some = false;
    fn walk_expr(expr: &nyar_language::types::hir::HirExpr, saw: &mut bool) {
        match &expr.kind {
            HirExprKind::Call { resolved, args, .. } => {
                if let Some(contract) = resolved {
                    if contract.symbol.to_string().contains("Some") {
                        *saw = true;
                        assert!(
                            matches!(contract.return_type, ValkyrieType::Nominal { .. }),
                            "Some 构造返回须是 Nominal，got {:?}",
                            contract.return_type
                        );
                        assert!(contract.sum_owner.is_some() && contract.sum_variant.is_some(), "Some 须携带 sum_owner/sum_variant");
                    }
                }
                for arg in args {
                    walk_expr(&arg.value, saw);
                }
            }
            HirExprKind::Construct { resolved, args, .. } => {
                if let Some(contract) = resolved {
                    if contract.symbol.to_string().contains("Some") {
                        *saw = true;
                        assert!(
                            matches!(contract.return_type, ValkyrieType::Nominal { .. }),
                            "Some 构造返回须是 Nominal，got {:?}",
                            contract.return_type
                        );
                        assert!(contract.sum_owner.is_some() && contract.sum_variant.is_some(), "Some 须携带 sum_owner/sum_variant");
                    }
                }
                for arg in args {
                    walk_expr(arg, saw);
                }
            }
            HirExprKind::Match { scrutinee, arms, .. } => {
                walk_expr(scrutinee, saw);
                for arm in arms {
                    walk_expr(&arm.body, saw);
                }
            }
            HirExprKind::Return(Some(inner)) => walk_expr(inner, saw),
            HirExprKind::Block(block) => {
                for statement in &block.statements {
                    if let HirStatementKind::Expr(inner) = &statement.kind {
                        walk_expr(inner, saw);
                    }
                }
                if let Some(inner) = &block.expr {
                    walk_expr(inner, saw);
                }
            }
            _ => {}
        }
    }
    for statement in &or_else.body.statements {
        if let HirStatementKind::Expr(expr) = &statement.kind {
            walk_expr(expr, &mut saw_resolved_some);
        }
    }
    if let Some(expr) = &or_else.body.expr {
        walk_expr(expr, &mut saw_resolved_some);
    }
    assert!(saw_resolved_some, "or_else 臂内须有已解析的 Some 构造合同");
}

#[test]
fn associated_type_named_base_binds_to_nominal_declaration() {
    // `Carrier::Payload` 降低为 Associated{base: Named}；bind 须冻结 base 为 Nominal{ItemId}。
    // 不返回该 Associated，避免控制流把 Associated 与结构 Nominal 认亲。
    let source = r#"
structure Payload {
    value: i64,
}
structure Carrier {
}
micro inspect(value: Carrier::Payload): unit {
}
"#;
    let hir = compile(source).unwrap_or_else(|error| panic!("Associated 基类型须能完成 bind: {error}"));
    let carrier = find_struct(&hir, "Carrier");
    let carrier_decl = carrier.constructor_declaration.expect("Carrier constructor_declaration");
    let inspect = hir.functions.iter().find(|item| item.name.as_str() == "inspect").expect("inspect");
    let param_ty = &inspect.params[0].ty;
    match param_ty {
        ValkyrieType::Associated(associated) => {
            assert_eq!(associated.name.as_str(), "Payload");
            assert_eq!(
                associated.base,
                ValkyrieType::Nominal { declaration: carrier_decl, arguments: Vec::new() },
                "Associated.base 须是 Nominal{{Carrier}}，不得残留 Named，got {:?}",
                associated.base
            );
        }
        other => panic!("参数类型须是 Associated，got {other:?}"),
    }
}

#[test]
fn same_module_usize_eq_literal_resolves_operator_contract() {
    // 字面量 `0` 默认推断为 i32；必须在 unify 前按 formal `usize` 特化。
    let source = r#"
[primitive("core::primitive::usize")]
structure usize { }
imply usize {
    infix `==`(self, rhs: Self): bool { return true }
}
micro check(ordinal: usize): bool {
    return ordinal == 0
}
"#;
    ValkyrieCompiler::default().compile_source(source).unwrap_or_else(|error| panic!("同模块 usize==0 必须解析: {error}"));
}

#[test]
fn cross_package_array_host_get_usize_ops_resolve() {
    // 对齐 std.adaptor.wasm `array_host_get`：`ordinal == 0` / `ordinal > item_count`。
    let groups = vec![
        CompilerSourceGroup {
            dependency_key: "core".into(),
            name: "core".into(),
            sources: vec![CompilerSourceFile {
                path: "core.v".into(),
                source: r#"
namespace core::primitive;
[primitive("core::primitive::usize")]
structure usize { }
imply usize {
    infix `==`(self, rhs: Self): bool { return true }
    infix `>`(self, rhs: Self): bool { return true }
    infix `-`(self, rhs: Self): Self { return self }
}
namespace core::types;
unite Option<T> { Some { value: T }, None }
micro Some<T>(value: T) -> Option<T> {
    let wrapped: Option<T> = Some(value)
    return wrapped
}
"#
                .into(),
            }],
            direct_dependencies: Vec::new(),
        },
        CompilerSourceGroup {
            dependency_key: "std.adaptor.wasm".into(),
            name: "std.adaptor.wasm".into(),
            sources: vec![CompilerSourceFile {
                path: "std.adaptor.wasm.v".into(),
                source: r#"
namespace std.adaptor.wasm.collection;
[export] micro array_host_get<T>(array: [T], ordinal: usize): Option<T> {
    let item: T = array::[ordinal]
    return Some(item)
}
"#
                .into(),
            }],
            direct_dependencies: vec!["core".into()],
        },
    ];
    ValkyrieCompiler::default()
        .resolve_source_groups(&groups, DEFAULT_COMPILE_ARCH)
        .unwrap_or_else(|error| panic!("跨包 array_host_get Some/下标必须解析: {error}"));
}

#[test]
fn cross_package_some_with_local_value_resolves() {
    let groups = vec![
        CompilerSourceGroup {
            dependency_key: "core".into(),
            name: "core".into(),
            sources: vec![CompilerSourceFile {
                path: "core.v".into(),
                source: r#"
namespace core::types;
unite Option<T> { Some { value: T }, None }
micro Some<T>(value: T) -> Option<T> {
    let wrapped: Option<T> = Some(value)
    return wrapped
}
"#
                .into(),
            }],
            direct_dependencies: Vec::new(),
        },
        CompilerSourceGroup {
            dependency_key: "app".into(),
            name: "app".into(),
            sources: vec![CompilerSourceFile {
                path: "app.v".into(),
                source: r#"
[export] micro wrap<T>(value: T): Option<T> {
    return Some(value)
}
"#
                .into(),
            }],
            direct_dependencies: vec!["core".into()],
        },
    ];
    ValkyrieCompiler::default()
        .resolve_source_groups(&groups, DEFAULT_COMPILE_ARCH)
        .unwrap_or_else(|error| panic!("跨包 Some(value) 必须解析: {error}"));
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn cross_package_array_host_get_links_without_orphan_callable() {
    // 对齐 std.adaptor.wasm Array.v：`builtin.array.length` / `array::[i]` / Some/None 不得留下无合同 Call。
    let groups = vec![
        CompilerSourceGroup {
            dependency_key: "core".into(),
            name: "core".into(),
            sources: vec![CompilerSourceFile {
                path: "core.v".into(),
                source: r#"
namespace core::primitive;
[primitive("core::primitive::usize")]
structure usize { }
imply usize {
    infix `==`(self, rhs: Self): bool { return true }
    infix `>`(self, rhs: Self): bool { return true }
    infix `-`(self, rhs: Self): Self { return self }
}
namespace core::types;
unite Option<T> { Some { value: T }, None }
micro Some<T>(value: T) -> Option<T> {
    let wrapped: Option<T> = Some(value)
    return wrapped
}
"#
                .into(),
            }],
            direct_dependencies: Vec::new(),
        },
        CompilerSourceGroup {
            dependency_key: "std.adaptor.wasm".into(),
            name: "std.adaptor.wasm".into(),
            sources: vec![CompilerSourceFile {
                path: "std.adaptor.wasm.v".into(),
                source: r#"
namespace std.adaptor.wasm.collection;
[export] micro array_host_get<T>(array: [T], ordinal: usize): Option<T> {
    let item_count: usize = builtin.array.length(array)
    if ordinal == 0 || ordinal > item_count {
        return None
    }
    return Some(array::[ordinal - 1])
}
micro array_host_set<T>(mut array: [T], ordinal: usize, value: T): unit {
    let item_count: usize = builtin.array.length(array)
    if ordinal == 0 || ordinal > item_count {
        return
    }
    array::[ordinal - 1] = value
}
"#
                .into(),
            }],
            direct_dependencies: vec!["core".into()],
        },
    ];
    let compiler = ValkyrieCompiler::default();
    let mut hir_groups = compiler.resolve_source_groups(&groups, DEFAULT_COMPILE_ARCH).expect("HIR");
    let final_hir = hir_groups.pop().expect("adaptor");
    let mir_groups = hir_groups.iter().map(nyar_language::valkyrie::mir::MirLowerer::lower_module_semantic).collect::<Vec<_>>();
    let mut final_mir = nyar_language::valkyrie::mir::MirLowerer::lower_module_semantic(&final_hir);
    let mut defined = std::collections::BTreeSet::new();
    for module in std::iter::once(&final_mir).chain(mir_groups.iter()) {
        eprintln!("-- mir module functions --");
        for function in &module.functions {
            eprintln!("  {:?} decl={:?} inst={:?}", function.symbol, function.declaration, function.instance);
            if let Some(instance) = function.instance {
                defined.insert(instance);
            }
        }
        for contract in &module.external_calls {
            eprintln!("  import {:?} inst={:?}", contract.symbol, contract.instance);
            if let Some(instance) = contract.instance {
                defined.insert(instance);
            }
        }
    }
    eprintln!("defined instances: {defined:?}");
    for function in &final_mir.functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                if let nyar_language::valkyrie::mir::ssa::MirOperation::Call {
                    callee: nyar_language::valkyrie::mir::ssa::MirOperand::Callable(callee),
                    ..
                } = &instruction.kind
                {
                    if !defined.contains(callee) {
                        panic!(
                            "orphan Callable {callee:?} in {}; ops={:?}",
                            function.symbol,
                            function
                                .blocks
                                .iter()
                                .flat_map(|block| block.instructions.iter().map(|item| format!("{:?}", item.kind)))
                                .collect::<Vec<_>>()
                        );
                    }
                }
            }
        }
    }
    let modules = std::iter::once(&final_mir).chain(mir_groups.iter()).collect::<Vec<_>>();
    let functions = modules.iter().flat_map(|module| module.functions.iter().cloned()).collect::<Vec<_>>();
    let imports = modules.iter().flat_map(|module| module.external_calls.iter().cloned()).collect::<Vec<_>>();
    let structures = modules.iter().flat_map(|module| module.structs.iter().cloned()).collect::<Vec<_>>();
    let type_identities = nyar_language::valkyrie::mir::ssa::type_identity_table(&functions, &imports, &structures, &final_mir.sum_types);
    final_mir.type_identities = type_identities.clone();
    let mut deps = mir_groups;
    for module in &mut deps {
        module.type_identities = type_identities.clone();
    }
    nyar_language::valkyrie::compile_pipeline::link_reachable_dependency_mir(&mut final_mir, &deps)
        .unwrap_or_else(|error| panic!("array_host 跨包链接必须成功: {error}"));
    // 完整 Compiler 载荷路径：字面量须特化为 Nominal{usize}，不得 SMIR007 Integer32。
    ValkyrieCompiler::default()
        .compile_source_groups_to_program(&groups)
        .unwrap_or_else(|error| panic!("array_host_get 完整 Compiler 载荷不得留下孤儿实例: {error}"));
}

#[test]
fn same_module_private_wasm_host_call_resolves() {
    let source = r#"
namespace demo.text;
micro host_length(value: utf16): isize {
    return __host_utf16_length(value)
}
[wasm("env", "utf16_length")]
private micro __host_utf16_length(s: utf16): isize
"#;
    ValkyrieCompiler::default().compile_source(source).unwrap_or_else(|error| panic!("同模块 private wasm host 调用必须解析: {error}"));
}

#[test]
fn same_module_private_wasm_host_call_with_std_text_path_resolves() {
    // Oak parse_type 必须产出完整 NamePath（`.` / `::`）；禁止前端再按 span 切片重解析。
    let source = r#"
namespace demo.text;
micro host_length(value: std.text.utf16): isize {
    return __host_utf16_length(value)
}
[wasm("env", "utf16_length")]
private micro __host_utf16_length(s: utf16): isize
"#;
    ValkyrieCompiler::default().compile_source(source).unwrap_or_else(|error| panic!("Oak NamePath 须保留 std.text.utf16: {error}"));
}

#[test]
fn numeric_type_name_call_lowers_as_cast() {
    // 对齐 std.adaptor.wasm `time_ms`：`i64(perf_now())` 不得走未解析函数调用。
    let source = r#"
micro time_ms(value: f64): i64 {
    return i64(value)
}
"#;
    let module = ValkyrieCompiler::default().compile_source(source).unwrap_or_else(|error| panic!("数值类型名调用必须降为 Cast: {error}"));
    let function = module.functions.iter().find(|item| item.name.as_str() == "time_ms").expect("time_ms");
    let Some(expr) = function.body.expr.as_ref()
    else {
        panic!("expected return expr");
    };
    let HirExprKind::Return(Some(inner)) = &expr.kind
    else {
        panic!("expected return: {:?}", expr.kind);
    };
    assert!(
        matches!(inner.kind, HirExprKind::Cast { ty: ValkyrieType::Integer64 { signed: true }, .. }),
        "i64(x) must lower to Cast, got {:?}",
        inner.kind
    );
}

#[test]
fn same_module_array_subscript_resolves() {
    let source = r#"
micro get<T>(array: [T], ordinal: usize): T {
    return array::[ordinal]
}
"#;
    ValkyrieCompiler::default().compile_source(source).unwrap_or_else(|error| panic!("同模块 array::[i] 必须解析: {error}"));
}

#[test]
fn same_module_some_after_let_subscript_resolves() {
    let source = r#"
[primitive("core::primitive::usize")]
structure usize { }
imply usize {
    infix `-`(self, rhs: Self): Self { return self }
}
unite Option<T> { Some { value: T }, None }
micro Some<T>(value: T) -> Option<T> {
    let wrapped: Option<T> = Some(value)
    return wrapped
}
[export] micro array_host_get<T>(array: [T], ordinal: usize): Option<T> {
    let item: T = array::[ordinal]
    return Some(item)
}
"#;
    ValkyrieCompiler::default().compile_source(source).unwrap_or_else(|error| panic!("同模块 let + Some(item) 必须解析: {error}"));
}
