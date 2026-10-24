use rusty_valkyrie::{
    ValkyrieCompiler,
    types::{
        Identifier,
        hir::{HirAssociatedTypeBinding, HirTraitBound, HirWhereConstraint, ValkyrieType},
    },
};

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn return_type_is_independent_of_multiple_qualified_bounds() {
    let module = ValkyrieCompiler::default().compile_source(
        "micro present<T, U>(value: T, other: U): utf8 where T: first::Display + second.Display, U: Mapper<i32, bool, Item = utf8> { return \"ok\" }"
    ).expect("结构化约束必须保留");
    let function = &module.functions[0];
    assert_eq!(function.return_type, ValkyrieType::Utf8);
    assert_eq!(function.where_constraints.len(), 2);
    let first = &function.where_constraints[0];
    assert_eq!(first.target, function.params[0].ty);
    assert_eq!(first.bounds.len(), 2);
    assert_eq!(first.bounds[0].trait_path.parts(), &[Identifier::new("first"), Identifier::new("Display")]);
    assert_eq!(first.bounds[1].trait_path.parts(), &[Identifier::new("second"), Identifier::new("Display")]);
    let second = &function.where_constraints[1];
    assert_eq!(second.target, function.params[1].ty);
    assert_eq!(second.bounds[0].type_arguments, vec![ValkyrieType::Integer32 { signed: true }, ValkyrieType::Boolean]);
    assert_eq!(second.bounds[0].associated_types, vec![HirAssociatedTypeBinding { name: Identifier::new("Item"), ty: ValkyrieType::Utf8 }]);
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn repeated_traits_keep_equations_attached_to_each_bound() {
    let module = ValkyrieCompiler::default()
        .compile_source("micro present<T>(value: T): utf8 where T: Mapper<i32, Item = utf8> + Mapper<bool, Item = i32> { return \"ok\" }")
        .expect("同名 bound 不能覆盖彼此的参数或等式");
    let bounds = &module.functions[0].where_constraints[0].bounds;
    assert_eq!(bounds.len(), 2);
    assert_ne!(bounds[0].type_arguments, bounds[1].type_arguments);
    assert_eq!(bounds[0].associated_types[0].ty, ValkyrieType::Utf8);
    assert_eq!(bounds[1].associated_types[0].ty, ValkyrieType::Integer32 { signed: true });
}

#[test]
fn intersection_bounds_and_type_function_constraints_are_preserved() {
    let module = ValkyrieCompiler::default()
        .compile_source("mezzo present<T>(value: T): utf8 where T: first::Display & second::Display { return \"ok\" }")
        .expect("类型函数约束不能覆盖返回类型");
    let function = &module.type_functions[0];
    assert_eq!(function.return_type, ValkyrieType::Utf8);
    assert_eq!(function.where_constraints.len(), 1);
    assert_eq!(function.where_constraints[0].bounds.len(), 2);
}

#[test]
#[ignore = "contract-gate: SMIR003 / identity pipeline pending"]
fn bound_nominal_types_share_the_signature_declaration_identity() {
    let module = ValkyrieCompiler::default()
        .compile_source("class Card {} micro present(value: Card): utf8 where Card: Mapper<Card, Item = Card> { return \"ok\" }")
        .expect("约束中的类型必须由同一 resolver 冻结");
    let function = &module.functions[0];
    assert!(matches!(function.params[0].ty, ValkyrieType::Nominal { .. }));
    let constraint = &function.where_constraints[0];
    assert_eq!(constraint.target, function.params[0].ty);
    assert_eq!(constraint.bounds[0].type_arguments, vec![function.params[0].ty.clone()]);
    assert_eq!(constraint.bounds[0].associated_types[0].ty, function.params[0].ty);
}

#[test]
fn malformed_where_clauses_fail_at_the_text_boundary() {
    for clause in ["where T", "where T:", "where T: Display +", "where T: Mapper<i32", "where T: Mapper<Item = >", "where T: Mapper<>"] {
        let source = format!("micro present<T>(value: T): utf8 {clause} {{ return \"ok\" }}");
        assert!(ValkyrieCompiler::default().compile_source(&source).is_err(), "错误约束不能退化为无约束成功：{clause}");
    }
}

#[test]
fn concrete_generic_trait_call_reaches_canonical_without_template_intrinsic() {
    let source = "trait Template<Arguments> { micro render(self, arguments: Arguments): utf8 } \
        class Card {} \
        imply Card: Template<utf8> { micro render(self, arguments: utf8): utf8 { return arguments } } \
        micro render_card(card: Card, arguments: utf8): utf8 { return card.render(arguments) }";
    let program = ValkyrieCompiler::default().compile_source_to_program(source).expect("具体 trait 实现必须贯通 Canonical");
    program.canonical().validate().expect("trait 调用必须经过同一合同");
}

#[test]
fn missing_trait_evidence_does_not_compile_as_a_successful_invoke() {
    let source = "trait Display { micro display(self): utf8 } \
        micro present<T>(value: T): utf8 where T: Display { return value.display() } \
        micro caller(value: utf8): utf8 { return present(value) }";
    assert!(ValkyrieCompiler::default().compile_source_to_program(source).is_err(), "没有 Display evidence 时不得产生成功载荷");
}
