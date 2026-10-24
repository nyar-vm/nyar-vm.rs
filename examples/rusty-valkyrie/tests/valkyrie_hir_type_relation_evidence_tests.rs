#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn explicit_impl_for_non_struct_type_supplies_where_evidence() {
    let compiler = rusty_valkyrie::ValkyrieCompiler::default();
    let module = compiler.compile_source(
        "trait Display { micro display(self): utf8 } imply utf8: Display { micro display(self): utf8 { return self } } micro present<T>(value: T): utf8 where T: Display { return \"ok\" } micro caller(value: utf8): utf8 { return present(value) }"
    ).expect("primitive explicit impl must satisfy generic bound");
    assert_eq!(module.functions.iter().find(|f| f.name.as_str() == "present").unwrap().where_constraints.len(), 1);
}

#[test]
fn absent_explicit_impl_for_non_struct_type_is_rejected() {
    let compiler = rusty_valkyrie::ValkyrieCompiler::default();
    assert!(compiler.compile_source(
        "trait Display { micro display(self): utf8 } micro present<T>(value: T): utf8 where T: Display { return \"ok\" } micro caller(value: utf8): utf8 { return present(value) }"
    ).is_err());
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn same_short_trait_names_keep_independent_evidence() {
    rusty_valkyrie::ValkyrieCompiler::default()
        .compile_source(
            r#"
namespace alpha;
trait Protocol { micro read(self): utf8 }
imply utf8: Protocol { micro read(self): utf8 { return self } }
micro accept<T>(value: T): utf8 where T: Protocol { return "ok" }
micro caller(value: utf8): utf8 { return accept(value) }
namespace beta;
trait Protocol { micro write(self): bool }
"#,
        )
        .expect("同短名的后续 trait 不得覆盖已绑定 evidence");
}

#[test]
fn evidence_cannot_cross_trait_namespace() {
    assert!(
        rusty_valkyrie::ValkyrieCompiler::default()
            .compile_source(
                r#"
namespace alpha;
trait Protocol { micro read(self): utf8 }
imply utf8: Protocol { micro read(self): utf8 { return self } }
namespace beta;
trait Protocol { micro read(self): utf8 }
micro accept<T>(value: T): utf8 where T: Protocol { return "ok" }
micro caller(value: utf8): utf8 { return accept(value) }
"#
            )
            .is_err(),
        "alpha 的实现不能提供 beta 的 evidence"
    );
}

#[test]
fn duplicate_or_incompatible_methods_cannot_supply_evidence() {
    for methods in ["micro read(self): utf8 { return self } micro read(self): utf8 { return self }", "micro read(self): bool { return true }"] {
        let source = format!(
            "trait Protocol {{ micro read(self): utf8 }} imply utf8: Protocol {{ {methods} }} micro accept<T>(value: T): utf8 where T: Protocol {{ return \"ok\" }} micro caller(value: utf8): utf8 {{ return accept(value) }}"
        );
        assert!(rusty_valkyrie::ValkyrieCompiler::default().compile_source(&source).is_err(), "歧义/错误签名不能作为 evidence");
    }
}
