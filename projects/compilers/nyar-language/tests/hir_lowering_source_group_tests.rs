use nyar_language::{
    CompilerSourceFile, CompilerSourceGroup, ValkyrieCompiler,
    valkyrie::{
        hir::lowering::DEFAULT_COMPILE_ARCH,
        types::hir::{HirExprKind, HirFunction, HirStatementKind, types::ValkyrieType},
    },
};

#[test]
fn entry_source_group_parse_failure_reports_file_and_reason() {
    let source = "micro broken() { value = }";
    let direct = nyar_language::valkyrie::frontend::parse_source(source).expect_err("语法必须失败");
    let groups = vec![
        CompilerSourceGroup::from_source_files(
            "broken.package",
            "broken.package",
            vec![CompilerSourceFile { path: "source/broken.v".into(), source: source.into() }],
            vec![],
        )
        .as_entry(),
    ];
    let error = ValkyrieCompiler::default().resolve_source_groups(&groups, DEFAULT_COMPILE_ARCH).expect_err("不能生成残缺 HIR");
    // 入口文件没有可索引声明：失败必须保留入口组、文件路径与底层解析原因，
    // 而不是把无法建立入口的文件当成可执行程序继续 lowering。
    let text = error.to_string();
    assert!(text.contains("broken.package"), "{text}");
    assert!(text.contains("source/broken.v"), "{text}");
    assert!(text.contains(&direct.to_string()), "{text}");
}

#[test]
fn compiler_selects_reachable_files_from_inventory() {
    let groups = vec![
        CompilerSourceGroup {
            dependency_key: "library".into(),
            name: "library".into(),
            sources: vec![
                CompilerSourceFile { path: "source/good.v".into(), source: "micro answer() -> i32 { return 23 }".into() },
                CompilerSourceFile { path: "source/broken.v".into(), source: "micro broken() { value = }".into() },
            ],
            direct_dependencies: vec![],
            is_entry: false,
        },
        CompilerSourceGroup::single_source("app", "app", "[export] micro run() -> i32 { let value: i32 = answer(); return value }")
            .as_entry()
            .with_direct_dependencies(vec!["library".into()]),
    ];
    let modules = ValkyrieCompiler::default().resolve_source_groups(&groups, DEFAULT_COMPILE_ARCH).expect("Compiler 自行选择可达声明");
    assert_eq!(modules[0].functions.len(), 1);
}

#[test]
fn entry_group_parse_failure_fails_closed() {
    let groups = vec![
        CompilerSourceGroup::from_source_files(
            "app",
            "app",
            vec![CompilerSourceFile { path: "solution.v".into(), source: "micro broken() { value = }".into() }],
            vec![],
        )
        .as_entry(),
    ];
    let error = ValkyrieCompiler::default().resolve_source_groups(&groups, DEFAULT_COMPILE_ARCH).expect_err("入口文件语法错误必须失败");
    assert!(error.to_string().contains("solution.v"), "{error}");
}

#[test]
fn compiler_owned_plan_skips_unreachable_files() {
    let groups = vec![
        CompilerSourceGroup {
            dependency_key: "library".into(),
            name: "library".into(),
            sources: vec![
                CompilerSourceFile { path: "source/good.v".into(), source: "micro answer() -> i32 { return 23 }".into() },
                CompilerSourceFile { path: "source/broken.v".into(), source: "micro broken() { value = }".into() },
            ],
            direct_dependencies: vec![],
            ..Default::default()
        },
        CompilerSourceGroup::single_source("app", "app", "[export] micro run() -> i32 { return answer() }")
            .as_entry()
            .with_direct_dependencies(vec!["library".into()]),
    ];
    let modules =
        ValkyrieCompiler::default().resolve_source_groups(&groups, DEFAULT_COMPILE_ARCH).expect("body lowering 路径应跳过不可达坏文件");
    assert!(modules[0].functions.iter().any(|function| function.name.as_str() == "answer"));
}

fn first_call(function: &HirFunction) -> &nyar_language::types::hir::HirResolvedCall {
    let HirStatementKind::Let { initializer: Some(expression), .. } = &function.body.statements[0].kind
    else {
        panic!("缺少源码调用绑定");
    };
    let HirExprKind::Call { resolved: Some(call), .. } = &expression.kind
    else {
        panic!("调用没有完成 HIR 解析");
    };
    call
}

#[test]
fn declaration_identity_survives_dependency_export_and_call_selection() {
    let groups = vec![
        CompilerSourceGroup {
            dependency_key: "library".into(),
            name: "library".into(),
            sources: vec![CompilerSourceFile { path: "library.v".into(), source: "micro answer() -> i32 { return 23 }".into() }],
            direct_dependencies: vec![],
            is_entry: false,
        },
        CompilerSourceGroup {
            dependency_key: "app".into(),
            name: "app".into(),
            sources: vec![CompilerSourceFile {
                path: "app.v".into(),
                source: "[export] micro run() -> i32 { let value: i32 = answer(); return value }".into(),
            }],
            direct_dependencies: vec!["library".into()],
            is_entry: true,
        },
    ];
    let modules = ValkyrieCompiler::default().resolve_source_groups(&groups, DEFAULT_COMPILE_ARCH).expect("完整源码闭包的 HIR");
    let declaration = modules[0].functions[0].declaration.expect("依赖声明身份");
    assert_eq!(modules[1].imported_semantic_exports[0].functions[0].declaration, Some(declaration));
    assert_eq!(first_call(&modules[1].functions[0]).declaration, Some(declaration));
    assert_ne!(modules[1].functions[0].declaration, Some(declaration));
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn declaration_identity_survives_generic_signature_instantiation() {
    let hir = ValkyrieCompiler::default().compile_source(
            "micro identity<T>(value: T) -> T { return value } micro run(value: i32) -> i32 { let result: i32 = identity(value); return result }",
        ).expect("泛型源码调用");
    let declaration = hir.functions[0].declaration.expect("泛型声明身份");
    let call = first_call(&hir.functions[1]);
    assert_eq!(call.declaration, Some(declaration));
    assert_eq!(call.parameter_types, vec![ValkyrieType::Integer32 { signed: true }]);
    assert_ne!(call.parameter_types[0], hir.functions[0].params[0].ty);
}

#[test]
fn declaration_identity_separates_same_named_static_methods() {
    let hir = ValkyrieCompiler::default().compile_source(
            "structure Alpha {} structure Beta {} imply Alpha { micro answer(value: i32) -> i32 { return value } } imply Beta { micro answer(value: i32) -> i32 { return value } } micro run(value: i32) -> i32 { let result: i32 = Alpha.answer(value); return result }",
        ).expect("同名不同 owner 的静态调用");
    let alpha = hir.impls[0].methods[0].declaration.expect("Alpha 声明身份");
    let beta = hir.impls[1].methods[0].declaration.expect("Beta 声明身份");
    assert_ne!(alpha, beta);
    let call = first_call(&hir.functions[0]);
    assert_eq!(call.declaration, Some(alpha));
    assert!(!call.has_receiver);
    assert_eq!(call.parameter_types.len(), 1);
}

#[test]
fn materialized_hir_requires_call_resolution_before_semantic_success() {
    let compiler = ValkyrieCompiler::default();
    let source = "micro answer() -> i32 { return 1 } micro main() -> i32 { return answer() }";
    let hir = compiler.parse_source_group_without_call_resolution(source, &[], None, DEFAULT_COMPILE_ARCH).expect("物化阶段不绑定调用");
    let error = compiler.validate_hir_semantic_contract(&hir).expect_err("未绑定 HIR 不能越过语义边界");
    assert!(error.to_string().contains("SMIR003"), "{error}");
    compiler.compile_source(source).expect("分析入口必须执行解析和验证");
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn source_closure_materialization_precedes_call_validation() {
    let groups = vec![
        CompilerSourceGroup {
            dependency_key: "library".into(),
            name: "library".into(),
            sources: vec![CompilerSourceFile { path: "library.v".into(), source: "micro helper() -> i32 { return missing() }".into() }],
            direct_dependencies: Vec::new(),
            is_entry: false,
        },
        CompilerSourceGroup {
            dependency_key: "app".into(),
            name: "app".into(),
            sources: vec![CompilerSourceFile { path: "app.v".into(), source: "micro main(".into() }],
            direct_dependencies: vec!["library".into()],
            is_entry: true,
        },
    ];
    let error = ValkyrieCompiler::default().compile_source_groups_to_program(&groups).expect_err("完整源码闭包必须先完成物化");
    assert!(!error.to_string().contains("SMIR003"), "调用验证不得先于后续源码解析: {error}");
}

#[test]
fn materialized_group_does_not_grant_undeclared_dependency_visibility() {
    let groups = vec![
        CompilerSourceGroup {
            dependency_key: "library".into(),
            name: "library".into(),
            sources: vec![CompilerSourceFile { path: "library.v".into(), source: "micro answer() -> i32 { return 1 }".into() }],
            direct_dependencies: Vec::new(),
            is_entry: false,
        },
        CompilerSourceGroup {
            dependency_key: "app".into(),
            name: "app".into(),
            sources: vec![CompilerSourceFile { path: "app.v".into(), source: "[export] micro main() -> i32 { return answer() }".into() }],
            direct_dependencies: Vec::new(),
            is_entry: true,
        },
    ];
    let error = ValkyrieCompiler::default().compile_source_groups_to_program(&groups).expect_err("完整闭包不授予未声明依赖可见性");
    assert!(error.to_string().contains("SMIR003"), "{error}");
}

#[test]
fn compiler_owns_dependency_group_linking() {
    let groups = vec![
        CompilerSourceGroup {
            dependency_key: "core".into(),
            name: "core".into(),
            sources: vec![CompilerSourceFile { path: "core.v".into(), source: "micro answer() -> i32 { return 1 }".into() }],
            direct_dependencies: Vec::new(),
            is_entry: false,
        },
        CompilerSourceGroup {
            dependency_key: "app".into(),
            name: "app".into(),
            sources: vec![CompilerSourceFile { path: "app.v".into(), source: "[export] micro main() -> i32 { return answer() }".into() }],
            direct_dependencies: vec!["core".into()],
            is_entry: true,
        },
    ];
    let output = ValkyrieCompiler::default().compile_source_groups_to_program(&groups).expect("compiler closes source groups");
    assert_eq!(output.canonical().linked.module_name, "app");
    assert!(output.canonical().linked.callable_names.values().any(|name| name.to_string() == "core::answer"));
}

#[test]
fn compiler_links_transitive_calls_by_instance_and_excludes_unused_bodies() {
    let groups = vec![
        CompilerSourceGroup {
            dependency_key: "base".into(),
            name: "base".into(),
            sources: vec![CompilerSourceFile {
                path: "base.v".into(),
                source: "micro answer() -> i32 { return 23 } micro unused() -> bool { return true }".into(),
            }],
            direct_dependencies: vec![],
            is_entry: false,
        },
        CompilerSourceGroup {
            dependency_key: "library".into(),
            name: "library".into(),
            sources: vec![CompilerSourceFile { path: "library.v".into(), source: "micro relay() -> i32 { return answer() }".into() }],
            direct_dependencies: vec!["base".into()],
            is_entry: false,
        },
        CompilerSourceGroup {
            dependency_key: "app".into(),
            name: "app".into(),
            sources: vec![CompilerSourceFile { path: "app.v".into(), source: "[export] micro run() -> i32 { return relay() }".into() }],
            direct_dependencies: vec!["library".into()],
            is_entry: true,
        },
    ];
    let output = ValkyrieCompiler::default().compile_source_groups_to_program(&groups).expect("实例闭包必须贯穿 Canonical 校验及表示规划");
    let program = output.canonical();
    assert_eq!(program.mir.functions.len(), 3);
    assert!(!program.linked.callable_names.values().any(|name| name.to_string() == "base::unused"));
    let calls = program
        .mir
        .functions
        .values()
        .flat_map(|function| function.blocks.values())
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| {
            if let nyar_types::CanonicalOperation::Invoke { callee: nyar_types::CanonicalCallee::Item(instance), .. } = instruction.operation {
                Some(instance)
            }
            else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 2);
    assert!(calls.iter().all(|instance| program.mir.functions.contains_key(instance)));
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn generic_top_level_constructor_seeds_receive_callable_instances() {
    let compiler = ValkyrieCompiler::default();
    let hir = compiler
            .compile_source(
                "unite Option<T> { Some { value: T }, None } micro Some<T>(value: T) -> Option<T> { return Some(value) } micro option_none<T>() -> Option<T> { return None }",
            )
            .expect("泛型构造种子必须注册 callable instance");
    let some = hir.functions.iter().find(|function| function.name.as_str() == "Some").expect("Some constructor");
    let option_none = hir.functions.iter().find(|function| function.name.as_str() == "option_none").expect("option_none constructor");
    assert!(some.instance.is_some(), "generic `Some<T>` must receive seed callable instance");
    assert!(option_none.instance.is_some(), "generic `option_none<T>` must receive seed callable instance");
}

#[test]
fn trait_semicolon_signatures_do_not_materialize_callable_instances() {
    let hir = ValkyrieCompiler::default()
        .compile_source("trait As<T> { micro as_type(self, target_type: T) -> T?; }")
        .expect("trait signature must compile");
    let as_trait = hir.traits.iter().find(|trait_def| trait_def.name.as_str() == "As").expect("As trait");
    assert!(as_trait.methods.iter().any(|method| method.name.as_str() == "as_type" && method.is_abstract));
    assert!(as_trait.default_methods.is_empty(), "semicolon-only trait members must not become default bodies");
    let mir = nyar_language::valkyrie::mir::MirLowerer::lower_module_semantic(&hir);
    assert!(
        !mir.functions.iter().any(|function| function.display_symbol.contains("as_type")),
        "abstract trait signatures must not lower to MIR callables"
    );
}

#[test]
fn monomorphic_trait_default_methods_receive_callable_instances() {
    let compiler = ValkyrieCompiler::default();
    let hir = compiler
        .compile_source("trait Counter { micro value(self) -> i32 micro bump(self) -> i32 { return 1 } }")
        .expect("trait default method must compile");
    let counter = hir.traits.iter().find(|trait_def| trait_def.name.as_str() == "Counter").expect("Counter trait");
    let bump = counter.default_methods.iter().find(|method| method.name.as_str() == "bump").expect("bump default");
    assert!(bump.instance.is_some(), "materialized trait default must receive callable instance");
}

#[test]
fn constructor_identities_are_registered_before_overload_selection() {
    let compiler = ValkyrieCompiler::default();
    let hir = compiler.compile_source(
            "structure Packet { value: i32 } unite State { Ready { value: i32 }, Empty } micro run(value: i32) -> Packet { let packet = Packet { value: value }; return packet }",
        ).expect("源码构造声明");
    let structure = &hir.structs[0];
    let variant = &hir.enums[0].variants[0];
    let structure_declaration = structure.constructor_declaration.expect("结构构造声明 identity");
    assert!(structure.constructor_instance.is_some());
    assert!(variant.instance.is_some());
    assert_ne!(Some(structure_declaration), variant.declaration);
    let HirStatementKind::Let { initializer: Some(expression), .. } = &hir.functions[0].body.statements[0].kind
    else {
        panic!("缺少构造表达式");
    };
    let HirExprKind::Construct { resolved: Some(call), .. } = &expression.kind
    else {
        panic!("构造调用未绑定");
    };
    assert_eq!(call.declaration, Some(structure_declaration));
    assert_eq!(call.instance, structure.constructor_instance);
}

#[test]
fn compiler_carries_fragment_contracts_into_canonical_program() {
    let output = ValkyrieCompiler::default()
        .compile_source_to_program("[export(name: \"answer\")] [main] micro answer() -> i32 { return 23 }")
        .expect("源码必须形成完整 Canonical 成功载荷");
    let linked = &output.canonical().linked;
    let fragment = linked.fragments.values().next().expect("Compiler 必须绑定至少一个语义片段");
    assert!(!fragment.exported_operations.is_empty(), "片段不得丢失 callable identity");
    assert!(fragment.entry_operation.is_some(), "片段不得丢失入口 identity");
    assert!(fragment.exported_operations.iter().all(|instance| linked.callable_names.contains_key(instance)));
}

#[test]
fn compiler_rejects_unknown_dependency_identity() {
    let groups = vec![CompilerSourceGroup {
        dependency_key: "app".into(),
        name: "app".into(),
        sources: vec![CompilerSourceFile { path: "app.v".into(), source: "[export] micro main() { return }".into() }],
        direct_dependencies: vec!["missing".into()],
        is_entry: true,
        ..Default::default()
    }];
    let error =
        ValkyrieCompiler::default().compile_source_groups_to_program(&groups).expect_err("unknown dependency must fail at Compiler boundary");
    assert!(error.to_string().contains("依赖 `missing` 未在 `app` 之前提供"), "{error}");
}

#[test]
fn compiler_rejects_dependency_identity_collision() {
    let groups = vec![
        CompilerSourceGroup {
            dependency_key: "same".into(),
            name: "first".into(),
            sources: vec![CompilerSourceFile { path: "first.v".into(), source: "micro first() { return }".into() }],
            direct_dependencies: Vec::new(),
            is_entry: false,
        },
        CompilerSourceGroup {
            dependency_key: "same".into(),
            name: "second".into(),
            sources: vec![CompilerSourceFile { path: "second.v".into(), source: "micro second() { return }".into() }],
            direct_dependencies: Vec::new(),
            is_entry: true,
        },
    ];
    let error = ValkyrieCompiler::default().compile_source_groups_to_program(&groups).expect_err("duplicate dependency identity must fail");
    assert!(error.to_string().contains("Compiler 源码组身份为空或重复"), "{error}");
}

#[test]
fn multiple_entry_groups_merge_their_frozen_surface_contracts() {
    let groups = vec![
        CompilerSourceGroup::single_source("app", "app", "[main] micro start() -> i32 { return 23 }").as_entry(),
        CompilerSourceGroup::single_source("awsl", "awsl", "[export(asgard)] micro hydrate() -> i32 { return 7 }")
            .as_entry()
            .with_direct_dependencies(vec!["app".into()]),
    ];
    let output = ValkyrieCompiler::default().compile_source_groups_to_program(&groups).expect("多个入口组共同产生程序表面");
    let linked = &output.canonical().linked;
    // 末尾入口组是程序模块宿主；前序入口组的 [main] 与其自身 [export] 都必须保留。
    assert_eq!(linked.exports.len(), 1, "生成入口组的导出不得因末尾组选择而丢失");
    assert_eq!(linked.entries.len(), 1, "应用入口组的 [main] 不得因末尾组选择而丢失");
}

#[test]
fn multiple_main_entry_groups_fail_closed_instead_of_overwriting() {
    let groups = vec![
        CompilerSourceGroup::single_source("first", "first", "[main] micro first() -> i32 { return 1 }").as_entry(),
        CompilerSourceGroup::single_source("second", "second", "[main] micro second() -> i32 { return 2 }")
            .as_entry()
            .with_direct_dependencies(vec!["first".into()]),
    ];
    let error = ValkyrieCompiler::default().compile_source_groups_to_program(&groups).expect_err("同一程序不允许两个 [main] 入口");
    assert!(error.to_string().contains("多个入口 operation"), "{error}");
}
