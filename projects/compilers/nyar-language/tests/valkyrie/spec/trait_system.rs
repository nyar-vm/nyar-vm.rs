use std::collections::BTreeMap;

use nyar_language::{
    ValkyrieCompiler,
    types::{
        Identifier, NamePath, SourceID, SourceSpan,
        hir::{
            HirAssociatedType, HirAssociatedTypeImpl, HirBlock, HirDocumentation, HirField, HirFunction, HirIdentifier, HirImpl, HirModule,
            HirParam, HirStruct, HirTrait, HirVisibility, ValkyrieType,
        },
        witness::{MethodPath, ModuleId, TraitId, TypeId, TypeKind, TypeMetadata},
    },
    valkyrie::trait_system::{
        NamedTraitWitness, TraitMethodBinding, TraitMethodBindingSource, TraitModuleError, TraitModuleView, TraitSatisfactionError,
        TraitWitnessSource, build_witness_method_entries, build_witness_table, satisfy_named_trait,
    },
};

#[test]
fn named_trait_satisfaction_produces_witness() {
    let iterator_trait = trait_with_assoc_type(
        "Iterator",
        vec![method("next", vec![], ValkyrieType::Named(Identifier::new("Option")))],
        vec![HirAssociatedType::new(Identifier::new("Item"), span())],
    );
    let counter = struct_with_methods("Counter", vec![method("next", vec![], ValkyrieType::Named(Identifier::new("Option")))]);
    let explicit_impl = HirImpl {
        methods: vec![method("next", vec![], ValkyrieType::Named(Identifier::new("Option")))],
        ..trait_impl("Counter", "Iterator", vec![HirAssociatedTypeImpl::new(Identifier::new("Item"), int32(), span())])
    };

    let witness = satisfy_named_trait(&counter, &iterator_trait, &[explicit_impl]).unwrap();

    assert_eq!(
        witness,
        NamedTraitWitness {
            trait_path: NamePath::new(vec![Identifier::new("Iterator")]),
            target: ValkyrieType::Named(Identifier::new("Counter")),
            source: TraitWitnessSource::ExplicitImpl,
            method_bindings: vec![TraitMethodBinding {
                name: Identifier::new("next"),
                source: TraitMethodBindingSource::ExplicitImpl,
                implementation_container: Identifier::new("Counter"),
                is_default: false,
            }],
            associated_types: BTreeMap::from([(Identifier::new("Item"), int32())]),
        }
    );
}

#[test]
fn explicit_impl_beats_structural_witness() {
    let writer_trait = trait_with_methods("Writer", vec![method("write", vec![ValkyrieType::Utf8], ValkyrieType::Unit)]);
    let console = struct_with_methods("Console", vec![method("write", vec![ValkyrieType::Utf8], ValkyrieType::Unit)]);
    let explicit_impl =
        HirImpl { methods: vec![method("write", vec![ValkyrieType::Utf8], ValkyrieType::Unit)], ..trait_impl("Console", "Writer", vec![]) };

    let witness = satisfy_named_trait(&console, &writer_trait, &[explicit_impl]).unwrap();

    assert_eq!(witness.source, TraitWitnessSource::ExplicitImpl);
}

#[test]
fn trait_module_view_resolves_named_trait_against_real_module_objects() {
    let writer_trait = trait_with_methods("Writer", vec![method("write", vec![ValkyrieType::Utf8], ValkyrieType::Unit)]);
    let console = struct_with_methods("Console", vec![method("write", vec![ValkyrieType::Utf8], ValkyrieType::Unit)]);
    let module = module_with_traits(vec![writer_trait], vec![console], vec![]);

    let witness = TraitModuleView::from_module(&module).satisfy_named_trait(&Identifier::new("Console"), &Identifier::new("Writer")).unwrap();

    assert_eq!(witness.source, TraitWitnessSource::Structural);
}

#[test]
fn trait_module_view_reports_unknown_trait_from_real_module_objects() {
    let console = struct_with_methods("Console", vec![method("write", vec![ValkyrieType::Utf8], ValkyrieType::Unit)]);
    let module = module_with_traits(vec![], vec![console], vec![]);

    let error = TraitModuleView::from_module(&module).satisfy_named_trait(&Identifier::new("Console"), &Identifier::new("Writer")).unwrap_err();

    assert_eq!(error, TraitModuleError::UnknownTrait { name: Identifier::new("Writer") });
}

#[test]
fn trait_module_view_reads_real_hir_module_objects_from_compiler() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 37 });
    let module = compiler
        .compile_source(
            r#"trait Writer {
    micro write(text: utf8);
}
class Console {
    micro write(text: utf8) {}
}
"#,
        )
        .unwrap();

    let witness = TraitModuleView::from_module(&module).satisfy_named_trait(&Identifier::new("Console"), &Identifier::new("Writer")).unwrap();

    assert_eq!(witness.source, TraitWitnessSource::Structural);
}

#[test]
fn trait_module_view_requires_transitive_super_trait_impl_chain() {
    let readable = trait_with_methods("Readable", vec![method("read", vec![], ValkyrieType::Utf8)]);
    let seekable = HirTrait {
        super_traits: vec![ValkyrieType::Named(Identifier::new("Readable"))],
        ..trait_with_methods("Seekable", vec![method("seek", vec![], ValkyrieType::Unit)])
    };
    let buffered = HirTrait {
        super_traits: vec![ValkyrieType::Named(Identifier::new("Seekable"))],
        ..trait_with_methods("BufferedReadable", vec![method("fill_buf", vec![], ValkyrieType::Utf8)])
    };
    let reader = struct_with_methods(
        "Reader",
        vec![
            method("read", vec![], ValkyrieType::Utf8),
            method("seek", vec![], ValkyrieType::Unit),
            method("fill_buf", vec![], ValkyrieType::Utf8),
        ],
    );
    let seekable_impl = HirImpl { methods: vec![method("seek", vec![], ValkyrieType::Unit)], ..trait_impl("Reader", "Seekable", vec![]) };
    let buffered_impl =
        HirImpl { methods: vec![method("fill_buf", vec![], ValkyrieType::Utf8)], ..trait_impl("Reader", "BufferedReadable", vec![]) };
    let module = module_with_traits(vec![readable, seekable, buffered], vec![reader], vec![seekable_impl, buffered_impl]);

    let error = TraitModuleView::from_module(&module)
        .satisfy_named_trait(&Identifier::new("Reader"), &Identifier::new("BufferedReadable"))
        .unwrap_err();

    assert_eq!(
        error,
        TraitModuleError::Satisfaction {
            error: TraitSatisfactionError::MissingExplicitImplForSuperTraits { trait_path: NamePath::new(vec![Identifier::new("Readable")]) },
        }
    );
}

#[test]
fn trait_module_view_accepts_transitive_super_trait_impl_chain() {
    let readable = trait_with_methods("Readable", vec![method("read", vec![], ValkyrieType::Utf8)]);
    let seekable = HirTrait {
        super_traits: vec![ValkyrieType::Named(Identifier::new("Readable"))],
        ..trait_with_methods("Seekable", vec![method("seek", vec![], ValkyrieType::Unit)])
    };
    let buffered = HirTrait {
        super_traits: vec![ValkyrieType::Named(Identifier::new("Seekable"))],
        ..trait_with_methods("BufferedReadable", vec![method("fill_buf", vec![], ValkyrieType::Utf8)])
    };
    let reader = struct_with_methods(
        "Reader",
        vec![
            method("read", vec![], ValkyrieType::Utf8),
            method("seek", vec![], ValkyrieType::Unit),
            method("fill_buf", vec![], ValkyrieType::Utf8),
        ],
    );
    let readable_impl = HirImpl { methods: vec![method("read", vec![], ValkyrieType::Utf8)], ..trait_impl("Reader", "Readable", vec![]) };
    let seekable_impl = HirImpl { methods: vec![method("seek", vec![], ValkyrieType::Unit)], ..trait_impl("Reader", "Seekable", vec![]) };
    let buffered_impl =
        HirImpl { methods: vec![method("fill_buf", vec![], ValkyrieType::Utf8)], ..trait_impl("Reader", "BufferedReadable", vec![]) };
    let module = module_with_traits(vec![readable, seekable, buffered], vec![reader], vec![readable_impl, seekable_impl, buffered_impl]);

    let witness =
        TraitModuleView::from_module(&module).satisfy_named_trait(&Identifier::new("Reader"), &Identifier::new("BufferedReadable")).unwrap();

    assert_eq!(witness.source, TraitWitnessSource::ExplicitImpl);
    assert_eq!(witness.trait_path, NamePath::new(vec![Identifier::new("BufferedReadable")]));
}

#[test]
fn ambiguous_explicit_impls_do_not_fall_back_to_structural_entry() {
    let writer_trait = trait_with_methods("Writer", vec![method("write", vec![ValkyrieType::Utf8], ValkyrieType::Unit)]);
    let console = struct_with_methods("Console", vec![method("write", vec![ValkyrieType::Utf8], ValkyrieType::Unit)]);
    let first_impl = trait_impl("Console", "Writer", vec![]);
    let second_impl = trait_impl("Console", "Writer", vec![]);

    let error = satisfy_named_trait(&console, &writer_trait, &[first_impl, second_impl]).unwrap_err();

    assert_eq!(
        error,
        TraitSatisfactionError::AmbiguousExplicitImpls {
            trait_path: NamePath::new(vec![Identifier::new("Writer")]),
            target: ValkyrieType::Named(Identifier::new("Console")),
        }
    );
}

#[test]
fn same_shape_traits_do_not_merge_identity() {
    let trait_a = trait_with_methods("Readable", vec![method("read", vec![], ValkyrieType::Utf8)]);
    let trait_b = trait_with_methods("StringSource", vec![method("read", vec![], ValkyrieType::Utf8)]);
    let file = struct_with_methods("FileReader", vec![method("read", vec![], ValkyrieType::Utf8)]);

    let witness_a = satisfy_named_trait(&file, &trait_a, &[]).unwrap();
    let witness_b = satisfy_named_trait(&file, &trait_b, &[]).unwrap();

    assert_eq!(witness_a.source, TraitWitnessSource::Structural);
    assert_eq!(witness_b.source, TraitWitnessSource::Structural);
    assert_ne!(witness_a.trait_path, witness_b.trait_path);
    assert_ne!(witness_a, witness_b);
}

#[test]
fn trait_with_associated_types_requires_named_binding() {
    let iterator_trait = trait_with_assoc_type(
        "Iterator",
        vec![method("next", vec![], ValkyrieType::Named(Identifier::new("Option")))],
        vec![HirAssociatedType::new(Identifier::new("Item"), span())],
    );
    let counter = struct_with_methods("Counter", vec![method("next", vec![], ValkyrieType::Named(Identifier::new("Option")))]);

    let error = satisfy_named_trait(&counter, &iterator_trait, &[]).unwrap_err();

    assert_eq!(
        error,
        TraitSatisfactionError::MissingExplicitImplForAssociatedTypes { trait_path: NamePath::new(vec![Identifier::new("Iterator")]) }
    );
}

#[test]
fn trait_structural_entry_rejects_ambiguous_row_match() {
    let writer_trait = trait_with_methods("Writer", vec![method("write", vec![ValkyrieType::Utf8], ValkyrieType::Unit)]);
    let overloaded = struct_with_methods(
        "OverloadedWriter",
        vec![method("write", vec![ValkyrieType::Utf8], ValkyrieType::Unit), method("write", vec![int32()], ValkyrieType::Unit)],
    );

    let error = satisfy_named_trait(&overloaded, &writer_trait, &[]).unwrap_err();

    assert_eq!(
        error,
        TraitSatisfactionError::StructuralAmbiguity {
            errors: vec![nyar_language::valkyrie::row::RowRequirementError::AmbiguousCandidateMethod { name: Identifier::new("write") }],
        }
    );
}

#[test]
fn trait_with_super_traits_requires_explicit_impl() {
    let child_trait = HirTrait {
        super_traits: vec![ValkyrieType::Named(Identifier::new("Readable"))],
        ..trait_with_methods("BufferedReadable", vec![method("fill_buf", vec![], ValkyrieType::Utf8)])
    };
    let reader =
        struct_with_methods("Reader", vec![method("read", vec![], ValkyrieType::Utf8), method("fill_buf", vec![], ValkyrieType::Utf8)]);

    let error = satisfy_named_trait(&reader, &child_trait, &[]).unwrap_err();

    assert_eq!(
        error,
        TraitSatisfactionError::MissingExplicitImplForSuperTraits { trait_path: NamePath::new(vec![Identifier::new("BufferedReadable")]) }
    );
}

#[test]
fn trait_default_methods_do_not_become_structural_requirements() {
    let buffered = HirTrait {
        default_methods: vec![method("close", vec![], ValkyrieType::Unit)],
        ..trait_with_methods("BufferedWriter", vec![method("write", vec![ValkyrieType::Utf8], ValkyrieType::Unit)])
    };
    let writer = struct_with_methods("ConsoleWriter", vec![method("write", vec![ValkyrieType::Utf8], ValkyrieType::Unit)]);

    let witness = satisfy_named_trait(&writer, &buffered, &[]).unwrap();

    assert_eq!(witness.source, TraitWitnessSource::Structural);
    assert_eq!(witness.trait_path, NamePath::new(vec![Identifier::new("BufferedWriter")]));
    assert_eq!(
        witness.method_bindings,
        vec![
            TraitMethodBinding {
                name: Identifier::new("write"),
                source: TraitMethodBindingSource::Structural,
                implementation_container: Identifier::new("ConsoleWriter"),
                is_default: false,
            },
            TraitMethodBinding {
                name: Identifier::new("close"),
                source: TraitMethodBindingSource::DefaultMethod,
                implementation_container: Identifier::new("BufferedWriter"),
                is_default: true,
            },
        ]
    );
}

#[test]
fn public_field_satisfies_named_trait_through_getter_and_setter_rows() {
    let sized_trait =
        trait_with_methods("SizedSlot", vec![method("get_size", vec![], int32()), method("set_size", vec![int32()], ValkyrieType::Unit)]);
    let slot = struct_with_fields(
        "Slot",
        vec![HirField {
            name: Identifier::new("size"),
            doc: HirDocumentation::default(),
            ty: int32(),
            visibility: HirVisibility::public(),
            is_mutable: true,
        }],
    );

    let witness = satisfy_named_trait(&slot, &sized_trait, &[]).unwrap();

    assert_eq!(witness.source, TraitWitnessSource::Structural);
    assert_eq!(witness.trait_path, NamePath::new(vec![Identifier::new("SizedSlot")]));
}

#[test]
fn explicit_impl_can_fall_back_to_trait_default_method_binding() {
    let comparable = HirTrait {
        default_methods: vec![method("ne", vec![int32()], ValkyrieType::Boolean)],
        ..trait_with_methods("Comparable", vec![method("eq", vec![int32()], ValkyrieType::Boolean)])
    };
    let value = struct_with_methods("NumberBox", vec![method("eq", vec![int32()], ValkyrieType::Boolean)]);
    let explicit_impl =
        HirImpl { methods: vec![method("eq", vec![int32()], ValkyrieType::Boolean)], ..trait_impl("NumberBox", "Comparable", vec![]) };

    let witness = satisfy_named_trait(&value, &comparable, &[explicit_impl]).unwrap();

    assert_eq!(witness.source, TraitWitnessSource::ExplicitImpl);
    assert_eq!(
        witness.method_bindings,
        vec![
            TraitMethodBinding {
                name: Identifier::new("eq"),
                source: TraitMethodBindingSource::ExplicitImpl,
                implementation_container: Identifier::new("NumberBox"),
                is_default: false,
            },
            TraitMethodBinding {
                name: Identifier::new("ne"),
                source: TraitMethodBindingSource::DefaultMethod,
                implementation_container: Identifier::new("Comparable"),
                is_default: true,
            },
        ]
    );
}

#[test]
fn explicit_impl_requires_required_method_binding() {
    let comparable = trait_with_methods("Comparable", vec![method("eq", vec![int32()], ValkyrieType::Boolean)]);
    let value = struct_with_methods("NumberBox", vec![]);
    let explicit_impl = trait_impl("NumberBox", "Comparable", vec![]);

    let error = satisfy_named_trait(&value, &comparable, &[explicit_impl]).unwrap_err();

    assert_eq!(
        error,
        TraitSatisfactionError::MissingMethodBinding {
            trait_path: NamePath::new(vec![Identifier::new("Comparable")]),
            name: Identifier::new("eq"),
        }
    );
}

#[test]
fn operator_method_binding_uses_plain_method_name_in_witness_entries() {
    let additive = trait_with_methods("Add", vec![method("infix +", vec![int32()], int32())]);
    let vector = struct_with_methods("Vec2", vec![method("infix +", vec![int32()], int32())]);
    let explicit_impl = HirImpl { methods: vec![method("infix +", vec![int32()], int32())], ..trait_impl("Vec2", "Add", vec![]) };

    let witness = satisfy_named_trait(&vector, &additive, &[explicit_impl]).unwrap();
    let method_entries = build_witness_method_entries(&witness, ModuleId::LOCAL, vec![Identifier::new("math")], TypeId::new(7));

    assert_eq!(witness.method_bindings[0].name, Identifier::new("infix +"));
    assert_eq!(method_entries.len(), 1);
    assert_eq!(method_entries[0].name, Identifier::new("infix +"));
    assert_eq!(
        method_entries[0].implementation_path,
        MethodPath::new(vec![Identifier::new("math")], Identifier::new("Vec2"), Identifier::new("infix +"))
    );
    assert!(!method_entries[0].is_default);
}

#[test]
fn witness_table_preserves_operator_and_default_method_bindings() {
    let additive = HirTrait {
        default_methods: vec![method("prefix -", vec![int32()], int32())],
        ..trait_with_assoc_type(
            "Add",
            vec![method("infix +", vec![int32()], int32())],
            vec![HirAssociatedType::new(Identifier::new("Output"), span())],
        )
    };
    let vector = struct_with_methods("Vec2", vec![method("infix +", vec![int32()], int32()), method("prefix -", vec![int32()], int32())]);
    let explicit_impl = HirImpl {
        methods: vec![method("infix +", vec![int32()], int32())],
        ..trait_impl("Vec2", "Add", vec![HirAssociatedTypeImpl::new(Identifier::new("Output"), int32(), span())])
    };

    let witness = satisfy_named_trait(&vector, &additive, &[explicit_impl]).unwrap();
    let table = build_witness_table(
        &witness,
        TraitId::new(11),
        ModuleId::LOCAL,
        vec![Identifier::new("math")],
        TypeMetadata::new(TypeId::new(7), Identifier::new("Vec2"), TypeKind::Struct),
        |_, ty| match ty {
            ValkyrieType::Integer32 { signed: _ } => TypeId::new(32),
            _ => TypeId::new(999),
        },
    );

    assert_eq!(table.method_count(), 2);
    assert_eq!(
        table.find_method_by_name(&Identifier::new("infix +")).unwrap().implementation_path,
        MethodPath::new(vec![Identifier::new("math")], Identifier::new("Vec2"), Identifier::new("infix +"))
    );
    assert!(!table.find_method_by_name(&Identifier::new("infix +")).unwrap().is_default);
    assert_eq!(
        table.find_method_by_name(&Identifier::new("prefix -")).unwrap().implementation_path,
        MethodPath::new(vec![Identifier::new("math")], Identifier::new("Add"), Identifier::new("prefix -"))
    );
    assert!(table.find_method_by_name(&Identifier::new("prefix -")).unwrap().is_default);
    assert_eq!(table.get_associated_type(&Identifier::new("Output")), Some(TypeId::new(32)));
}

fn trait_with_methods(name: &str, methods: Vec<HirFunction>) -> HirTrait {
    HirTrait {
        name: Identifier::new(name),
        doc: HirDocumentation::default(),
        generics: vec![],
        methods,
        associated_types: vec![],
        associated_constants: vec![],
        super_traits: vec![],
        default_methods: vec![],
        visibility: HirVisibility::public(),
    }
}

fn trait_with_assoc_type(name: &str, methods: Vec<HirFunction>, associated_types: Vec<HirAssociatedType>) -> HirTrait {
    HirTrait { associated_types, ..trait_with_methods(name, methods) }
}

fn struct_with_methods(name: &str, methods: Vec<HirFunction>) -> HirStruct {
    struct_with_fields_and_methods(name, vec![], methods)
}

fn struct_with_fields(name: &str, fields: Vec<HirField>) -> HirStruct {
    struct_with_fields_and_methods(name, fields, vec![])
}

fn struct_with_fields_and_methods(name: &str, fields: Vec<HirField>, methods: Vec<HirFunction>) -> HirStruct {
    HirStruct {
        name: Identifier::new(name),
        namespace: vec![],
        doc: HirDocumentation::default(),
        generics: vec![],
        parents: vec![],
        fields,
        methods,
        properties: vec![],
        visibility: HirVisibility::public(),
        is_value_type: false,
        is_abstract: false,
        is_sealed: false,
        is_final: false,
        is_open: false,
        abstract_methods: vec![],
        abstract_properties: vec![],
        derives: vec![],
    }
}

fn trait_impl(target: &str, trait_name: &str, associated_type_impls: Vec<HirAssociatedTypeImpl>) -> HirImpl {
    HirImpl {
        generics: vec![],
        where_constraints: vec![],
        target: ValkyrieType::Named(Identifier::new(target)),
        trait_path: Some(NamePath::new(vec![Identifier::new(trait_name)])),
        methods: vec![],
        associated_type_impls,
        associated_const_impls: vec![],
    }
}

fn module_with_traits(traits: Vec<HirTrait>, structs: Vec<HirStruct>, impls: Vec<HirImpl>) -> HirModule {
    HirModule {
        name: NamePath::new(vec![Identifier::new("spec")]),
        doc: HirDocumentation::default(),
        imports: vec![],
        warnings: Vec::new(),
        submodules: vec![],
        functions: vec![],
        structs,
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

fn span() -> SourceSpan {
    SourceSpan::new(SourceID::default(), 0, 0)
}

fn int32() -> ValkyrieType {
    ValkyrieType::Integer32 { signed: true }
}
