use nyar_language::{
    SourceID, SourceSpan,
    types::hir::{HirBlock, HirDocumentation, HirVisibility, HirWidgetLifecycle},
    valkyrie::types::hir::{HirFunction, HirModule, HirWidget, types::ValkyrieType},
    *,
};

use nyar_language::valkyrie::hir::lowering::enhance_vx_widgets;

#[test]
fn renames_view_method_to_render_with_element_return() {
    let widget = HirWidget {
        name: Identifier::new("Counter"),
        doc: HirDocumentation::default(),
        generics: Vec::new(),
        fields: Vec::new(),
        methods: vec![HirFunction {
            declaration: None,
            instance: None,
            name: Identifier::new("view"),
            where_constraints: Vec::new(),
            declaring_namespace: NamePath::default(),
            doc: HirDocumentation::default(),
            annotations: Vec::new(),
            generics: Vec::new(),
            params: Vec::new(),
            return_type: ValkyrieType::AutoType,
            body: HirBlock { statements: Vec::new(), expr: None, span: SourceSpan::new(SourceID::default(), 0, 0) },
            span: SourceSpan::new(SourceID::default(), 0, 0),
            visibility: HirVisibility::default(),
            is_abstract: false,
            is_final: false,
            is_virtual: false,
            is_override: false,
        }],
        visibility: HirVisibility::default(),
        state_fields: Vec::new(),
        initial_state: Vec::new(),
        lifecycle: HirWidgetLifecycle::default(),
    };
    let module = HirModule {
        name: NamePath::new(vec![Identifier::new("test")]),
        dependency_key: String::new(),
        source_path: "test.v".to_string(),
        doc: HirDocumentation::default(),
        imports: Vec::new(),
        warnings: Vec::new(),
        submodules: Vec::new(),
        functions: Vec::new(),
        structs: Vec::new(),
        enums: Vec::new(),
        imported_enums: Vec::new(),
        imported_semantic_exports: Vec::new(),
        flags: Vec::new(),
        traits: Vec::new(),
        impls: Vec::new(),
        type_functions: Vec::new(),
        type_families: Vec::new(),
        widgets: vec![widget],
        singletons: Vec::new(),
        type_aliases: Vec::new(),
        statements: Vec::new(),
    };
    let enhanced = enhance_vx_widgets(module);
    let render = enhanced.widgets[0].methods.iter().find(|method| method.name.as_str() == "render").expect("render");
    assert!(enhanced.widgets[0].methods.iter().all(|method| method.name.as_str() != "view"));
    assert!(matches!(&render.return_type, ValkyrieType::Named(name) if name.as_str() == "Element"));
}
