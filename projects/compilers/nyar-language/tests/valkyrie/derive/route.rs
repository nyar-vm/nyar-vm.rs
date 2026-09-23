use nyar_language::{
    route_injector::{collect_atlas_routes, inject_atlas_routes},
    types::{
        Identifier, NamePath, SourceID, SourceSpan,
        hir::{
            HirBlock, HirDocumentation, HirExpr, HirExprKind, HirFunction, HirLiteral, HirModule, HirStringLiteral, HirStringSegment,
            HirStruct, HirVisibility, ValkyrieType,
        },
    },
};

fn test_span() -> SourceSpan {
    SourceSpan::new(SourceID::default(), 0, 0)
}

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

fn route_attr(method: &str, path: &str) -> nyar_language::types::hir::HirAttribute {
    nyar_language::types::hir::HirAttribute::with_arguments(
        NamePath::new(vec![Identifier::new(method)]),
        vec![nyar_language::types::hir::HirArgument {
            key: None,
            value: Box::new(HirExpr {
                kind: HirExprKind::Literal(HirLiteral::String(HirStringLiteral {
                    prefix: None,
                    quote_count: 1,
                    segments: vec![HirStringSegment::Text(path.to_string())],
                })),
                span: test_span(),
            }),
        }],
    )
}

fn health_controller() -> HirStruct {
    HirStruct {
        name: Identifier::new("HealthController"),
        doc: HirDocumentation::from_lines(vec!["@route_prefix(\"/api\")".to_string()]),
        methods: vec![
            HirFunction {
                name: Identifier::new("get_health"),
                doc: HirDocumentation::default(),
                annotations: vec![route_attr("get", "/health")],
                generics: vec![],
                params: vec![],
                return_type: ValkyrieType::Named(Identifier::new("AtlasResult")),
                body: HirBlock {
                    statements: vec![],
                    expr: None,
                    span: test_span(),
                },
                span: test_span(),
                visibility: HirVisibility::public(),
                is_abstract: false,
                is_final: false,
        export_spec: None,
            },
            HirFunction {
                name: Identifier::new("get_live"),
                doc: HirDocumentation::default(),
                annotations: vec![route_attr("get", "/health/live")],
                generics: vec![],
                params: vec![],
                return_type: ValkyrieType::Named(Identifier::new("AtlasResult")),
                body: HirBlock {
                    statements: vec![],
                    expr: None,
                    span: test_span(),
                },
                span: test_span(),
                visibility: HirVisibility::public(),
                is_abstract: false,
                is_final: false,
        export_spec: None,
            },
        ],
        ..Default::default()
    }
}

#[test]
fn route_injector_collects_prefixed_paths() {
    let module = empty_module(vec![health_controller()]);
    let routes = collect_atlas_routes(&module);
    assert_eq!(routes.len(), 2);
    assert_eq!(routes[0].path, "/api/health");
    assert_eq!(routes[1].path, "/health/live");
}

#[test]
fn route_injector_generates_register_and_invoke() {
    let module = empty_module(vec![health_controller()]);
    let result = inject_atlas_routes(&module);
    assert!(result.generated_v.contains("atlas_register_routes"));
    assert!(result.generated_v.contains("atlas_invoke_handler"));
    assert!(result.generated_v.contains("HealthController::wire"));
}
