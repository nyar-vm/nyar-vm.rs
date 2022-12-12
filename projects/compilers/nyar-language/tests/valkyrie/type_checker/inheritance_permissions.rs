use nyar_language::{
    type_checker::*,
    types::{
        Identifier, NamePath,
        hir::{HirDocumentation, HirModule, HirParent, HirStruct},
    },
};

fn empty_module(structs: Vec<HirStruct>) -> HirModule {
    HirModule {
        name: NamePath::new(vec![Identifier::new("test")]),
        doc: HirDocumentation::default(),
        imports: vec![],
        warnings: Vec::new(),
        submodules: vec![],
        functions: vec![],
        structs,
        enums: vec![],
        imported_enums: Vec::new(),
        flags: vec![],
        traits: vec![],
        impls: vec![],
        type_functions: vec![],
        type_families: vec![],
        widgets: vec![],
        type_aliases: Vec::new(),
        singletons: vec![],
        statements: vec![],
    }
}

fn class(name: &str, parents: Vec<HirParent>) -> HirStruct {
    HirStruct { name: Identifier::new(name), parents, ..Default::default() }
}

fn open_class(name: &str) -> HirStruct {
    HirStruct { name: Identifier::new(name), is_open: true, ..Default::default() }
}

fn sealed_class(name: &str, namespace: &[&str]) -> HirStruct {
    HirStruct {
        name: Identifier::new(name),
        namespace: namespace.iter().map(|part| Identifier::new(part)).collect(),
        is_sealed: true,
        ..Default::default()
    }
}

fn final_class(name: &str) -> HirStruct {
    HirStruct { name: Identifier::new(name), is_final: true, ..Default::default() }
}

fn abstract_class(name: &str) -> HirStruct {
    HirStruct { name: Identifier::new(name), is_abstract: true, ..Default::default() }
}

fn parent(name: &str) -> HirParent {
    HirParent::new(NamePath::new(vec![Identifier::new(name)]))
}

fn with_namespace(mut class: HirStruct, namespace: &[&str]) -> HirStruct {
    class.namespace = namespace.iter().map(|part| Identifier::new(part)).collect();
    class
}

#[test]
fn closed_parent_is_blocked() {
    let module = empty_module(vec![class("Closed", vec![]), class("Child", vec![parent("Closed")])]);
    let errors = InheritancePermissionChecker::new().check_module(&module);
    assert_eq!(errors.len(), 1);
    assert!(matches!(errors[0].kind, InheritancePermissionErrorKind::ClosedParent { .. }));
}

#[test]
fn final_parent_is_blocked() {
    let module = empty_module(vec![final_class("FinalBase"), class("Child", vec![parent("FinalBase")])]);
    let errors = InheritancePermissionChecker::new().check_module(&module);
    assert_eq!(errors.len(), 1);
    assert!(matches!(errors[0].kind, InheritancePermissionErrorKind::FinalParent { .. }));
}

#[test]
fn sealed_same_module_is_allowed() {
    let module = empty_module(vec![sealed_class("SealedBase", &[]), class("Child", vec![parent("SealedBase")])]);
    let errors = InheritancePermissionChecker::new().check_module(&module);
    assert!(errors.is_empty());
}

#[test]
fn sealed_cross_module_is_blocked() {
    let module = empty_module(vec![
        sealed_class("SealedBase", &["mod_a"]),
        with_namespace(class("Child", vec![parent("SealedBase")]), &["mod_b"]),
    ]);
    let errors = InheritancePermissionChecker::new().check_module(&module);
    assert_eq!(errors.len(), 1);
    assert!(matches!(errors[0].kind, InheritancePermissionErrorKind::SealedCrossModule { .. }));
}

#[test]
fn open_and_abstract_parents_are_allowed() {
    let module = empty_module(vec![
        open_class("OpenBase"),
        abstract_class("AbstractBase"),
        class("OpenChild", vec![parent("OpenBase")]),
        class("AbstractChild", vec![parent("AbstractBase")]),
    ]);
    let errors = InheritancePermissionChecker::new().check_module(&module);
    assert!(errors.is_empty());
}

#[test]
fn trait_like_unknown_parents_are_skipped() {
    let module = empty_module(vec![class("Impl", vec![parent("Printable")])]);
    let errors = InheritancePermissionChecker::new().check_module(&module);
    assert!(errors.is_empty());
}
