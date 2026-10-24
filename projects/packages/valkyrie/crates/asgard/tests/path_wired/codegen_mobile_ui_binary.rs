//! Migrated from `src/codegen/mobile_ui_binary.rs` (`tests`).

use super::*;

use crate::awsl::{LoweringOptions, lower_component};
use oak_awsl::AwslParser;

#[test]
fn counter_ui_bin_snapshot() {
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
</script>"#;
    let root = AwslParser::parse_root(source).expect("parse awsl");
    let component = lower_component(&root, "counter", "counter.awsl", &LoweringOptions::default());
    let blob = encode_mobile_ui_package(&[component]);
    assert!(blob.starts_with(UI_BIN_MAGIC));
    assert_eq!(blob[UI_BIN_MAGIC.len()], UI_BIN_VERSION_V2);
    let text = String::from_utf8_lossy(&blob);
    assert!(text.contains("counter"));
    assert!(text.contains("Column"));
    assert!(text.contains("count"));
    assert!(text.contains("on_tap"));

    let decoded = decode_mobile_ui_package(&blob).expect("decode v0 baseline");
    assert_eq!(decoded.version, UI_BIN_VERSION_V2);
    assert_eq!(decoded.components.len(), 1);
    assert_eq!(decoded.components[0].name, "counter");
}

#[test]
fn rejects_package_without_container_tag() {
    let mut blob = Vec::new();
    blob.extend_from_slice(UI_BIN_MAGIC);
    blob.extend_from_slice(&1u32.to_le_bytes());
    write_string(&mut blob, "route");
    write_string(&mut blob, "widget");
    write_bindings(&mut blob, &[]);
    write_ir(&mut blob, &RenderModule::empty());
    assert!(decode_mobile_ui_package(&blob).is_err(), "缺失容器标签的负载必须失败");
}
