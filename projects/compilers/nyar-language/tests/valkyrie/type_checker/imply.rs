use nyar_language::{
    type_checker::{ImplyChecker, ImplyErrorKind},
    types::{
        Identifier, NamePath, SourceID, SourceSpan,
        hir::{
            HirAssociatedConst, HirAssociatedConstImpl, HirAssociatedType, HirAssociatedTypeImpl, HirBlock, HirDocumentation, HirExpr,
            HirExprKind, HirFunction, HirIdentifier, HirImpl, HirLiteral, HirModule, HirParam, HirTrait, HirVisibility, HirWhereConstraint,
            ValkyrieType,
        },
    },
};

#[test]
fn imply_checker_reports_missing_members_and_signature_mismatch() {
    let trait_def = trait_with_contract(
        "Iterator",
        vec![method("next", vec![ValkyrieType::Integer64 { signed: true }], ValkyrieType::Integer64 { signed: true })],
        vec![HirAssociatedType::new(Identifier::new("Item"), span())],
        vec![HirAssociatedConst::new(Identifier::new("SIZE"), ValkyrieType::Integer64 { signed: true }, span())],
    );
    let impl_block = HirImpl {
        generics: vec![],
        where_constraints: vec![where_constraint(ValkyrieType::Named(Identifier::new("T")), vec!["Display"])],
        target: ValkyrieType::Named(Identifier::new("Counter")),
        trait_path: Some(path("Iterator")),
        methods: vec![method("next", vec![ValkyrieType::Boolean], ValkyrieType::Integer64 { signed: true })],
        associated_type_impls: vec![],
        associated_const_impls: vec![],
    };

    let errors = ImplyChecker::new().check_module(&module(vec![trait_def], vec![impl_block]));

    assert!(
        errors.iter().any(
            |error| matches!(error.kind, ImplyErrorKind::MethodSignatureMismatch { ref method, .. } if method == &Identifier::new("next"))
        )
    );
    assert!(
        errors
            .iter()
            .any(|error| matches!(error.kind, ImplyErrorKind::MissingAssociatedType { ref name, .. } if name == &Identifier::new("Item")))
    );
    assert!(
        errors
            .iter()
            .any(|error| matches!(error.kind, ImplyErrorKind::MissingAssociatedConst { ref name, .. } if name == &Identifier::new("SIZE")))
    );
}

#[test]
fn imply_checker_reports_duplicate_trait_impls_when_where_matches() {
    let trait_def = trait_with_contract("Display", vec![method("show", vec![], ValkyrieType::Utf8)], vec![], vec![]);
    let first_impl = HirImpl {
        generics: vec![],
        where_constraints: vec![where_constraint(ValkyrieType::Named(Identifier::new("T")), vec!["Clone"])],
        target: ValkyrieType::Named(Identifier::new("Box")),
        trait_path: Some(path("Display")),
        methods: vec![method("show", vec![], ValkyrieType::Utf8)],
        associated_type_impls: vec![],
        associated_const_impls: vec![],
    };
    let second_impl = HirImpl {
        generics: vec![],
        where_constraints: vec![where_constraint(ValkyrieType::Named(Identifier::new("T")), vec!["Clone"])],
        target: ValkyrieType::Named(Identifier::new("Box")),
        trait_path: Some(path("Display")),
        methods: vec![method("show", vec![], ValkyrieType::Utf8)],
        associated_type_impls: vec![],
        associated_const_impls: vec![],
    };

    let errors = ImplyChecker::new().check_module(&module(vec![trait_def], vec![first_impl, second_impl]));

    assert!(errors.iter().any(|error| matches!(
        error.kind,
        ImplyErrorKind::DuplicateImpl { ref target, trait_name: Some(ref trait_name) }
            if target == &ValkyrieType::Named(Identifier::new("Box")) && trait_name == &path("Display")
    )));
}

#[test]
fn imply_checker_reports_overlapping_trait_impls_when_where_is_incomparable() {
    let trait_def = trait_with_contract("Display", vec![method("show", vec![], ValkyrieType::Utf8)], vec![], vec![]);
    let first_impl = HirImpl {
        generics: vec![],
        where_constraints: vec![where_constraint(ValkyrieType::Named(Identifier::new("T")), vec!["Clone"])],
        target: ValkyrieType::Named(Identifier::new("Box")),
        trait_path: Some(path("Display")),
        methods: vec![method("show", vec![], ValkyrieType::Utf8)],
        associated_type_impls: vec![],
        associated_const_impls: vec![],
    };
    let second_impl = HirImpl {
        generics: vec![],
        where_constraints: vec![where_constraint(ValkyrieType::Named(Identifier::new("T")), vec!["Debug"])],
        target: ValkyrieType::Named(Identifier::new("Box")),
        trait_path: Some(path("Display")),
        methods: vec![method("show", vec![], ValkyrieType::Utf8)],
        associated_type_impls: vec![],
        associated_const_impls: vec![],
    };

    let errors = ImplyChecker::new().check_module(&module(vec![trait_def], vec![first_impl, second_impl]));

    assert!(errors.iter().any(|error| matches!(
        error.kind,
        ImplyErrorKind::OverlappingImpl { ref target, trait_name: Some(ref trait_name) }
            if target == &ValkyrieType::Named(Identifier::new("Box")) && trait_name == &path("Display")
    )));
}

#[test]
fn imply_checker_accepts_more_specific_where_impl() {
    let trait_def = trait_with_contract("Display", vec![method("show", vec![], ValkyrieType::Utf8)], vec![], vec![]);
    let general_impl = HirImpl {
        generics: vec![],
        where_constraints: vec![where_constraint(ValkyrieType::Named(Identifier::new("T")), vec!["Clone"])],
        target: ValkyrieType::Named(Identifier::new("Box")),
        trait_path: Some(path("Display")),
        methods: vec![method("show", vec![], ValkyrieType::Utf8)],
        associated_type_impls: vec![],
        associated_const_impls: vec![],
    };
    let specific_impl = HirImpl {
        generics: vec![],
        where_constraints: vec![where_constraint(ValkyrieType::Named(Identifier::new("T")), vec!["Clone", "Debug"])],
        target: ValkyrieType::Named(Identifier::new("Box")),
        trait_path: Some(path("Display")),
        methods: vec![method("show", vec![], ValkyrieType::Utf8)],
        associated_type_impls: vec![],
        associated_const_impls: vec![],
    };

    let errors = ImplyChecker::new().check_module(&module(vec![trait_def], vec![general_impl, specific_impl]));

    assert!(!errors.iter().any(|error| matches!(error.kind, ImplyErrorKind::DuplicateImpl { .. } | ImplyErrorKind::OverlappingImpl { .. })));
}

#[test]
fn imply_checker_reports_missing_super_trait_impl() {
    let readable = trait_with_contract("Readable", vec![method("read", vec![], ValkyrieType::Utf8)], vec![], vec![]);
    let buffered = HirTrait {
        super_traits: vec![ValkyrieType::Named(Identifier::new("Readable"))],
        ..trait_with_contract("BufferedReadable", vec![method("fill_buf", vec![], ValkyrieType::Utf8)], vec![], vec![])
    };
    let impl_block = HirImpl {
        generics: vec![],
        where_constraints: vec![],
        target: ValkyrieType::Named(Identifier::new("Reader")),
        trait_path: Some(path("BufferedReadable")),
        methods: vec![method("fill_buf", vec![], ValkyrieType::Utf8)],
        associated_type_impls: vec![],
        associated_const_impls: vec![],
    };

    let errors = ImplyChecker::new().check_module(&module(vec![readable, buffered], vec![impl_block]));

    assert!(errors.iter().any(|error| matches!(
        error.kind,
        ImplyErrorKind::MissingSuperTraitImpl { ref trait_name, ref super_trait, ref target }
            if trait_name == &path("BufferedReadable")
                && super_trait == &path("Readable")
                && target == &ValkyrieType::Named(Identifier::new("Reader"))
    )));
}

#[test]
fn imply_checker_accepts_explicit_super_trait_impl_chain() {
    let readable = trait_with_contract("Readable", vec![method("read", vec![], ValkyrieType::Utf8)], vec![], vec![]);
    let buffered = HirTrait {
        super_traits: vec![ValkyrieType::Named(Identifier::new("Readable"))],
        ..trait_with_contract("BufferedReadable", vec![method("fill_buf", vec![], ValkyrieType::Utf8)], vec![], vec![])
    };
    let readable_impl = HirImpl {
        generics: vec![],
        where_constraints: vec![],
        target: ValkyrieType::Named(Identifier::new("Reader")),
        trait_path: Some(path("Readable")),
        methods: vec![method("read", vec![], ValkyrieType::Utf8)],
        associated_type_impls: vec![],
        associated_const_impls: vec![],
    };
    let buffered_impl = HirImpl {
        generics: vec![],
        where_constraints: vec![],
        target: ValkyrieType::Named(Identifier::new("Reader")),
        trait_path: Some(path("BufferedReadable")),
        methods: vec![method("fill_buf", vec![], ValkyrieType::Utf8)],
        associated_type_impls: vec![],
        associated_const_impls: vec![],
    };

    let errors = ImplyChecker::new().check_module(&module(vec![readable, buffered], vec![readable_impl, buffered_impl]));

    assert!(!errors.iter().any(|error| matches!(error.kind, ImplyErrorKind::MissingSuperTraitImpl { .. })));
}

#[test]
fn imply_checker_reports_missing_transitive_super_trait_impl() {
    let readable = trait_with_contract("Readable", vec![method("read", vec![], ValkyrieType::Utf8)], vec![], vec![]);
    let seekable = HirTrait {
        super_traits: vec![ValkyrieType::Named(Identifier::new("Readable"))],
        ..trait_with_contract("Seekable", vec![method("seek", vec![], ValkyrieType::Unit)], vec![], vec![])
    };
    let buffered = HirTrait {
        super_traits: vec![ValkyrieType::Named(Identifier::new("Seekable"))],
        ..trait_with_contract("BufferedReadable", vec![method("fill_buf", vec![], ValkyrieType::Utf8)], vec![], vec![])
    };
    let seekable_impl = HirImpl {
        generics: vec![],
        where_constraints: vec![],
        target: ValkyrieType::Named(Identifier::new("Reader")),
        trait_path: Some(path("Seekable")),
        methods: vec![method("seek", vec![], ValkyrieType::Unit)],
        associated_type_impls: vec![],
        associated_const_impls: vec![],
    };
    let buffered_impl = HirImpl {
        generics: vec![],
        where_constraints: vec![],
        target: ValkyrieType::Named(Identifier::new("Reader")),
        trait_path: Some(path("BufferedReadable")),
        methods: vec![method("fill_buf", vec![], ValkyrieType::Utf8)],
        associated_type_impls: vec![],
        associated_const_impls: vec![],
    };

    let errors = ImplyChecker::new().check_module(&module(vec![readable, seekable, buffered], vec![seekable_impl, buffered_impl]));

    assert!(errors.iter().any(|error| matches!(
        error.kind,
        ImplyErrorKind::MissingSuperTraitImpl { ref trait_name, ref super_trait, ref target }
            if trait_name == &path("BufferedReadable")
                && super_trait == &path("Readable")
                && target == &ValkyrieType::Named(Identifier::new("Reader"))
    )));
}

#[test]
fn imply_checker_reports_unknown_and_duplicate_associated_members() {
    let trait_def = trait_with_contract(
        "Iterator",
        vec![method("next", vec![], ValkyrieType::Integer64 { signed: true })],
        vec![HirAssociatedType::new(Identifier::new("Item"), span())],
        vec![HirAssociatedConst::new(Identifier::new("SIZE"), ValkyrieType::Integer64 { signed: true }, span()).with_default(literal_i64(1))],
    );
    let impl_block = HirImpl {
        generics: vec![],
        where_constraints: vec![],
        target: ValkyrieType::Named(Identifier::new("Counter")),
        trait_path: Some(path("Iterator")),
        methods: vec![method("next", vec![], ValkyrieType::Integer64 { signed: true })],
        associated_type_impls: vec![
            HirAssociatedTypeImpl::new(Identifier::new("Item"), ValkyrieType::Integer64 { signed: true }, span()),
            HirAssociatedTypeImpl::new(Identifier::new("Item"), ValkyrieType::Integer64 { signed: true }, span()),
            HirAssociatedTypeImpl::new(Identifier::new("Extra"), ValkyrieType::Boolean, span()),
        ],
        associated_const_impls: vec![
            associated_const_impl("SIZE", Some(ValkyrieType::Boolean)),
            associated_const_impl("SIZE", Some(ValkyrieType::Integer64 { signed: true })),
            associated_const_impl("UNKNOWN", Some(ValkyrieType::Integer64 { signed: true })),
        ],
    };

    let errors = ImplyChecker::new().check_module(&module(vec![trait_def], vec![impl_block]));

    assert!(
        errors
            .iter()
            .any(|error| matches!(error.kind, ImplyErrorKind::DuplicateAssociatedType { ref name, .. } if name == &Identifier::new("Item")))
    );
    assert!(
        errors
            .iter()
            .any(|error| matches!(error.kind, ImplyErrorKind::UnknownAssociatedType { ref name, .. } if name == &Identifier::new("Extra")))
    );
    assert!(
        errors
            .iter()
            .any(|error| matches!(error.kind, ImplyErrorKind::DuplicateAssociatedConst { ref name, .. } if name == &Identifier::new("SIZE")))
    );
    assert!(
        errors
            .iter()
            .any(|error| matches!(error.kind, ImplyErrorKind::UnknownAssociatedConst { ref name, .. } if name == &Identifier::new("UNKNOWN")))
    );
    assert!(
        errors.iter().any(
            |error| matches!(error.kind, ImplyErrorKind::AssociatedConstTypeMismatch { ref name, .. } if name == &Identifier::new("SIZE"))
        )
    );
}

#[test]
fn imply_checker_reports_associated_const_value_type_mismatch() {
    let trait_def = trait_with_contract(
        "Iterator",
        vec![method("next", vec![], ValkyrieType::Integer64 { signed: true })],
        vec![],
        vec![HirAssociatedConst::new(Identifier::new("SIZE"), ValkyrieType::Integer64 { signed: true }, span())],
    );
    let impl_block = HirImpl {
        generics: vec![],
        where_constraints: vec![],
        target: ValkyrieType::Named(Identifier::new("Counter")),
        trait_path: Some(path("Iterator")),
        methods: vec![method("next", vec![], ValkyrieType::Integer64 { signed: true })],
        associated_type_impls: vec![],
        associated_const_impls: vec![HirAssociatedConstImpl {
            name: Identifier::new("SIZE"),
            const_type: Some(ValkyrieType::Integer64 { signed: true }),
            value: HirExpr { kind: HirExprKind::Literal(HirLiteral::Bool(true)), span: span() },
            span: span(),
        }],
    };

    let errors = ImplyChecker::new().check_module(&module(vec![trait_def], vec![impl_block]));

    assert!(errors.iter().any(|error| matches!(
        error.kind,
        ImplyErrorKind::AssociatedConstValueTypeMismatch { ref name, ref expected, ref found, .. }
            if name == &Identifier::new("SIZE") && expected == &ValkyrieType::Integer64 { signed: true } && found == &ValkyrieType::Boolean
    )));
}

#[test]
fn imply_checker_reports_unknown_trait() {
    let impl_block = HirImpl {
        generics: vec![],
        where_constraints: vec![],
        target: ValkyrieType::Named(Identifier::new("Counter")),
        trait_path: Some(path("MissingTrait")),
        methods: vec![],
        associated_type_impls: vec![],
        associated_const_impls: vec![],
    };

    let errors = ImplyChecker::new().check_module(&module(vec![], vec![impl_block]));

    assert!(
        errors.iter().any(|error| matches!(error.kind, ImplyErrorKind::UnknownTrait { ref trait_name } if trait_name == &path("MissingTrait")))
    );
}

fn trait_with_contract(
    name: &str,
    methods: Vec<HirFunction>,
    associated_types: Vec<HirAssociatedType>,
    associated_constants: Vec<HirAssociatedConst>,
) -> HirTrait {
    HirTrait {
        name: Identifier::new(name),
        doc: HirDocumentation::default(),
        generics: vec![],
        methods,
        associated_types,
        associated_constants,
        super_traits: vec![],
        default_methods: vec![],
        visibility: HirVisibility::public(),
    }
}

fn module(traits: Vec<HirTrait>, impls: Vec<HirImpl>) -> HirModule {
    HirModule {
        name: path("test"),
        doc: HirDocumentation::default(),
        imports: vec![],
        warnings: Vec::new(),
        submodules: vec![],
        functions: vec![],
        structs: vec![],
        enums: vec![],
        imported_enums: Vec::new(),
        flags: vec![],
        traits,
        impls,
        type_functions: vec![],
        type_families: vec![],
        widgets: vec![],
        type_aliases: Vec::new(),
        singletons: vec![],
        statements: vec![],
    }
}

fn method(name: &str, params: Vec<ValkyrieType>, return_type: ValkyrieType) -> HirFunction {
    HirFunction {
        name: Identifier::new(name),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: vec![],
        generics: vec![],
        params: params
            .into_iter()
            .enumerate()
            .map(|(index, ty)| HirParam {
                name: HirIdentifier { name: Identifier::new(&format!("arg{index}")), shadow_index: 0, span: span() },
                ty,
                ..Default::default()
            })
            .collect(),
        return_type,
        body: HirBlock { statements: vec![], expr: None, span: span() },
        span: span(),
        visibility: HirVisibility::public(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    }
}

fn associated_const_impl(name: &str, const_type: Option<ValkyrieType>) -> HirAssociatedConstImpl {
    HirAssociatedConstImpl { name: Identifier::new(name), const_type, value: literal_i64(1), span: span() }
}

fn literal_i64(value: i64) -> HirExpr {
    HirExpr { kind: HirExprKind::Literal(HirLiteral::Integer64(value)), span: span() }
}

fn where_constraint(target: ValkyrieType, bounds: Vec<&str>) -> HirWhereConstraint {
    HirWhereConstraint { target, bounds: bounds.into_iter().map(path).collect(), span: span() }
}

fn path(name: &str) -> NamePath {
    NamePath::new(vec![Identifier::new(name)])
}

fn span() -> SourceSpan {
    SourceSpan::new(SourceID::default(), 0, 0)
}
