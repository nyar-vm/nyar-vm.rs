//! `.vx` compile helpers: X-Grammar `view` to widget `render`.
use crate::{
    Identifier,
    types::{
        NamePath,
        hir::{HirFunction, HirModule, HirWidget, ValkyrieType},
    },
};

/// Normalize widget `view` methods to `render() -> Element` when `render` is missing.
pub fn enhance_vx_widgets(mut module: HirModule) -> HirModule {
    for widget in &mut module.widgets {
        enhance_widget_view_to_render(widget);
    }
    for submodule in &mut module.submodules {
        *submodule = enhance_vx_widgets(submodule.clone());
    }
    module
}

fn enhance_widget_view_to_render(widget: &mut HirWidget) {
    if widget.methods.iter().any(|method| method.name.as_str() == "render") {
        return;
    }
    let Some(view_index) = widget.methods.iter().position(|method| method.name.as_str() == "view")
    else {
        return;
    };
    let view = widget.methods.remove(view_index);
    widget.methods.push(view_to_render(view));
}

fn view_to_render(view: HirFunction) -> HirFunction {
    HirFunction { name: Identifier::new("render"), return_type: ValkyrieType::Named(Identifier::new("Element")), ..view }
}

/// Lower X-Grammar markup tree to `Element::from_markup(...)` call (placeholder).
pub fn lower_xml_markup_to_element_expr(node_count: usize, span: crate::types::SourceSpan) -> crate::types::hir::HirExpr {
    use crate::types::hir::{HirCallArgument, HirExpr, HirExprKind, HirLiteral};

    HirExpr {
        kind: HirExprKind::Call {
            callee: Box::new(HirExpr {
                kind: HirExprKind::Path(NamePath::new(vec![Identifier::new("Element"), Identifier::new("from_markup")])),
                span: span.clone(),
            }),
            args: vec![HirCallArgument::positional(HirExpr {
                kind: HirExprKind::Literal(HirLiteral::Integer64(node_count as i64)),
                span: span.clone(),
            })],
            resolved: None,
        },
        span,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        SourceID, SourceSpan,
        types::hir::{HirBlock, HirDocumentation, HirVisibility, HirWidgetLifecycle},
    };

    #[test]
    fn renames_view_method_to_render_with_element_return() {
        let widget = HirWidget {
            name: Identifier::new("Counter"),
            doc: HirDocumentation::default(),
            generics: Vec::new(),
            fields: Vec::new(),
            methods: vec![HirFunction {
                name: Identifier::new("view"),
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
}
