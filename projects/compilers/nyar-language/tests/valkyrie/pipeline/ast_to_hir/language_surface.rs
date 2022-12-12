use super::*;

#[test]
fn compiler_facade_lowers_into_mir_and_build_output_from_moved_tests() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 11 });
    let mir = compiler
        .compile_source_to_mir(
            r#"micro main(input: i64) -> i64 {
    return input;
}
"#,
        )
        .unwrap();
    assert_eq!(mir.functions.len(), 1);
    assert!(mir.functions[0].values.iter().any(|value| matches!(value.origin, nyar_language::MirValueOrigin::Parameter { index: 0, .. })));

    let build_output = compiler
        .compile_source_to_build_output(
            r#"micro main() {
    std::console::write_line("hi");
}
"#,
        )
        .unwrap();
    assert_eq!(build_output.hir_function_count(), 1);
    assert_eq!(build_output.neutral_plan().semantic_fragments.len(), 1);
}

#[test]
fn lowers_structured_attribute_arguments_into_hir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 13 });
    let module = compiler
        .compile_source(
            r#"
[clr("mscorlib", "System.Console", "WriteLine")]
micro helper(message: utf16) {
    return;
}
"#,
        )
        .unwrap();

    assert_eq!(module.functions[0].annotations.len(), 1);
    assert_eq!(module.functions[0].annotations[0].name.to_string(), "clr");
    assert_eq!(module.functions[0].annotations[0].arguments.len(), 3);
    assert!(matches!(module.functions[0].annotations[0].arguments[0].value.kind, HirExprKind::Literal(_)));
}

#[test]
fn lowers_brand_interop_tags_into_neutral_host_contract() {
    for (version_id, tag, arguments, argument_count) in [
        (703, "clr", r#""mscorlib", "System.Console", "WriteLine""#, 3usize),
        (704, "wasm", r#""env", "memory""#, 2usize),
        (705, "wasi", r#""wasi:io/streams", "blocking-write-and-flush""#, 2usize),
        (706, "jvm", r#""java/lang/System", "currentTimeMillis""#, 2usize),
        (707, "com", r#""Excel.Application", "Visible", "set""#, 3usize),
    ] {
        let compiler = ValkyrieCompiler::new(SourceID { version_id });
        let build_output = compiler
            .compile_source_to_build_output(&format!(
                r#"
[{tag}({arguments})]
micro helper(message: utf16) {{
    return;
}}
"#
            ))
            .unwrap();
        let neutral_plan = build_output.neutral_plan();

        assert!(neutral_plan.program_facts.requires_capability("host-interop"));
        assert_eq!(neutral_plan.program_facts.capabilities.len(), 1);
        assert_eq!(neutral_plan.program_facts.capabilities[0].as_str(), "host-interop");
        assert_eq!(neutral_plan.program_facts.runtime_requirements.len(), 1);
        assert_eq!(neutral_plan.program_facts.runtime_requirements[0].key, "host-interop");
        assert_eq!(neutral_plan.program_facts.runtime_requirements[0].value, "required");
        assert!(neutral_plan.program_facts.functions[0].uses_host_interop);
        assert!(neutral_plan.program_facts.functions[0].external_import_link.is_some());
        assert_eq!(neutral_plan.semantic_fragments[0].external_import_links.len(), 1);
        assert!(neutral_plan.semantic_fragments[0].external_call_edges.is_empty());
        let link = neutral_plan.program_facts.functions[0].external_import_link.as_ref().unwrap();
        assert!(link.matches_boundary("host"));
        assert!(link.platform_tag.is_none());
        assert_eq!(link.locator_segments().len(), argument_count);
    }
}

#[test]
fn lowers_real_clr_call_edge_from_main_into_semantic_fragment() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 708 });
    let build_output = compiler
        .compile_source_to_build_output(
            r#"
[clr("mscorlib", "System.Console", "WriteLine")]
micro console_write_line(message: utf16): unit;

[main]
micro main() -> i64 {
    console_write_line("hello from clr")
    return 0;
}
"#,
        )
        .unwrap();
    let fragment = &build_output.neutral_plan().semantic_fragments[0];

    assert_eq!(fragment.external_call_edges.len(), 1);
    assert_eq!(fragment.external_call_edges[0].caller.to_string(), "main::main");
    assert_eq!(fragment.external_call_edges[0].callee_symbol.to_string(), "main::console_write_line");
}

#[test]
fn lowers_term_turbofish_into_generic_apply() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 29 });
    let module = compiler
        .compile_source(
            r#"micro main() {
    T::<i64>();
}
"#,
        )
        .unwrap();
    let HirStatementKind::Expr(expression) = &module.functions[0].body.statements[0].kind
    else {
        panic!("expected expression statement");
    };

    match &expression.kind {
        HirExprKind::Call { callee, args, .. } => {
            assert!(args.is_empty());
            assert!(matches!(
                callee.kind,
                HirExprKind::GenericApply { ref arguments, .. }
                    if arguments.len() == 1 && arguments[0] == ValkyrieType::Integer64 { signed: true }
            ));
        }
        _ => panic!("expected call expression"),
    }
}

#[test]
fn lowers_instance_method_with_implicit_self_param() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 31 });
    let module = compiler
        .compile_source(
            r#"
class Player {
    micro heal(amount: i64) -> i64 {
        self.health;
        return amount;
    }
}
"#,
        )
        .unwrap();

    let method = &module.structs[0].methods[0];
    assert_eq!(method.params.len(), 2);
    assert_eq!(method.params[0].name.name.as_str(), "self");
    assert!(matches!(method.params[0].ty, ValkyrieType::r#SelfType));
    assert_eq!(method.params[1].name.name.as_str(), "amount");
}

#[test]
fn keeps_void_alias_and_self_name_as_user_types_until_hir_lowering() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 67 });
    let module = compiler
        .compile_source(
            r#"
type void = c_void;
micro convert(value: Self) -> void {
}
micro make() -> () {
}
"#,
        )
        .unwrap();

    let convert = &module.functions[0];
    assert!(matches!(convert.params[0].ty, ValkyrieType::r#SelfType));
    assert!(matches!(
        convert.return_type,
        ValkyrieType::Named(ref name) if name.as_str() == "void" || name.as_str() == "c_void"
    ));
    assert!(!matches!(convert.return_type, ValkyrieType::Void));

    let make = &module.functions[1];
    assert_eq!(make.return_type, ValkyrieType::Unit);
}

#[test]
fn lowers_getter_and_setter_into_one_hir_property() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 61 });
    let module = compiler
        .compile_source(
            r#"class Rectangle {
    get area(self) -> i64 {
        return self.width;
    }

    set area(mut self, value: i64) {
        self.width = value;
    }
}"#,
        )
        .unwrap();

    let class = &module.structs[0];
    assert_eq!(class.properties.len(), 1);
    let property = &class.properties[0];
    assert_eq!(property.name.as_str(), "area");
    assert_eq!(property.ty, ValkyrieType::Integer64 { signed: true });
    assert!(!property.is_readonly);
    assert!(property.getter.is_some());
    assert!(property.setter.is_some());

    let getter = property.getter.as_ref().unwrap();
    assert_eq!(getter.name.as_str(), "area");
    assert_eq!(getter.params.len(), 1);
    assert_eq!(getter.return_type, ValkyrieType::Integer64 { signed: true });

    let setter = property.setter.as_ref().unwrap();
    assert_eq!(setter.name.as_str(), "set_area");
    assert_eq!(setter.params.len(), 2);
    assert_eq!(setter.return_type, ValkyrieType::Unit);
}

#[test]
fn lowers_property_modifiers_into_hir_flags() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 63 });
    let module = compiler
        .compile_source(
            r#"class Shape {
    virtual get area(self) -> i64;
}

class MathConstants {
    static final get pi() -> i64 {
        return 3;
    }
}"#,
        )
        .unwrap();

    let shape = &module.structs[0];
    assert_eq!(shape.properties.len(), 1);
    let area = &shape.properties[0];
    assert!(area.is_abstract);
    assert!(area.is_virtual);
    assert!(!area.is_static);
    assert!(area.getter.as_ref().unwrap().is_abstract);

    let math = &module.structs[1];
    assert_eq!(math.properties.len(), 1);
    let pi = &math.properties[0];
    assert!(pi.is_static);
    assert!(pi.is_final);
    assert!(pi.is_readonly);
    assert!(pi.getter.is_some());
    assert_eq!(pi.getter.as_ref().unwrap().params.len(), 0);
}

#[test]
fn lowers_mut_field_modifier_into_hir_flag() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 64 });
    let module = compiler
        .compile_source(
            r#"singleton Counter {
    mut total: i64 = 0
    count: i64 = 0
}"#,
        )
        .unwrap();

    assert_eq!(module.singletons.len(), 1);
    let fields = &module.singletons[0].fields;
    assert_eq!(fields.len(), 2);
    assert!(fields[0].is_mutable);
    assert_eq!(fields[0].name.as_str(), "total");
    assert!(!fields[1].is_mutable);
    assert_eq!(fields[1].name.as_str(), "count");
}

#[test]
fn lowers_lazy_singleton_modifier_into_hir_flag() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 65 });
    let module = compiler
        .compile_source(
            r#"lazy singleton Counter {
    total: i64 = 0
}"#,
        )
        .unwrap();

    assert_eq!(module.singletons.len(), 1);
    assert!(module.singletons[0].is_lazy);
    assert_eq!(module.singletons[0].instance_name.as_str(), "INSTANCE");
}

#[test]
fn eager_singleton_defaults_to_static_init() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 66 });
    let module = compiler
        .compile_source(
            r#"singleton Counter {
    total: i64 = 0
}"#,
        )
        .unwrap();

    assert_eq!(module.singletons.len(), 1);
    assert!(!module.singletons[0].is_lazy);
}

#[test]
fn lowers_static_method_without_implicit_self_param() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 37 });
    let module = compiler
        .compile_source(
            r#"
class Math {
    static micro abs(value: i64) -> i64 {
        return value;
    }
}
"#,
        )
        .unwrap();

    let method = &module.structs[0].methods[0];
    assert_eq!(method.params.len(), 1);
    assert_eq!(method.params[0].name.name.as_str(), "value");
}

#[test]
fn lowers_member_field_access_and_assignment_into_getter_setter_calls() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 41 });
    let module = compiler
        .compile_source(
            r#"
class Player {
    micro heal(amount: i64) {
        self.health = amount;
        self.health;
    }
}
"#,
        )
        .unwrap();

    let statements = &module.structs[0].methods[0].body.statements;
    let HirStatementKind::Expr(setter_expr) = &statements[0].kind
    else {
        panic!("expected setter expression");
    };
    let HirStatementKind::Expr(getter_expr) = &statements[1].kind
    else {
        panic!("expected getter expression");
    };

    match &setter_expr.kind {
        HirExprKind::StoreField { object, field, value } => {
            assert_eq!(field.as_str(), "health");
            assert!(matches!(object.kind, HirExprKind::Variable(_)));
            assert!(matches!(value.kind, HirExprKind::Variable(_)));
        }
        _ => panic!("expected store field"),
    }

    match &getter_expr.kind {
        HirExprKind::FieldAccess { object, field } => {
            assert_eq!(field.as_str(), "health");
            assert!(matches!(object.kind, HirExprKind::Variable(_)));
        }
        _ => panic!("expected field access"),
    }
}

#[test]
fn preserves_instance_method_call_without_rewriting_to_getter() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 43 });
    let module = compiler
        .compile_source(
            r#"
class Player {
    micro tick() {
        self.refresh();
    }

    micro refresh() {
    }
}
"#,
        )
        .unwrap();

    let HirStatementKind::Expr(expression) = &module.structs[0].methods[0].body.statements[0].kind
    else {
        panic!("expected expression statement");
    };

    match &expression.kind {
        HirExprKind::Call { callee, args, .. } => {
            assert_eq!(args.len(), 1);
            assert!(matches!(args[0].value.kind, HirExprKind::Variable(_)));
            assert!(matches!(
                callee.kind,
                HirExprKind::Path(ref path) if path.to_string() == "refresh"
            ));
        }
        _ => panic!("expected method call"),
    }
}

#[test]
fn lowers_member_turbofish_call_with_receiver_as_first_argument() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 47 });
    let module = compiler
        .compile_source(
            r#"
class Player {
    micro tick(value: i64) {
        self.refresh::<i64>(value);
    }

    micro refresh(value: i64) {
    }
}
"#,
        )
        .unwrap();

    let HirStatementKind::Expr(expression) = &module.structs[0].methods[0].body.statements[0].kind
    else {
        panic!("expected expression statement");
    };

    match &expression.kind {
        HirExprKind::Call { callee, args, .. } => {
            assert_eq!(args.len(), 2);
            assert!(matches!(args[0].value.kind, HirExprKind::Variable(_)));
            assert!(matches!(args[1].value.kind, HirExprKind::Variable(_)));
            assert!(matches!(
                callee.kind,
                HirExprKind::GenericApply { ref callee, ref arguments }
                    if matches!(callee.kind, HirExprKind::Path(ref path) if path.to_string() == "refresh")
                        && arguments.len() == 1
            ));
        }
        _ => panic!("expected turbofish method call"),
    }
}

#[test]
fn lowers_parent_slot_method_call_as_slot_access_plus_method_call() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 53 });
    let module = compiler
        .compile_source(
            r#"
class Display {
    micro show() {
    }
}

class Document(rename: Display) {
    micro render() {
        self.rename.show();
    }
}
"#,
        )
        .unwrap();

    let HirStatementKind::Expr(expression) = &module.structs[1].methods[0].body.statements[0].kind
    else {
        panic!("expected expression statement");
    };

    match &expression.kind {
        HirExprKind::Call { callee, args, .. } => {
            assert_eq!(args.len(), 1);
            assert!(matches!(
                callee.kind,
                HirExprKind::Path(ref path) if path.to_string() == "show"
            ));
            assert!(matches!(
                args[0].value.kind,
                HirExprKind::FieldAccess { ref object, ref field }
                    if field.as_str() == "rename"
                        && matches!(object.kind, HirExprKind::Variable(_))
            ));
        }
        _ => panic!("expected renamed parent method call"),
    }
}

#[test]
fn lowers_parent_slot_turbofish_call_as_slot_access_plus_method_call() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 59 });
    let module = compiler
        .compile_source(
            r#"
class Reader {
    micro read(value: i64) {
    }
}

class Hybrid(reader: Reader) {
    micro consume(value: i64) {
        self.reader.read::<i64>(value);
    }
}
"#,
        )
        .unwrap();

    let HirStatementKind::Expr(expression) = &module.structs[1].methods[0].body.statements[0].kind
    else {
        panic!("expected expression statement");
    };

    match &expression.kind {
        HirExprKind::Call { callee, args, .. } => {
            assert_eq!(args.len(), 2);
            assert!(matches!(args[1].value.kind, HirExprKind::Variable(_)));
            assert!(matches!(
                callee.kind,
                HirExprKind::GenericApply { ref callee, ref arguments }
                    if matches!(callee.kind, HirExprKind::Path(ref path) if path.to_string() == "read")
                        && arguments.len() == 1
            ));
            assert!(matches!(
                args[0].value.kind,
                HirExprKind::FieldAccess { ref object, ref field }
                    if field.as_str() == "reader"
                        && matches!(object.kind, HirExprKind::Variable(_))
            ));
        }
        _ => panic!("expected renamed parent turbofish method call"),
    }
}

#[test]
fn lowers_parent_slot_name_from_alias_or_type_name() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 61 });
    let module = compiler
        .compile_source(
            r#"
class Mixed(primary: Teacher, BaseWidget) {
}
"#,
        )
        .unwrap();

    assert_eq!(module.structs[0].parents.len(), 2);
    assert_eq!(module.structs[0].parents[0].slot_name().as_str(), "primary");
    assert_eq!(module.structs[0].parents[1].slot_name().as_str(), "base_widget");
}

#[test]
fn lowers_unite_declaration_into_hir_enum_family() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 67 });
    let module = compiler
        .compile_source(
            r#"
unite Option {
    Some {
        value: i64,
    }
    None
}
"#,
        )
        .unwrap();

    assert_eq!(module.enums.len(), 1);
    let option = &module.enums[0];
    assert!(option.is_unity());
    assert_eq!(option.name.as_str(), "Option");
    assert_eq!(option.variants.len(), 2);
    assert_eq!(option.variants[0].name.as_str(), "Some");
    assert_eq!(option.variants[0].fields.len(), 1);
    assert_eq!(option.variants[1].name.as_str(), "None");
    assert!(option.variants[1].fields.is_empty());
}

#[test]
fn lowers_trait_associated_types_into_hir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 81 });
    let module = compiler
        .compile_source(
            r#"
trait Iterator<T>: Display + Clone {
    type Item
    type Output = T
    const Limit: i64 = 42

    micro next(self) -> Self::Item
    micro collect(self) -> T {
        return self;
    }
}
"#,
        )
        .unwrap();

    assert_eq!(module.traits.len(), 1);
    let trait_def = &module.traits[0];
    assert_eq!(trait_def.name.as_str(), "Iterator");
    assert_eq!(trait_def.super_traits.len(), 2);
    assert_eq!(trait_def.associated_types.len(), 2);
    assert_eq!(trait_def.associated_constants.len(), 1);
    assert_eq!(trait_def.associated_types[0].name.as_str(), "Item");
    assert!(trait_def.associated_types[0].default.is_none());
    assert_eq!(trait_def.associated_types[1].name.as_str(), "Output");
    assert!(matches!(trait_def.associated_types[1].default, Some(ValkyrieType::Named(ref name)) if name.as_str() == "T"));
    assert_eq!(trait_def.associated_constants[0].name.as_str(), "Limit");
    assert_eq!(trait_def.associated_constants[0].const_type, ValkyrieType::Integer64 { signed: true });
    assert!(matches!(
        trait_def.associated_constants[0].default_value.as_ref(),
        Some(expr) if matches!(expr.kind, nyar_language::types::hir::HirExprKind::Literal(nyar_language::types::hir::HirLiteral::Integer64(42)))
    ));
    assert_eq!(trait_def.methods.len(), 1);
    assert_eq!(trait_def.default_methods.len(), 1);
}

#[test]
fn lowers_imply_blocks_into_hir_impls() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 83 });
    let module = compiler
        .compile_source(
            r#"
imply<T: Clone> Buffer<T>: Iterator
where T: Display {
    type Item = T
    const SIZE: i64 = 1

    micro next(self) -> T {
        return self.value;
    }
}

imply Point {
    micro length(self) -> i64 {
        return self.x;
    }
}
"#,
        )
        .unwrap();

    assert_eq!(module.impls.len(), 2);

    let trait_impl = &module.impls[0];
    assert!(matches!(trait_impl.target, ValkyrieType::Apply(_, _)));
    assert!(matches!(trait_impl.trait_path.as_ref(), Some(path) if path.to_string() == "Iterator"));
    assert_eq!(trait_impl.generics.len(), 1);
    assert_eq!(trait_impl.generics[0].name.as_str(), "T");
    assert_eq!(trait_impl.generics[0].bounds.len(), 1);
    assert_eq!(trait_impl.where_constraints.len(), 1);
    assert!(matches!(trait_impl.where_constraints[0].target, ValkyrieType::Named(ref name) if name.as_str() == "T"));
    assert_eq!(trait_impl.where_constraints[0].bounds.len(), 1);
    assert_eq!(trait_impl.where_constraints[0].bounds[0].to_string(), "Display");
    assert_eq!(trait_impl.methods.len(), 1);
    assert_eq!(trait_impl.associated_type_impls.len(), 1);
    assert_eq!(trait_impl.associated_const_impls.len(), 1);
    assert_eq!(trait_impl.associated_type_impls[0].name.as_str(), "Item");
    assert!(matches!(trait_impl.associated_type_impls[0].concrete_type, ValkyrieType::Named(ref name) if name.as_str() == "T"));
    assert_eq!(trait_impl.associated_const_impls[0].name.as_str(), "SIZE");
    assert_eq!(trait_impl.associated_const_impls[0].const_type, Some(ValkyrieType::Integer64 { signed: true }));

    let inherent_impl = &module.impls[1];
    assert!(matches!(inherent_impl.target, ValkyrieType::Named(ref name) if name.as_str() == "Point"));
    assert!(inherent_impl.trait_path.is_none());
    assert!(inherent_impl.where_constraints.is_empty());
    assert_eq!(inherent_impl.methods.len(), 1);
    assert!(inherent_impl.associated_type_impls.is_empty());
}

#[test]
fn lowers_enums_and_flags_declarations_into_hir() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 9201 });
    let module = compiler
        .compile_source(
            r#"
enums Color {
    Red = 2
    Green = 4
}

flags FilePerm {
    Read = 1
    Write = 2
}
"#,
        )
        .unwrap();
    assert_eq!(module.enums.len(), 1);
    assert_eq!(module.enums[0].name.as_str(), "Color");
    assert!(!module.enums[0].is_unity);
    assert_eq!(module.enums[0].variants.len(), 2);
    assert!(module.enums[0].variants[0].discriminator.is_some());
    assert!(module.enums[0].variants[1].discriminator.is_some());
    assert_eq!(module.flags.len(), 1);
    assert_eq!(module.flags[0].name.as_str(), "FilePerm");
    assert_eq!(module.flags[0].members.len(), 2);

    let (sum_types, _) = nyar_language::compute_nominal_layouts(&module);
    let color = sum_types.iter().find(|item| item.name == "Color").expect("Color layout");
    assert_eq!(color.variants[0].tag, 2);
    assert_eq!(color.variants[1].tag, 4);
}

#[test]
fn lowers_enums_with_implicit_discriminator_increment() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 9203 });
    let module = compiler
        .compile_source(
            r#"
enums Status {
    Active = 0
    Inactive
}
"#,
        )
        .unwrap();
    let (sum_types, _) = nyar_language::compute_nominal_layouts(&module);
    let status = sum_types.iter().find(|item| item.name == "Status").expect("Status layout");
    assert_eq!(status.variants[0].tag, 0);
    assert_eq!(status.variants[1].tag, 1);
}

#[test]
fn rejects_tag_attribute_on_enums_declaration() {
    let error = ValkyrieCompiler::new(SourceID { version_id: 9204 })
        .compile_source(
            r#"
[tag(KindTag)]
enums Kind {
    A
    B
}
"#,
        )
        .expect_err("tag on enums");
    let message = error.to_string();
    assert!(message.contains("`[tag(...)]` is only valid on `unite`"), "got {message}");
    assert!(message.contains("Variant = N") || message.contains("`enums"), "got {message}");
}

#[test]
fn rejects_tag_attribute_on_enums_variant() {
    let error = ValkyrieCompiler::new(SourceID { version_id: 9205 })
        .compile_source(
            r#"
enums Kind {
    [tag(0)]
    A
    B
}
"#,
        )
        .expect_err("tag on enums variant");
    let message = error.to_string();
    assert!(message.contains("`[tag(...)]` is only valid on `unite`"), "got {message}");
    assert!(message.contains("A = N") || message.contains("write `A = N`"), "got {message}");
}

#[test]
fn lowers_enums_with_pure_auto_increment_from_zero() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 9206 });
    let module = compiler
        .compile_source(
            r#"
enums VonTokenKind {
    Identifier
    StringLiteral
    EndOfFile
}
"#,
        )
        .unwrap();
    let (sum_types, _) = nyar_language::compute_nominal_layouts(&module);
    let kind = sum_types.iter().find(|item| item.name == "VonTokenKind").expect("VonTokenKind layout");
    assert_eq!(kind.variants[0].tag, 0);
    assert_eq!(kind.variants[1].tag, 1);
    assert_eq!(kind.variants[2].tag, 2);
}

#[test]
fn lowers_generic_enums_into_hir_generics() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 9202 });
    let module = compiler
        .compile_source(
            r#"
enums Box<T> {
    Full { value: T }
    Empty
}
"#,
        )
        .unwrap();
    assert_eq!(module.enums.len(), 1);
    assert_eq!(module.enums[0].name.as_str(), "Box");
    assert_eq!(module.enums[0].generics.len(), 1);
    assert_eq!(module.enums[0].generics[0].name.as_str(), "T");
}

#[test]
fn lowers_mezzo_and_macro_assign_into_type_functions() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 9202 });
    let module = compiler
        .compile_source(
            r#"
mezzo BoxOf(T: type) -> type {
    T
}

macro double(x) = x + x
"#,
        )
        .unwrap();
    assert_eq!(module.type_functions.len(), 2);
    assert!(module.type_functions.iter().any(|item| item.name.as_str() == "BoxOf"));
    assert!(module.type_functions.iter().any(|item| item.name.as_str() == "double"));
    let double = module.type_functions.iter().find(|item| item.name.as_str() == "double").expect("double macro");
    assert!(double.body.expr.is_some());
}
