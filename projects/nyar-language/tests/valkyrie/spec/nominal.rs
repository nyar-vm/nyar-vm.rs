use std::path::PathBuf;

use nyar_language::{
    SumTypeLayout, ValkyrieCompiler, compute_nominal_layouts,
    types::{
        Identifier, NamePath, SourceID,
        hir::{GenericType, HirDocumentation, HirEnum, HirField, HirKind, HirModule, HirStruct, HirVariant, HirVisibility, ValkyrieType},
    },
    valkyrie::nominal::{
        NominalModuleError, NominalModuleView, UniteCoverageError, UniteDefinitionError, UniteLayout, lower_unite, matches_nominal_parameter,
        validate_unite_definition,
    },
};

#[test]
fn class_parameters_use_nominal_matching() {
    let animal = class("Animal");
    let dog = subclass("Dog", "Animal");
    let corgi = subclass("Corgi", "Dog");
    let declared = vec![animal.clone(), dog.clone(), corgi.clone()];

    assert!(matches_nominal_parameter(&animal, &animal, &declared));
    assert!(matches_nominal_parameter(&dog, &animal, &declared));
    assert!(matches_nominal_parameter(&corgi, &animal, &declared));
    assert!(!matches_nominal_parameter(&animal, &dog, &declared));
}

#[test]
fn nominal_type_is_not_satisfied_by_shape_alone() {
    let animal = class("Animal");
    let robot_dog = class("RobotDog");
    let declared = vec![animal.clone(), robot_dog.clone()];

    assert!(!matches_nominal_parameter(&robot_dog, &animal, &declared));
}

#[test]
fn nominal_cycles_do_not_create_false_subtyping() {
    let mut alpha = class("Alpha");
    let mut beta = class("Beta");
    alpha.parents = vec![nyar_language::types::hir::HirParent::new(NamePath::new(vec![Identifier::new("Beta")]))];
    beta.parents = vec![nyar_language::types::hir::HirParent::new(NamePath::new(vec![Identifier::new("Alpha")]))];
    let gamma = class("Gamma");
    let declared = vec![alpha.clone(), beta.clone(), gamma.clone()];

    assert!(matches_nominal_parameter(&alpha, &beta, &declared));
    assert!(matches_nominal_parameter(&beta, &alpha, &declared));
    assert!(!matches_nominal_parameter(&alpha, &gamma, &declared));
    assert!(!matches_nominal_parameter(&gamma, &alpha, &declared));
}

#[test]
fn unite_defaults_to_abstract_base_with_closed_variants() {
    let option = option_unite();
    let lowered = lower_unite(&option, UniteLayout::Untagged);

    assert_eq!(lowered.base.name, Identifier::new("Option"));
    assert!(lowered.base.is_abstract);
    assert!(lowered.base.is_sealed);
    assert!(!lowered.base.is_open);
    assert_eq!(lowered.layout, UniteLayout::Untagged);

    assert_eq!(lowered.variant_names(), vec![Identifier::new("Some"), Identifier::new("None")]);
    assert!(lowered.variants.iter().all(|variant| variant.is_sealed));
    assert!(lowered.variants.iter().all(|variant| variant.is_final));
    assert_eq!(lowered.variants[0].parents[0].name, NamePath::new(vec![Identifier::new("Option")]));
    assert_eq!(lowered.variants[1].parents[0].name, NamePath::new(vec![Identifier::new("Option")]));
}

#[test]
fn unite_variants_preserve_base_generics_in_parent_edge() {
    let either = generic_unite();
    let lowered = lower_unite(&either, UniteLayout::Untagged);

    assert_eq!(lowered.base.generics, either.generics);
    assert_eq!(lowered.variants.len(), 2);
    assert_eq!(lowered.variants[0].generics, either.generics);
    assert_eq!(lowered.variants[1].generics, either.generics);

    let left_parent = &lowered.variants[0].parents[0];
    assert_eq!(left_parent.name, NamePath::new(vec![Identifier::new("Either")]));
    assert_eq!(
        left_parent.generics,
        vec![
            ValkyrieType::Generic(GenericType { name: Identifier::new("L"), kind: HirKind::Type, bounds: vec![] }),
            ValkyrieType::Generic(GenericType { name: Identifier::new("R"), kind: HirKind::Type, bounds: vec![] }),
        ]
    );
}

#[test]
fn gadt_variant_result_type_refines_parent_generics() {
    let expr = gadt_unite();
    let lowered = lower_unite(&expr, UniteLayout::Tagged);

    assert_eq!(lowered.variants.len(), 2);

    let literal = &lowered.variants[0];
    assert!(literal.generics.is_empty());
    assert_eq!(literal.parents[0].generics, vec![ValkyrieType::Float64]);

    let branch = &lowered.variants[1];
    assert_eq!(branch.generics, vec![GenericType { name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] }]);
    assert_eq!(
        branch.parents[0].generics,
        vec![ValkyrieType::Generic(GenericType { name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] })]
    );
}

#[test]
fn unite_exhaustiveness_is_independent_of_runtime_layout() {
    let option = option_unite();
    let tagged = lower_unite(&option, UniteLayout::Tagged);
    let untagged = lower_unite(&option, UniteLayout::Untagged);
    let complete = vec![Identifier::new("Some"), Identifier::new("None")];
    let incomplete = vec![Identifier::new("Some")];

    assert!(tagged.is_exhaustive_over(&complete));
    assert!(untagged.is_exhaustive_over(&complete));
    assert!(!tagged.is_exhaustive_over(&incomplete));
    assert!(!untagged.is_exhaustive_over(&incomplete));
    assert_eq!(tagged.variant_names(), untagged.variant_names());
}

#[test]
fn unite_coverage_rejects_missing_variants() {
    let option = option_unite();
    let lowered = lower_unite(&option, UniteLayout::Untagged);
    let error = lowered.check_exhaustiveness(&[Identifier::new("Some")]).unwrap_err();

    assert_eq!(error, UniteCoverageError::MissingVariants { names: vec![Identifier::new("None")] });
}

#[test]
fn unite_coverage_rejects_unknown_variants() {
    let option = option_unite();
    let lowered = lower_unite(&option, UniteLayout::Untagged);
    let error = lowered.check_exhaustiveness(&[Identifier::new("Some"), Identifier::new("None"), Identifier::new("Later")]).unwrap_err();

    assert_eq!(error, UniteCoverageError::UnknownVariants { names: vec![Identifier::new("Later")] });
}

#[test]
fn unite_coverage_rejects_duplicate_variants() {
    let option = option_unite();
    let lowered = lower_unite(&option, UniteLayout::Tagged);
    let error = lowered.check_exhaustiveness(&[Identifier::new("Some"), Identifier::new("Some"), Identifier::new("None")]).unwrap_err();

    assert_eq!(error, UniteCoverageError::DuplicateVariants { names: vec![Identifier::new("Some")] });
}

#[test]
fn unite_definition_rejects_empty_family() {
    let empty = HirEnum::new_unity(Identifier::new("Never"));

    let error = validate_unite_definition(&empty).unwrap_err();

    assert_eq!(error, UniteDefinitionError::EmptyVariants);
}

#[test]
fn unite_definition_rejects_duplicate_variant_names() {
    let mut duplicate = HirEnum::new_unity(Identifier::new("Bad"));
    duplicate.visibility = HirVisibility::public();
    duplicate.variants = vec![
        HirVariant { name: Identifier::new("Same"), doc: HirDocumentation::default(), fields: vec![], result_type: None, discriminator: None },
        HirVariant { name: Identifier::new("Same"), doc: HirDocumentation::default(), fields: vec![], result_type: None, discriminator: None },
    ];

    let error = validate_unite_definition(&duplicate).unwrap_err();

    assert_eq!(error, UniteDefinitionError::DuplicateVariants { names: vec![Identifier::new("Same")] });
}

#[test]
fn unite_definition_rejects_variant_result_type_outside_family() {
    let mut expr = gadt_unite();
    expr.variants[0].result_type = Some(ValkyrieType::Named(Identifier::new("Other")));

    let error = validate_unite_definition(&expr).unwrap_err();

    assert_eq!(error, UniteDefinitionError::InvalidVariantResultType { variant: Identifier::new("Literal") });
}

#[test]
fn unite_definition_rejects_variant_result_type_with_undeclared_generic() {
    let mut expr = gadt_unite();
    expr.variants[1].result_type = Some(ValkyrieType::Apply(
        Box::new(ValkyrieType::Named(Identifier::new("Expr"))),
        vec![ValkyrieType::Generic(GenericType { name: Identifier::new("U"), kind: HirKind::Type, bounds: vec![] })],
    ));

    let error = validate_unite_definition(&expr).unwrap_err();

    assert_eq!(error, UniteDefinitionError::InvalidVariantResultType { variant: Identifier::new("If") });
}

#[test]
fn unite_definition_rejects_bare_result_type_for_generic_family() {
    let mut expr = gadt_unite();
    expr.variants[1].result_type = Some(ValkyrieType::Named(Identifier::new("Expr")));

    let error = validate_unite_definition(&expr).unwrap_err();

    assert_eq!(error, UniteDefinitionError::InvalidVariantResultType { variant: Identifier::new("If") });
}

#[test]
fn nominal_module_view_resolves_unite_variants_as_real_named_types() {
    let view = NominalModuleView::from_module(&module_with_unite(option_unite()));

    assert!(view.matches_nominal_parameter(&Identifier::new("Some"), &Identifier::new("Option")).unwrap());
    assert!(view.matches_nominal_parameter(&Identifier::new("None"), &Identifier::new("Option")).unwrap());
}

#[test]
fn nominal_module_view_reads_real_hir_module_objects_from_compiler() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 31 });
    let module = compiler
        .compile_source(
            r#"class Animal {}
class Dog(Animal) {}
"#,
        )
        .unwrap();
    let view = NominalModuleView::from_module(&module);

    assert!(view.matches_nominal_parameter(&Identifier::new("Dog"), &Identifier::new("Animal")).unwrap());
}

#[test]
fn nominal_module_view_reads_real_unite_family_from_compiler() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 33 });
    let module = compiler
        .compile_source(
            r#"unite Option {
    Some {
        value: i64,
    }
    None
}
"#,
        )
        .unwrap();
    let view = NominalModuleView::from_module(&module);

    assert!(view.matches_nominal_parameter(&Identifier::new("Some"), &Identifier::new("Option")).unwrap());
    assert!(view.matches_nominal_parameter(&Identifier::new("None"), &Identifier::new("Option")).unwrap());
}

#[test]
fn nominal_module_view_surfaces_invalid_unite_definition() {
    let mut invalid = gadt_unite();
    invalid.variants[1].result_type = Some(ValkyrieType::Named(Identifier::new("Other")));
    let view = NominalModuleView::from_module(&module_with_unite(invalid));

    let error = view.lower_unite(&Identifier::new("Expr"), UniteLayout::Tagged).unwrap_err();

    assert_eq!(
        error,
        NominalModuleError::InvalidUnite {
            name: Identifier::new("Expr"),
            error: UniteDefinitionError::InvalidVariantResultType { variant: Identifier::new("If") },
        }
    );
}

fn class(name: &str) -> HirStruct {
    HirStruct {
        name: Identifier::new(name),
        namespace: vec![],
        doc: HirDocumentation::default(),
        generics: vec![],
        parents: vec![],
        fields: vec![],
        methods: vec![],
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

fn subclass(name: &str, parent: &str) -> HirStruct {
    let mut class = class(name);
    class.parents = vec![nyar_language::types::hir::HirParent::new(NamePath::new(vec![Identifier::new(parent)]))];
    class
}

fn option_unite() -> HirEnum {
    let mut enum_def = HirEnum::new_unity(Identifier::new("Option"));
    enum_def.visibility = HirVisibility::public();
    enum_def.variants = vec![
        HirVariant {
            name: Identifier::new("Some"),
            doc: HirDocumentation::default(),
            fields: vec![HirField {
                name: Identifier::new("value"),
                doc: HirDocumentation::default(),
                ty: int32(),
                visibility: HirVisibility::public(),
                is_mutable: false,
            }],
            result_type: None,
            discriminator: None,
        },
        HirVariant { name: Identifier::new("None"), doc: HirDocumentation::default(), fields: vec![], result_type: None, discriminator: None },
    ];
    enum_def
}

fn module_with_unite(enum_def: HirEnum) -> HirModule {
    HirModule {
        name: NamePath::new(vec![Identifier::new("spec")]),
        doc: HirDocumentation::default(),
        imports: vec![],
        warnings: Vec::new(),
        submodules: vec![],
        functions: vec![],
        structs: vec![],
        enums: vec![enum_def],
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

fn generic_unite() -> HirEnum {
    let mut enum_def = HirEnum::new_unity(Identifier::new("Either"));
    enum_def.visibility = HirVisibility::public();
    enum_def.generics = vec![
        GenericType { name: Identifier::new("L"), kind: HirKind::Type, bounds: vec![] },
        GenericType { name: Identifier::new("R"), kind: HirKind::Type, bounds: vec![] },
    ];
    enum_def.variants = vec![
        HirVariant {
            name: Identifier::new("Left"),
            doc: HirDocumentation::default(),
            fields: vec![HirField {
                name: Identifier::new("value"),
                doc: HirDocumentation::default(),
                ty: ValkyrieType::Generic(GenericType { name: Identifier::new("L"), kind: HirKind::Type, bounds: vec![] }),
                visibility: HirVisibility::public(),
                is_mutable: false,
            }],
            result_type: None,
            discriminator: None,
        },
        HirVariant {
            name: Identifier::new("Right"),
            doc: HirDocumentation::default(),
            fields: vec![HirField {
                name: Identifier::new("value"),
                doc: HirDocumentation::default(),
                ty: ValkyrieType::Generic(GenericType { name: Identifier::new("R"), kind: HirKind::Type, bounds: vec![] }),
                visibility: HirVisibility::public(),
                is_mutable: false,
            }],
            result_type: None,
            discriminator: None,
        },
    ];
    enum_def
}

fn gadt_unite() -> HirEnum {
    let mut enum_def = HirEnum::new_unity(Identifier::new("Expr"));
    enum_def.visibility = HirVisibility::public();
    enum_def.generics = vec![GenericType { name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] }];
    enum_def.variants = vec![
        HirVariant {
            name: Identifier::new("Literal"),
            doc: HirDocumentation::default(),
            fields: vec![HirField {
                name: Identifier::new("value"),
                doc: HirDocumentation::default(),
                ty: ValkyrieType::Float64,
                visibility: HirVisibility::public(),
                is_mutable: false,
            }],
            result_type: Some(ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Expr"))), vec![ValkyrieType::Float64])),
            discriminator: None,
        },
        HirVariant {
            name: Identifier::new("If"),
            doc: HirDocumentation::default(),
            fields: vec![
                HirField {
                    name: Identifier::new("condition"),
                    doc: HirDocumentation::default(),
                    ty: ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Expr"))), vec![ValkyrieType::Boolean]),
                    visibility: HirVisibility::public(),
                    is_mutable: false,
                },
                HirField {
                    name: Identifier::new("then_branch"),
                    doc: HirDocumentation::default(),
                    ty: ValkyrieType::Apply(
                        Box::new(ValkyrieType::Named(Identifier::new("Expr"))),
                        vec![ValkyrieType::Generic(GenericType { name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] })],
                    ),
                    visibility: HirVisibility::public(),
                    is_mutable: false,
                },
            ],
            result_type: Some(ValkyrieType::Apply(
                Box::new(ValkyrieType::Named(Identifier::new("Expr"))),
                vec![ValkyrieType::Generic(GenericType { name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] })],
            )),
            discriminator: None,
        },
    ];
    enum_def
}

fn int32() -> ValkyrieType {
    ValkyrieType::Integer32 { signed: true }
}

// ---------------------------------------------------------------------------
// SubTask 3.3 / 3.4: core::types 迁移后的 nominal/layout 回归测试
//
// 验收结论：
// - core 归属变化（std.types -> core::types）不应影响运行时布局。
// - 迁移前后对比点：
//   1. 命名空间：std.types -> core::types（唯一变化点）
//   2. tag 值由源码 `[tag(0)]` / `[tag(1, default)]` / `[tag(1)]` 驱动，
//      经 HIR discriminator 进入 SumTypeLayout（不再假装“仅按索引碰巧一致”）。
//   3. variant 结构不变：Some{value:T}/None、Fine{value:T}/Fail{error:E}。
//   4. is_unity 不变：源码使用 `unite` 关键字，HIR 中 is_unity=true。
// ---------------------------------------------------------------------------

/// 返回 `valkyrie.v/projects/core/source/types/` 目录的绝对路径。
fn core_types_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../valkyrie.v/projects/core/source/types")
}

/// 读取并编译 `core/source/types` 下的指定 `.v` 文件，返回 HIR 模块。
fn compile_core_type_file(name: &str) -> HirModule {
    let path = core_types_dir().join(name);
    assert!(path.exists(), "missing core type file: {}", path.display());
    let compiler = ValkyrieCompiler::default();
    compiler.compile_path(&path).unwrap_or_else(|error| panic!("failed to compile {}: {error:?}", path.display()))
}

/// 在 SumTypeLayout 列表中按名称查找指定的 unite 布局。
fn find_sum_layout<'a>(layouts: &'a [SumTypeLayout], name: &str) -> &'a SumTypeLayout {
    layouts.iter().find(|layout| layout.name == name).unwrap_or_else(|| panic!("sum type layout `{name}` should exist"))
}

/// 验证 `core::types::Option` 编译后是 unite（tagged union）类型，且变体结构正确。
///
/// 迁移前（std.types）与迁移后（core::types）均应满足：
/// - `is_unity == true`
/// - 变体顺序：Some, None
/// - Some 携带 `value` 字段，None 无字段
#[test]
fn core_option_file_is_nominal_unite_with_correct_variants() {
    let module = compile_core_type_file("Option.v");

    let option = module
        .enums
        .iter()
        .find(|enum_def| enum_def.name == Identifier::new("Option"))
        .expect("Option unite should be present in core::types::Option.v");

    assert!(option.is_unity, "Option must be a unite (is_unity=true) after migration to core::types");

    let variant_names: Vec<&Identifier> = option.variants.iter().map(|v| &v.name).collect();
    assert_eq!(variant_names, vec![&Identifier::new("Some"), &Identifier::new("None")]);

    let some = &option.variants[0];
    assert_eq!(some.fields.len(), 1);
    assert_eq!(some.fields[0].name, Identifier::new("value"));

    let none = &option.variants[1];
    assert!(none.fields.is_empty(), "None variant must have no fields");
}

/// 验证 `core::types::Result` 编译后是 unite（tagged union）类型，且变体结构正确。
///
/// 迁移前后均应满足：
/// - `is_unity == true`
/// - 变体顺序：Fine, Fail
/// - Fine 携带 `value` 字段，Fail 携带 `error` 字段
#[test]
fn core_result_file_is_nominal_unite_with_correct_variants() {
    let module = compile_core_type_file("Result.v");

    let result = module
        .enums
        .iter()
        .find(|enum_def| enum_def.name == Identifier::new("Result"))
        .expect("Result unite should be present in core::types::Result.v");

    assert!(result.is_unity, "Result must be a unite (is_unity=true) after migration to core::types");

    let variant_names: Vec<&Identifier> = result.variants.iter().map(|v| &v.name).collect();
    assert_eq!(variant_names, vec![&Identifier::new("Fine"), &Identifier::new("Fail")]);

    let fine = &result.variants[0];
    assert_eq!(fine.fields.len(), 1);
    assert_eq!(fine.fields[0].name, Identifier::new("value"));

    let fail = &result.variants[1];
    assert_eq!(fail.fields.len(), 1);
    assert_eq!(fail.fields[0].name, Identifier::new("error"));
}

/// 验证 `core::types::Option` 的 SumTypeLayout tag 值与源码 `[tag(0)]` / `[tag(1, default)]` 一致。
///
/// `[tag(N)]` 写入 HIR discriminator，再进入 SumTypeLayout.tag。
#[test]
fn core_option_sum_layout_tags_match_source_annotations() {
    let module = compile_core_type_file("Option.v");
    let (sum_layouts, _) = compute_nominal_layouts(&module);

    let option_layout = find_sum_layout(&sum_layouts, "Option");

    assert!(option_layout.is_unite, "Option SumTypeLayout must have is_unite=true");
    assert_eq!(option_layout.variants.len(), 2);

    assert_eq!(option_layout.variants[0].name, "Some");
    assert_eq!(option_layout.variants[0].tag, 0, "Some variant must have tag=0, matching [tag(0)] in source");

    assert_eq!(option_layout.variants[1].name, "None");
    assert_eq!(option_layout.variants[1].tag, 1, "None variant must have tag=1, matching [tag(1, default)] in source");
}

/// 验证 `core::types::Result` 的 SumTypeLayout tag 值与源码 `[tag(0)]` / `[tag(1)]` 一致。
///
/// `[tag(N)]` 写入 HIR discriminator，再进入 SumTypeLayout.tag。
#[test]
fn core_result_sum_layout_tags_match_source_annotations() {
    let module = compile_core_type_file("Result.v");
    let (sum_layouts, _) = compute_nominal_layouts(&module);

    let result_layout = find_sum_layout(&sum_layouts, "Result");

    assert!(result_layout.is_unite, "Result SumTypeLayout must have is_unite=true");
    assert_eq!(result_layout.variants.len(), 2);

    assert_eq!(result_layout.variants[0].name, "Fine");
    assert_eq!(result_layout.variants[0].tag, 0, "Fine variant must have tag=0, matching [tag(0)] in source");

    assert_eq!(result_layout.variants[1].name, "Fail");
    assert_eq!(result_layout.variants[1].tag, 1, "Fail variant must have tag=1, matching [tag(1)] in source");
}

/// 验证 `core::types::Option` 经 `lower_unite` 后变体为 sealed/final，基类为 abstract/sealed。
///
/// 迁移前后均应满足：
/// - 基类 Option: is_abstract=true, is_sealed=true, is_open=false
/// - 变体 Some/None: is_sealed=true, is_final=true
/// - 变体的 parent 边指向 Option
#[test]
fn core_option_unite_lowering_preserves_sealed_family() {
    let module = compile_core_type_file("Option.v");
    let option = module.enums.iter().find(|enum_def| enum_def.name == Identifier::new("Option")).expect("Option unite should be present");

    let lowered = lower_unite(option, UniteLayout::Tagged);

    assert_eq!(lowered.base.name, Identifier::new("Option"));
    assert!(lowered.base.is_abstract);
    assert!(lowered.base.is_sealed);
    assert!(!lowered.base.is_open);
    assert_eq!(lowered.layout, UniteLayout::Tagged);

    assert_eq!(lowered.variant_names(), vec![Identifier::new("Some"), Identifier::new("None")]);
    assert!(lowered.variants.iter().all(|variant| variant.is_sealed));
    assert!(lowered.variants.iter().all(|variant| variant.is_final));
    assert_eq!(lowered.variants[0].parents[0].name, NamePath::new(vec![Identifier::new("Option")]));
    assert_eq!(lowered.variants[1].parents[0].name, NamePath::new(vec![Identifier::new("Option")]));
}

/// 验证 `core::types::Result` 经 `lower_unite` 后变体为 sealed/final，基类为 abstract/sealed。
#[test]
fn core_result_unite_lowering_preserves_sealed_family() {
    let module = compile_core_type_file("Result.v");
    let result = module.enums.iter().find(|enum_def| enum_def.name == Identifier::new("Result")).expect("Result unite should be present");

    let lowered = lower_unite(result, UniteLayout::Tagged);

    assert_eq!(lowered.base.name, Identifier::new("Result"));
    assert!(lowered.base.is_abstract);
    assert!(lowered.base.is_sealed);
    assert!(!lowered.base.is_open);

    assert_eq!(lowered.variant_names(), vec![Identifier::new("Fine"), Identifier::new("Fail")]);
    assert!(lowered.variants.iter().all(|variant| variant.is_sealed));
    assert!(lowered.variants.iter().all(|variant| variant.is_final));
}

/// 验证 `core::types::Option` 的穷尽性与运行时布局（Tagged vs Untagged）无关。
///
/// 迁移前后均应满足：
/// - Tagged 和 Untagged 布局下，[Some, None] 均穷尽
/// - Tagged 和 Untagged 布局下，[Some] 均不穷尽
/// - 两种布局的变体名列表相同
#[test]
fn core_option_unite_exhaustiveness_is_layout_independent() {
    let module = compile_core_type_file("Option.v");
    let option = module.enums.iter().find(|enum_def| enum_def.name == Identifier::new("Option")).expect("Option unite should be present");

    let tagged = lower_unite(option, UniteLayout::Tagged);
    let untagged = lower_unite(option, UniteLayout::Untagged);
    let complete = vec![Identifier::new("Some"), Identifier::new("None")];
    let incomplete = vec![Identifier::new("Some")];

    assert!(tagged.is_exhaustive_over(&complete));
    assert!(untagged.is_exhaustive_over(&complete));
    assert!(!tagged.is_exhaustive_over(&incomplete));
    assert!(!untagged.is_exhaustive_over(&incomplete));
    assert_eq!(tagged.variant_names(), untagged.variant_names());
}

/// 验证 `core::types::Option` 模块的命名空间为 `core::types`。
///
/// 这是迁移的唯一实质性变化点：命名空间从 `std.types` 变为 `core::types`。
/// 运行时布局（tag 值、变体结构、is_unity）不受命名空间变化影响。
#[test]
fn core_option_module_namespace_is_core_types() {
    let module = compile_core_type_file("Option.v");

    let namespace_segments: Vec<&str> = module.name.parts().iter().map(|id| id.as_str()).collect();
    assert_eq!(namespace_segments, vec!["core", "types"]);
}

/// 验证 `core::types::Result` 模块的命名空间为 `core::types`。
#[test]
fn core_result_module_namespace_is_core_types() {
    let module = compile_core_type_file("Result.v");

    let namespace_segments: Vec<&str> = module.name.parts().iter().map(|id| id.as_str()).collect();
    assert_eq!(namespace_segments, vec!["core", "types"]);
}

/// 验证 `core::types::Option` 的变体可通过 `NominalModuleView` 解析为 Option 的子类型。
///
/// 迁移前后均应满足：Some <: Option, None <: Option。
#[test]
fn core_option_nominal_view_resolves_variants() {
    let module = compile_core_type_file("Option.v");
    let view = NominalModuleView::from_module(&module);

    assert!(view.matches_nominal_parameter(&Identifier::new("Some"), &Identifier::new("Option")).unwrap());
    assert!(view.matches_nominal_parameter(&Identifier::new("None"), &Identifier::new("Option")).unwrap());
}

/// 验证 `core::types::Result` 的变体可通过 `NominalModuleView` 解析为 Result 的子类型。
#[test]
fn core_result_nominal_view_resolves_variants() {
    let module = compile_core_type_file("Result.v");
    let view = NominalModuleView::from_module(&module);

    assert!(view.matches_nominal_parameter(&Identifier::new("Fine"), &Identifier::new("Result")).unwrap());
    assert!(view.matches_nominal_parameter(&Identifier::new("Fail"), &Identifier::new("Result")).unwrap());
}
