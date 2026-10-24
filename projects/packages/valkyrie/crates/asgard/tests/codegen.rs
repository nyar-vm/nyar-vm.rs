//! AWSL 降级与 JS 胶水测试（自包含样例，不读取外部语言语料）。

use asgard::{
    awsl::{LoweringOptions, lower_component},
    codegen::{asgard_boot_script_tag, build_awsl_wasm_source, generate_boot_script, manifest_url_for_mode},
};
use oak_awsl::AwslParser;

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
