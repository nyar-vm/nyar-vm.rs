//! AWSL 降级与 JS 胶水测试。

use std::path::PathBuf;

use asgard::{
    awsl::{LoweringOptions, RenderNode, lower_component},
    codegen::{asgard_boot_script_tag, build_awsl_wasm_source, generate_boot_script, generate_component_glue, manifest_url_for_mode},
};
use oak_awsl::AwslParser;

fn valkyrie_v_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let submodule = manifest.join("../../../../valkyrie.v");
    assert!(submodule.is_dir(), "missing `projects/valkyrie.v` submodule");
    submodule
}

fn fixture(path: &str) -> String {
    let path = path.strip_prefix("valkyrie.v/").unwrap_or(path);
    std::fs::read_to_string(valkyrie_v_root().join(path)).expect("read fixture")
}

#[test]
fn lower_button_has_render_ir() {
    let source = fixture("projects/asgard._/projects/asgard/source/components/button.awsl");
    let root = AwslParser::parse_root(&source).expect("parse");
    let lowered = lower_component(&root, "button", "button.awsl", &LoweringOptions::default());
    let has_element =
        lowered.render_ir.roots.iter().any(|&id| matches!(lowered.render_ir.node(id), RenderNode::Element(_) | RenderNode::Component(_)));
    assert!(has_element);
}

#[test]
fn wasm_source_contains_dom_render_export() {
    let source = fixture("examples/test.blog/source/pages/index.awsl");
    let root = AwslParser::parse_root(&source).expect("parse");
    let lowered = lower_component(&root, "index", "index.awsl", &LoweringOptions::default());
    let v_source = build_awsl_wasm_source(&[lowered]);
    assert!(v_source.contains("dom_create_element"));
    assert!(v_source.contains("awsl_hydrate_index"));
}

#[test]
fn script_let_mut_emits_reactive_binding_in_wasm() {
    let source = r#"<widget><text>{count}</text></widget>
<script>
let mut count = 0
let title = "hi"
</script>"#;
    let root = AwslParser::parse_root(source).expect("parse");
    let lowered = lower_component(&root, "demo", "demo.awsl", &LoweringOptions::default());
    assert!(lowered.script_bindings.iter().any(|b| b.name == "count" && b.reactive));
    assert!(lowered.script_bindings.iter().any(|b| b.name == "title" && !b.reactive));
    let v_source = build_awsl_wasm_source(&[lowered]);
    assert!(v_source.contains("sig_create_i32"));
    assert!(v_source.contains("let title = \"hi\""));
}

#[test]
fn boot_script_auto_starts_manifest_no_page_inline() {
    let relative = generate_boot_script("legion-test", manifest_url_for_mode(true));
    assert!(relative.contains("start('manifest.json')"));
    assert!(relative.contains("asgard start failed"));
    let absolute = generate_boot_script("asgard-app", manifest_url_for_mode(false));
    assert!(absolute.contains("start('/manifest.json')"));
    assert_eq!(asgard_boot_script_tag(true), r#"<script src="boot.js"></script>"#);
    assert_eq!(asgard_boot_script_tag(false), r#"<script src="/boot.js"></script>"#);
    assert!(!asgard_boot_script_tag(true).contains("__voa"));
}

#[test]
fn glue_js_calls_wasm_not_reactive_dom() {
    let source = fixture("examples/test.blog/source/pages/index.awsl");
    let root = AwslParser::parse_root(&source).expect("parse");
    let lowered = lower_component(&root, "index", "index.awsl", &LoweringOptions::default());
    let glue = generate_component_glue(&lowered, "asgard-blog");
    assert!(glue.content.contains("callExport"));
    assert!(glue.content.contains("__voa"));
    assert!(!glue.content.contains("createSignal"));
    assert!(!glue.content.contains("listMap"));
    assert!(!glue.content.contains("Voa."));
    assert_eq!(glue.relative_path, "c/index.js");
}

#[test]
fn blog_awsl_v_compiles_or_prints_error() {
    use asgard::codegen::build_awsl_wasm_source;
    use rusty_valkyrie::ValkyrieCompiler;

    let root = valkyrie_v_root();
    let source_root = root.join("examples/test.blog/source");
    if !source_root.exists() {
        return;
    }
    let mut components = Vec::new();
    collect_awsl_components(&source_root, &mut components);
    let v_source = build_awsl_wasm_source(&components);
    let result = ValkyrieCompiler::default().compile_source_to_program(&v_source);
    if let Err(error) = &result {
        eprintln!("AWSL V compile error: {error}");
        eprintln!("--- source tail ---\n{}", &v_source[v_source.len().saturating_sub(500)..]);
    }
    result.expect("AWSL pages should compile to WASM-ready V");
}

#[test]
fn demo_todo_awsl_v_compiles_or_prints_error() {
    use asgard::codegen::build_awsl_wasm_source;
    use rusty_valkyrie::ValkyrieCompiler;

    let root = valkyrie_v_root();
    let source_root = root.join("examples/demo.asgard.todo/source");
    if !source_root.exists() {
        return;
    }
    let mut components = Vec::new();
    collect_awsl_components(&source_root, &mut components);
    let v_source = build_awsl_wasm_source(&components);
    let result = ValkyrieCompiler::default().compile_source_to_program(&v_source);
    if let Err(error) = &result {
        eprintln!("AWSL V compile error: {error}");
        let snippet_start = 1000usize.min(v_source.len());
        let snippet_end = 1150usize.min(v_source.len());
        eprintln!("--- snippet ---\n{}", &v_source[snippet_start..snippet_end]);
    }
    result.expect("demo todo AWSL should compile to WASM-ready V");
}

#[test]
fn interactive_col_plot_emits_loop_and_parameterized_events() {
    let source = fixture("projects/asgard._/projects/asgard.plotter/source/components/interactive-col-plot.awsl");
    let root = AwslParser::parse_root(&source).expect("parse");
    let lowered = lower_component(&root, "interactive-col-plot", "interactive-col-plot.awsl", &LoweringOptions::default());
    let v_source = build_awsl_wasm_source(&[lowered]);
    assert!(v_source.contains("rx_bind_loop"), "expected loop binding");
    assert!(v_source.contains("loop_mount"), "expected loop_mount export");
    assert!(v_source.contains("let item = series[index]") || v_source.contains("let item = series [index]"), "expected item bind from index");
    assert!(v_source.contains("dom_add_event_export_utf8"), "expected utf8-arg event bind");
    assert!(v_source.contains("awsl_call_toggle"), "expected toggle call wrapper");
    assert!(v_source.contains("awsl_call_pin"), "expected pin call wrapper");
    assert!(v_source.contains("index: i32"), "loop_mount should take index");
}

#[test]
fn tabs_emits_arrow_handler_and_icon_fields() {
    let source = fixture("projects/asgard._/projects/asgard/source/components/tabs.awsl");
    let root = AwslParser::parse_root(&source).expect("parse");
    let lowered = lower_component(&root, "tabs", "tabs.awsl", &LoweringOptions::default());
    let v_source = build_awsl_wasm_source(&[lowered]);
    assert!(v_source.contains("rx_bind_loop"));
    assert!(v_source.contains("let tab ="), "expected tab loop item");
    assert!(v_source.contains("awsl_call_setactive"), "expected setActive call");
    assert!(v_source.contains("dom_add_event_export_utf8") || v_source.contains("dom_add_event_export"), "expected event bind");
    assert!(v_source.contains("tab.icon") || v_source.contains("utf8(tab.icon)"), "expected icon field binding");
}

#[test]
fn list_emits_item_select_handler_and_icon_fields() {
    let source = fixture("projects/asgard._/projects/asgard/source/components/list.awsl");
    let root = AwslParser::parse_root(&source).expect("parse");
    let lowered = lower_component(&root, "list", "list.awsl", &LoweringOptions::default());
    let v_source = build_awsl_wasm_source(&[lowered]);
    assert!(v_source.contains("rx_bind_loop"));
    assert!(v_source.contains("let item ="));
    assert!(v_source.contains("awsl_call_selectitem"));
    assert!(
        v_source.contains("dom_add_event_export_i32") || v_source.contains("dom_add_event_export_utf8"),
        "selectItem(item) should capture index or field"
    );
    assert!(v_source.contains("item.icon") || v_source.contains("utf8(item.icon)"));
}

#[test]
fn interactive_col_plot_v_compiles_or_prints_error() {
    use rusty_valkyrie::ValkyrieCompiler;

    let source = fixture("projects/asgard._/projects/asgard.plotter/source/components/interactive-col-plot.awsl");
    let root = AwslParser::parse_root(&source).expect("parse");
    let lowered = lower_component(&root, "interactive-col-plot", "interactive-col-plot.awsl", &LoweringOptions::default());
    let v_source = build_awsl_wasm_source(&[lowered]);
    let result = ValkyrieCompiler::default().compile_source_to_program(&v_source);
    if let Err(error) = &result {
        eprintln!("InteractiveColPlot V compile error: {error}");
        eprintln!("--- source tail ---\n{}", &v_source[v_source.len().saturating_sub(800)..]);
    }
    result.expect("InteractiveColPlot AWSL should compile to WASM-ready V");
}

#[test]
fn abi_property_and_event_extracted_from_script() {
    let source = r#"<widget demo><Child :theme="theme" /></widget>
<script>
[property]
let theme = "dark";

[event]
micro theme_change(theme: string) { }

micro handler() {
    emit(theme_change, theme);
}
</script>"#;
    let root = AwslParser::parse_root(source).expect("parse");
    let lowered = lower_component(&root, "demo", "demo.awsl", &LoweringOptions::default());
    assert_eq!(lowered.component_abi.properties.len(), 1);
    assert_eq!(lowered.component_abi.events.len(), 1);
    assert!(lowered.abi_issues.iter().all(|issue| issue.severity != oak_awsl::AbiSeverity::Error));
}

fn collect_awsl_components(dir: &std::path::Path, out: &mut Vec<asgard::awsl::LoweredComponent>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            collect_awsl_components(&path, out);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("awsl") {
            continue;
        }
        let name = path.file_stem().unwrap().to_str().unwrap().to_string();
        let source = std::fs::read_to_string(&path).unwrap();
        let parsed = AwslParser::parse_root(&source).unwrap();
        out.push(lower_component(&parsed, &name, "", &LoweringOptions::default()));
    }
}
