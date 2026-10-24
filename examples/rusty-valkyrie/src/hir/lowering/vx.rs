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
