//! Migrated from `src/codegen/terminal_v_render.rs` (`tests`).

use super::*;

use crate::awsl::{LoweringOptions, lower_component};
use oak_awsl::AwslParser;

fn lower(source: &str) -> LoweredComponent {
    let root = AwslParser::parse_root(source).expect("parse");
    lower_component(&root, "Test", "test.awsl", &LoweringOptions::default())
}

#[test]
fn generates_mount_with_widget_column() {
    let component = lower("<widget><Column><Text>Hi</Text></Column></widget>");
    let v = render_terminal_component_v(&component);
    assert!(v.contains("micro awsl_mount_test()"), "缺少 mount 函数: {v}");
    assert!(v.contains("widget_column"), "缺少 widget_column: {v}");
    assert!(v.contains("widget_text"), "缺少 widget_text: {v}");
}

#[test]
fn generates_init_for_bindings() {
    let component = lower(
        r#"<widget><Text>{count}</Text></widget>
<script>
let mut count: i32 = 0
</script>"#,
    );
    let v = render_terminal_component_v(&component);
    assert!(v.contains("micro awsl_init_test()"), "缺少 init 函数: {v}");
    assert!(v.contains("tui_register_binding(\"count\""), "缺少绑定注册: {v}");
}

#[test]
fn generates_main_entry() {
    let component = lower("<widget><Text>Hi</Text></widget>");
    let v = render_terminal_component_v(&component);
    assert!(v.contains("micro awsl_main()"), "缺少 main 函数: {v}");
    assert!(v.contains("asgard_terminal_run"), "缺少启动调用: {v}");
}

#[test]
fn dynamic_text_reads_binding() {
    let component = lower(
        r#"<widget><Text>Count: {count}</Text></widget>
<script>
let mut count: i32 = 0
</script>"#,
    );
    let v = render_terminal_component_v(&component);
    assert!(v.contains("tui_binding_get(\"count\")"), "动态文本应读取绑定: {v}");
}

#[test]
fn button_emits_handler_wrapper() {
    let component = lower(r#"<widget><Button @click="on_tap">Tap</Button></widget>"#);
    let v = render_terminal_component_v(&component);
    assert!(v.contains("widget_button"), "缺少 widget_button: {v}");
    assert!(v.contains("awsl_call_on_tap"), "缺少 handler 包装: {v}");
    assert!(v.contains("tui_remount"), "缺少重挂载调用: {v}");
}

#[test]
fn list_and_item_generate_widget_calls() {
    let component = lower(r#"<widget><List><Item @click="on_a">A</Item><Item @click="on_b">B</Item></List></widget>"#);
    let v = render_terminal_component_v(&component);
    assert!(v.contains("widget_list"), "缺少 widget_list: {v}");
    assert!(v.contains("widget_item"), "缺少 widget_item: {v}");
    assert!(v.contains("awsl_call_on_a"), "缺少 on_a handler: {v}");
    assert!(v.contains("awsl_call_on_b"), "缺少 on_b handler: {v}");
}

#[test]
fn checkbox_emits_binding() {
    let component = lower(
        r#"<widget><Checkbox :binding="agree">同意</Checkbox></widget>
<script>
let mut agree: bool = false
</script>"#,
    );
    let v = render_terminal_component_v(&component);
    assert!(v.contains("widget_checkbox"), "缺少 widget_checkbox: {v}");
    assert!(v.contains("\"agree\""), "缺少绑定名: {v}");
}

#[test]
fn box_generates_widget_box() {
    let component = lower("<widget><Box><Text>Content</Text></Box></widget>");
    let v = render_terminal_component_v(&component);
    assert!(v.contains("widget_box"), "缺少 widget_box: {v}");
}
