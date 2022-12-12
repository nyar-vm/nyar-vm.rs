use nyar_language::{SourceID, ValkyrieCompiler, type_checker::WidgetChecker, types::hir::ValkyrieType};

const COUNTER_VX: &str = r#"
widget Counter {
    micro view() {
        <div>{count}</div>
    }
}
"#;

#[test]
fn compile_vx_source_normalizes_view_to_render() {
    let compiler = ValkyrieCompiler::new(SourceID::default());
    let module = compiler.compile_vx_source(COUNTER_VX).expect("compile vx");
    let widget = module.widgets.iter().find(|widget| widget.name.as_str() == "Counter").expect("Counter widget");
    let render = widget.methods.iter().find(|method| method.name.as_str() == "render").expect("render");
    assert!(widget.methods.iter().all(|method| method.name.as_str() != "view"));
    assert!(matches!(&render.return_type, ValkyrieType::Named(name) if name.as_str() == "Element"));
}

#[test]
fn widget_checker_accepts_vx_render_after_normalization() {
    let compiler = ValkyrieCompiler::new(SourceID::default());
    let module = compiler.compile_vx_source(COUNTER_VX).expect("compile vx");
    let mut checker = WidgetChecker::new();
    let errors = checker.check_module(&module);
    assert!(errors.is_empty(), "widget errors: {errors:?}");
}

#[test]
fn compile_vx_source_expands_meta_if_in_view() {
    let compiler = ValkyrieCompiler::new(SourceID::default());
    let source = r#"
widget Panel {
    micro view() {
        <% if show %><div>{x}</div><% end %>
    }
}
"#;
    let module = compiler.compile_vx_source(source).expect("compile vx with meta if");
    let widget = module.widgets.iter().find(|widget| widget.name.as_str() == "Panel").expect("Panel widget");
    let render = widget.methods.iter().find(|method| method.name.as_str() == "render").expect("render");
    use nyar_language::types::hir::HirExprKind;
    assert!(matches!(render.body.expr.as_deref().map(|expr| &expr.kind), Some(HirExprKind::If { .. })));
}
