use nyar::{CanonicalTarget, ClrSuspendStrategy};
use nyar_emitter::nyar_backend_wasi::WasmPackageKind;
use rusty_valkyrie::{compile_pipeline::compile_source_groups_to_backend_bundle, *};

const TEMPLATE_SOURCE: &str = r#"[primitive("core::primitive::i32")]
structure i32 { }

<% match arch %>
<% case "wasm32" %>
[main] micro main() -> i32 { return 23 }
<% else %>
[main] micro main() -> i32 { return 0 }
<% end %>"#;

fn template_groups_from_source(source: String) -> [CompilerSourceGroup; 1] {
    [CompilerSourceGroup::from_source_files("app", "app", vec![CompilerSourceFile { path: "app.v".into(), source }], Vec::new()).as_entry()]
}

#[test]
fn structured_target_templates_compile_with_arch_context() {
    let groups = template_groups_from_source(TEMPLATE_SOURCE.to_string());
    let context = CompilerBuildContext::new(
        "wasm32",
        CanonicalTarget::parse("node").expect("node"),
        ClrSuspendStrategy::default(),
        WasmPackageKind::Binary,
    );
    let bundle =
        compile_source_groups_to_backend_bundle(&ValkyrieCompiler::default(), &groups, &context).expect("structured TGrammar must compile");
    assert_eq!(bundle.surface_counts(), (0, 1));
}

#[test]
fn structured_target_templates_compile_after_resolver_style_snapshot() {
    let mut source = String::new();
    source.push_str(TEMPLATE_SOURCE);
    source.push('\n');
    let groups = template_groups_from_source(source);
    let context = CompilerBuildContext::new(
        "wasm32",
        CanonicalTarget::parse("node").expect("node"),
        ClrSuspendStrategy::default(),
        WasmPackageKind::Binary,
    );
    let bundle =
        compile_source_groups_to_backend_bundle(&ValkyrieCompiler::default(), &groups, &context).expect("resolver-style snapshot must compile");
    assert_eq!(bundle.surface_counts(), (0, 1));
}
