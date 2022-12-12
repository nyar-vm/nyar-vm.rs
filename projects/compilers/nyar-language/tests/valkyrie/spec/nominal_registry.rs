use nyar_language::{
    ValkyrieCompiler,
    types::{Identifier, SourceID},
    valkyrie::hir::{NominalTypeRegistry, variant_constructor_param_types},
};

#[test]
fn registry_indexes_enums_flags_and_unite() {
    let module = ValkyrieCompiler::new(SourceID { version_id: 9401 })
        .compile_source(
            r#"
namespace demo;

enums Color { Red Green Blue }

unite Option<T> { Some { value: T } None }

flags FilePerm { Read = 1 Write = 2 }
"#,
        )
        .expect("compile");
    let registry = NominalTypeRegistry::from_module(&module);
    assert!(registry.is_sum_type(&Identifier::new("Color")));
    assert!(registry.is_sum_type(&Identifier::new("Option")));
    assert!(registry.flags_def(&Identifier::new("FilePerm")).is_some());
    assert_eq!(registry.variant_names(&Identifier::new("Color")).len(), 3);
}

#[test]
fn variant_constructor_uses_record_fields() {
    let module = ValkyrieCompiler::new(SourceID { version_id: 9402 })
        .compile_source(
            r#"
unite Pair {
    First { left: i64, right: i64 }
    Second
}
"#,
        )
        .expect("compile");
    let registry = NominalTypeRegistry::from_module(&module);
    let variant = registry.variant_def(&Identifier::new("Pair"), &Identifier::new("First")).expect("First variant");
    assert_eq!(variant_constructor_param_types(variant).len(), 2);
}

#[test]
fn materialize_unite_injects_structs() {
    let module = ValkyrieCompiler::new(SourceID { version_id: 9403 })
        .compile_source(
            r#"
unite Option<T> { Some { value: T } None }
"#,
        )
        .expect("compile");
    let registry = NominalTypeRegistry::from_module(&module);
    let mut structs = module.structs.clone();
    registry.materialize_unite_structs(&mut structs);
    assert!(structs.iter().any(|item| item.name.as_str() == "Option"));
    assert!(structs.iter().any(|item| item.name.as_str() == "Some"));
    assert!(structs.iter().any(|item| item.name.as_str() == "None"));
    let some_struct = structs.iter().find(|item| item.name.as_str() == "Some").expect("Some struct");
    assert_eq!(some_struct.fields.len(), 1);
}
