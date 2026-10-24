//! Migrated from `src/codegen/mp_render.rs` (`tests`).

use super::*;

use crate::awsl::{LoweringOptions, lower_component};
use oak_awsl::AwslParser;

#[test]
fn counter_page_wxml_snapshot() {
    let source = r#"<widget counter>
<Column>
<Text>{count}</Text>
<Button @click="on_tap">+1</Button>
</Column>
</widget>
<script>
let mut count: i32 = 0
micro on_tap() {
count = count + 1
}
</script>
<style>
.wrap { padding: 24px; }
.count { font-size: 32px; }
</style>"#;
    let root = AwslParser::parse_root(source).expect("parse awsl");
    let component = lower_component(&root, "counter", "counter.awsl", &LoweringOptions::default());
    let wxml = generate_page_wxml(&component);
    assert!(wxml.content.contains("<view"), "wxml={}", wxml.content);
    assert!(wxml.content.contains("{{count}}"), "wxml={}", wxml.content);
    assert!(wxml.content.contains("bind:tap=\"on_tap\""), "wxml={}", wxml.content);
    assert_eq!(wxml.relative_path, "pages/counter/counter.wxml");
    assert!(wxml.reactive_fields.contains(&"count".to_string()));
}
