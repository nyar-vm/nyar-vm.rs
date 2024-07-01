use nyar_language::*;
use std::collections::BTreeSet;

fn plan_from_groups(groups: &[(&str, bool, Vec<(String, String)>)]) -> SemanticClosure {
    let keys = groups.iter().map(|(key, _, _)| *key).collect::<Vec<_>>();
    let inputs = groups
        .iter()
        .enumerate()
        .map(|(index, (key, is_entry, files))| ClosureGroupInput {
            dependency_key: key,
            module_name: key,
            // 夹具按依赖顺序提供组：先出现的组视为可被后出现的组直接引用。
            direct_dependencies: &keys[..index],
            is_entry: *is_entry,
            files: files.as_slice(),
        })
        .collect::<Vec<_>>();
    plan_semantic_closure(&inputs, "wasm32").expect("semantic closure planning succeeds")
}

/// 显式声明依赖可见性的夹具：用于验证「未声明的依赖组不构成可见性」。
fn plan_with_visibility(groups: &[(&str, bool, Vec<(String, String)>)], visibility: &[&[&str]]) -> SemanticClosure {
    let inputs = groups
        .iter()
        .zip(visibility)
        .map(|((key, is_entry, files), dependencies)| ClosureGroupInput {
            dependency_key: key,
            module_name: key,
            direct_dependencies: dependencies,
            is_entry: *is_entry,
            files: files.as_slice(),
        })
        .collect::<Vec<_>>();
    plan_semantic_closure(&inputs, "wasm32").expect("semantic closure planning succeeds")
}

fn file(path: &str, source: &str) -> (String, String) {
    (path.to_string(), source.to_string())
}

#[test]
fn unreachable_inventory_syntax_error_does_not_expand_into_body_paths() {
    let groups = [
        (
            "library",
            false,
            vec![file("source/good.v", "micro answer() -> i32 { return 23 }"), file("source/broken.v", "micro broken() { value = }")],
        ),
        ("app", true, vec![file("solution.v", "[export] micro run() -> i32 { let value: i32 = answer(); return value }")]),
    ];
    let closure = plan_from_groups(&groups);
    assert_eq!(closure.groups[0].body_lowering_source_paths, BTreeSet::from(["source/good.v".into()]));
    assert_eq!(closure.groups[1].body_lowering_source_paths, BTreeSet::from(["solution.v".into()]));
    assert!(!closure.digest.is_empty());
    assert!(closure.demanded_declarations.iter().any(|key| key.name == "answer"));
    // 无法建立索引的文件必须显式记录，供下游在声明缺失时 fail-closed。
    assert_eq!(closure.unindexed_source_files.len(), 1);
    assert!(closure.unindexed_source_files.contains_key("library::source/broken.v"));
}

#[test]
fn entry_group_without_entry_declaration_is_structured_failure() {
    let groups = [("app", true, vec![file("solution.v", "micro run() -> i32 { return 23 }")])];
    let inputs = [ClosureGroupInput {
        dependency_key: "app",
        module_name: "app",
        direct_dependencies: &[],
        is_entry: true,
        files: groups[0].2.as_slice(),
    }];
    let error = plan_semantic_closure(&inputs, "wasm32").expect_err("entry group without `[export]`/`[main]` must fail");
    assert_eq!(error, SemanticClosureError::MissingEntryDeclaration { dependency_key: "app".to_string(), unindexed_source_files: Vec::new() });
}

#[test]
fn entry_group_body_paths_follow_explicit_entry_declarations_only() {
    let groups = [(
        "app",
        true,
        vec![file("solution.v", "[export] micro run() -> i32 { return 23 }"), file("source/not_entry.v", "micro helper() -> i32 { return 1 }")],
    )];
    let closure = plan_from_groups(&groups);
    // 入口组不是「整组文件都是程序」：未注解声明既不进入 seed，也不下沉其文件。
    assert_eq!(closure.groups[0].body_lowering_source_paths, BTreeSet::from(["solution.v".into()]));
    assert!(!closure.demanded_declarations.iter().any(|key| key.name == "helper"));
}

#[test]
fn main_annotation_is_an_entry_seed() {
    let groups = [("app", true, vec![file("main.v", "[main] micro start() -> i32 { return 1 }")])];
    let closure = plan_from_groups(&groups);
    assert!(closure.demanded_declarations.iter().any(|key| key.name == "start"));
    assert_eq!(closure.groups[0].body_lowering_source_paths, BTreeSet::from(["main.v".into()]));
}

#[test]
fn transitive_cross_package_symbol_pulls_base_dependency_file() {
    let groups = [
        ("base", false, vec![file("source/answer.v", "micro answer() -> i32 { return 23 }")]),
        ("library", false, vec![file("source/relay.v", "micro relay() -> i32 { return answer() }")]),
        ("app", true, vec![file("solution.v", "[export] micro run() -> i32 { return relay() }")]),
    ];
    let closure = plan_from_groups(&groups);
    assert_eq!(closure.groups[0].body_lowering_source_paths, BTreeSet::from(["source/answer.v".into()]));
    assert_eq!(closure.groups[1].body_lowering_source_paths, BTreeSet::from(["source/relay.v".into()]));
}

#[test]
fn cross_package_reachable_symbol_pulls_dependency_file_into_closure() {
    let groups = [
        (
            "library",
            false,
            vec![file("source/answer.v", "micro answer() -> i32 { return 23 }"), file("source/unused.v", "micro unused() -> i32 { return 0 }")],
        ),
        ("app", true, vec![file("solution.v", "[export] micro run() -> i32 { return answer() }")]),
    ];
    let closure = plan_from_groups(&groups);
    assert_eq!(closure.groups[0].body_lowering_source_paths, BTreeSet::from(["source/answer.v".into()]));
    assert!(!closure.groups[0].body_lowering_source_paths.contains("source/unused.v"));
}

#[test]
fn namespace_chain_does_not_pull_same_name_from_other_namespace() {
    let groups = [
        (
            "library",
            false,
            vec![
                file("source/a.v", "namespace a;\nmicro helper() -> i32 { return 1 }"),
                file("source/b.v", "namespace b;\nmicro helper() -> i32 { return 2 }"),
            ],
        ),
        ("app", true, vec![file("solution.v", "namespace a;\n[export] micro run() -> i32 { return helper() }")]),
    ];
    let closure = plan_from_groups(&groups);
    assert_eq!(closure.groups[0].body_lowering_source_paths, BTreeSet::from(["source/a.v".into()]));
}

#[test]
fn member_reference_resolves_within_namespace_before_global_fallback() {
    let groups = [
        (
            "library",
            false,
            vec![
                file("source/a.v", "namespace a;\nstructure S { _x: i32 }\nimply S { micro is_empty(self): bool { return true } }"),
                file("source/b.v", "namespace b;\nstructure T { _x: i32 }\nimply T { micro is_empty(self): bool { return true } }"),
            ],
        ),
        ("app", true, vec![file("solution.v", "namespace a;\n[export] micro run(s: S) -> bool { return s.is_empty() }")]),
    ];
    let closure = plan_from_groups(&groups);
    let lowered = &closure.groups[0].body_lowering_source_paths;
    assert!(lowered.contains("source/a.v"));
    assert!(!lowered.contains("source/b.v"));
}

#[test]
fn declared_member_pulls_owning_aggregate_shell() {
    let groups = [
        (
            "library",
            false,
            vec![file("source/hash.v", "namespace a;\nstructure S { _x: i32 }\nimply S { micro hash(self): usize { return 0 } }")],
        ),
        ("app", true, vec![file("solution.v", "namespace a;\n[export] micro run(s: S) -> usize { return s.hash() }")]),
    ];
    let closure = plan_from_groups(&groups);
    let demanded = &closure.groups[0].body_lowering_declarations;
    assert!(demanded.iter().any(|key| key.kind == DeclarationKind::Structure && key.name == "S"));
    assert!(demanded.iter().any(|key| key.name == "hash"));
}

#[test]
fn undemanded_impl_witness_keeps_its_shell_out_of_closure() {
    let groups = [
        (
            "library",
            false,
            vec![
                file("source/iterator/_.v", "namespace std.iterator;\ntrait Iterator { }"),
                file(
                    "source/impl.v",
                    "namespace std.collection;\nstructure S { _x: i32 }\nimply S: std::iterator::Iterator { micro not_used(self): bool { return true } }",
                ),
            ],
        ),
        ("app", true, vec![file("solution.v", "namespace std.collection;\n[export] micro run(s: S) -> bool { return true }")]),
    ];
    let closure = plan_from_groups(&groups);
    let lowered = &closure.groups[0].body_lowering_source_paths;
    assert!(lowered.contains("source/impl.v"));
    // 未被需求的 witness 方法连带其 impl 外壳一起不进入闭包：外壳的 trait 路径因此不必解析，
    // 闭包不得以「外壳始终物化」为名把整个文件的无差别引用拉进来。
    assert!(!lowered.contains("source/iterator/_.v"));
    assert!(!closure.groups[0].body_lowering_declarations.iter().any(|key| key.name == "not_used"));
}

#[test]
fn qualified_constructor_resolves_through_dependency_visibility() {
    let groups = [
        (
            "std",
            false,
            vec![file(
                "source/collection/HashMap.v",
                "namespace std.collection;\nclass HashMap<K, V> { _impl: i32 }\nimply HashMap<K, V> { micro new(capacity: usize): Self { return Self { _impl: 0 } } }",
            )],
        ),
        (
            "app",
            true,
            vec![file(
                "solution.v",
                "namespace leetcode.two_sum;\n[export] micro two_sum(size: usize) -> HashMap<i32, i32> { return HashMap::new(0) }",
            )],
        ),
    ];
    let closure = plan_from_groups(&groups);
    let demanded = &closure.groups[0].body_lowering_declarations;
    // 跨 namespace 的类型引用与限定构造必须通过依赖可见性命中，而不是靠名字认亲。
    assert!(demanded.iter().any(|key| key.kind == DeclarationKind::Class && key.name == "HashMap"));
    assert!(demanded.iter().any(|key| key.kind == DeclarationKind::ImplyMethod && key.name == "new"));
}

#[test]
fn member_selector_promotes_used_member_and_skips_unused() {
    let groups = [
        (
            "library",
            false,
            vec![file(
                "source/s.v",
                "namespace a;\nstructure S { _x: i32 }\nimply S { micro used(self): bool { return true }\nmicro unused(self): bool { return true } }",
            )],
        ),
        ("app", true, vec![file("solution.v", "namespace a;\n[export] micro run(s: S) -> bool { return s.used() }")]),
    ];
    let closure = plan_from_groups(&groups);
    let demanded = &closure.groups[0].body_lowering_declarations;
    // 接收者类型在声明索引阶段不可知：成员只按「该名字出现在闭包需求中 + 拥有聚合已需求」提升。
    assert!(demanded.iter().any(|key| key.name == "used"));
    assert!(!demanded.iter().any(|key| key.name == "unused"));
}

#[test]
fn reference_into_undeclared_dependency_group_is_not_demanded() {
    let groups = [
        ("library", false, vec![file("source/answer.v", "micro answer() -> i32 { return 23 }")]),
        ("app", true, vec![file("solution.v", "[export] micro run() -> i32 { return answer() }")]),
    ];
    let visibility: &[&[&str]] = &[&[], &[]];
    let closure = plan_with_visibility(&groups, visibility);
    // 未声明直接依赖即不构成可见性：名字命中也不得跨组把声明拉进闭包。
    assert!(closure.groups[0].body_lowering_source_paths.is_empty());
    assert!(!closure.demanded_declarations.iter().any(|key| key.name == "answer"));
}

#[test]
fn closure_digest_is_stable_for_identical_input() {
    let groups = [("app", true, vec![file("solution.v", "[export] micro run() -> i32 { return 23 }")])];
    let first = plan_from_groups(&groups);
    let second = plan_from_groups(&groups);
    assert_eq!(first.digest, second.digest);
    assert_eq!(first.declaration_index.len(), 1);
}

#[test]
fn closure_digest_binds_demanded_source_content() {
    let baseline = [("app", true, vec![file("solution.v", "[export] micro run() -> i32 { return 23 }")])];
    let changed = [("app", true, vec![file("solution.v", "[export] micro run() -> i32 { return 24 }")])];
    assert_ne!(plan_from_groups(&baseline).digest, plan_from_groups(&changed).digest);
}

#[test]
fn closure_digest_ignores_unreachable_dependency_body_change() {
    let groups = |body: &str| {
        [
            ("library", false, vec![file("source/answer.v", "micro answer() -> i32 { return 23 }"), file("source/unused.v", body)]),
            ("app", true, vec![file("solution.v", "[export] micro run() -> i32 { return answer() }")]),
        ]
    };
    let first = plan_from_groups(&groups("micro unused() -> i32 { return 1 }"));
    let second = plan_from_groups(&groups("micro unused() -> i32 { return 2 }"));
    assert_eq!(first.digest, second.digest);
}

#[test]
fn operator_use_site_promotes_receiver_operator_member() {
    let groups = [
        (
            "core",
            false,
            vec![file(
                "source/primitive/usize.v",
                "namespace core::primitive;\nstructure usize { }\nimply usize {\n    infix `+`(self, rhs: Self): Self { return rhs }\n    infix `<`(self, rhs: Self): bool { return true }\n    infix `==`(self, rhs: Self): bool { return true }\n}",
            )],
        ),
        (
            "std",
            false,
            vec![file(
                "source/hash.v",
                "namespace std.collection;\nmicro hash_utf8(n: usize) -> usize {\n    let mut i: usize = 0\n    while i < n {\n        i = i + 1\n    }\n    return i\n}",
            )],
        ),
        ("app", true, vec![file("solution.v", "namespace app;\n[export] micro run() -> usize { return hash_utf8(3) }")]),
    ];
    let closure = plan_from_groups(&groups);
    let demanded = &closure.groups[0].body_lowering_declarations;
    // 运算符是普通成员调用：`i < n` 与 `i + 1` 都是 `usize` 的成员选择名，
    // 因此接收者类型 `usize` 上的对应运算符成员必须进入闭包。
    assert!(demanded.iter().any(|key| key.kind == DeclarationKind::ImplyMethod && key.name == "infix <"));
    assert!(demanded.iter().any(|key| key.kind == DeclarationKind::ImplyMethod && key.name == "infix +"));
    // 没有被选择名命中的运算符成员不得连带进入闭包。
    assert!(!demanded.iter().any(|key| key.kind == DeclarationKind::ImplyMethod && key.name == "infix =="));
}

#[test]
fn bare_member_selector_never_resolves_to_any_member_declaration() {
    let groups = [
        ("library", false, vec![file("source/s.v", "namespace a;\nstructure S { _x: i32 }\nimply S { micro hash(self): usize { return 0 } }")]),
        ("app", true, vec![file("solution.v", "namespace a;\n[export] micro run() -> usize { return hash() }")]),
    ];
    let closure = plan_from_groups(&groups);
    // 成员选择名不建立全库同名索引：没有接收者类型事实时，裸名 `hash` 不得命中任何成员声明，
    // 因此既不能把成员拉进闭包，也不能把只含该成员的文件下沉到 body lowering。
    assert!(!closure.groups[0].body_lowering_declarations.iter().any(|key| key.name == "hash"));
    assert!(closure.groups[0].body_lowering_source_paths.is_empty());
}

#[test]
fn member_selector_promotes_only_demanded_aggregate_owner() {
    let groups = [
        (
            "library",
            false,
            vec![
                file(
                    "source/swiss.v",
                    "namespace a;\nstructure SwissTable { _x: i32 }\nimply SwissTable { micro hash(self): usize { return 0 } }",
                ),
                file("source/bitset.v", "namespace a;\nstructure BitSet { _x: i32 }\nimply BitSet { micro hash(self): usize { return 0 } }"),
            ],
        ),
        ("app", true, vec![file("solution.v", "namespace a;\n[export] micro run(s: SwissTable) -> usize { return s.hash() }")]),
    ];
    let closure = plan_from_groups(&groups);
    let demanded = &closure.groups[0].body_lowering_declarations;
    // 同 namespace 内的同名成员不得互相污染：只有被需求的拥有聚合能凭选择名提升自己的成员。
    assert!(demanded.iter().any(|key| key.name == "hash" && key.owner.as_deref() == Some("SwissTable")));
    assert!(!demanded.iter().any(|key| key.owner.as_deref() == Some("BitSet")));
    assert!(!closure.groups[0].body_lowering_source_paths.contains("source/bitset.v"));
}

#[test]
fn member_owner_identity_is_resolved_through_type_alias() {
    let groups = [
        (
            "std",
            false,
            vec![
                file("source/text/_.v", "namespace std.text;\ntype utf8 = Utf8Text;"),
                file(
                    "source/text/Utf8Text.v",
                    "namespace std.text;\nclass Utf8Text { _repr: [u8] }\nimply Utf8Text { micro length(self) -> i32 { return 0 } }",
                ),
                file("source/hash/HashPrimitives.v", "namespace std.collection;\nimply utf8: Hash { micro hash(self): usize { return 0 } }"),
            ],
        ),
        ("app", true, vec![file("solution.v", "namespace app;\n[export] micro run(s: utf8) -> usize { return s.hash() }")]),
    ];
    let closure = plan_from_groups(&groups);
    let demanded = &closure.groups[0].body_lowering_declarations;
    // 别名不是独立名义聚合：`utf8` 的引用展开到目标聚合 `Utf8Text`，`imply utf8: Hash`
    // 的 witness 因此挂在该聚合身份上，而不是挂在别名身份或短名 `utf8` 上。
    assert!(demanded.iter().any(|key| key.kind == DeclarationKind::TypeAlias && key.name == "utf8"));
    assert!(demanded.iter().any(|key| key.kind == DeclarationKind::Class && key.name == "Utf8Text"));
    assert!(demanded.iter().any(|key| key.name == "hash" && key.owner.as_deref() == Some("utf8")));
    assert!(closure.groups[0].body_lowering_source_paths.contains("source/hash/HashPrimitives.v"));
}

#[test]
fn member_owner_link_is_gated_by_owner_group_visibility() {
    let groups = [
        ("core", false, vec![file("source/primitive/i64.v", "namespace core.primitive;\nstructure i64 { }")]),
        ("std", false, vec![file("source/hash.v", "namespace std.collection;\nimply i64: Hash { micro hash(self): usize { return 0 } }")]),
        ("app", true, vec![file("solution.v", "namespace app;\n[export] micro run(x: i64) -> usize { return x.hash() }")]),
    ];
    // 成员与拥有者跨依赖组：std 的 `imply i64: Hash` 与 core 的 `structure i64` 按声明身份连接。
    let visible = plan_with_visibility(&groups, &[&[], &["core"], &["core", "std"]]);
    assert!(visible.demanded_declarations.iter().any(|key| key.dependency_key == "std" && key.name == "hash"));
    // std 未声明 core 为直接依赖时，拥有者引用不得命中 core 的声明，成员因而不成立也不得提升。
    let hidden = plan_with_visibility(&groups, &[&[], &[], &["core", "std"]]);
    assert!(!hidden.demanded_declarations.iter().any(|key| key.name == "hash"));
    assert!(hidden.demanded_declarations.iter().any(|key| key.kind == DeclarationKind::Structure && key.name == "i64"));
}
