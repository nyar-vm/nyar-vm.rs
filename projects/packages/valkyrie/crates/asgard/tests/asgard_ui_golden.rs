//! Asgard UI golden fixture + closed-loop CIP（跨解码器 / shim 门禁）。

use std::path::PathBuf;

use asgard::{
    awsl::{LoweringOptions, lower_component},
    codegen::{
        HOST_NATIVE_MAGIC, UI_BIN_MAGIC, embed_asgard_ui_section, embed_host_native_section, encode_mobile_ui_package,
        find_asgard_native_section, find_asgard_ui_section, generate_android_compose_runtime, generate_desktop_native_runtime,
        generate_ios_swiftui_runtime, generate_mp_runtime, magic_bytes_literal, resolve_call_export,
    },
};
use oak_awsl::AwslParser;

fn counter_awsl() -> &'static str {
    r#"<widget counter>
<Column>
    <Text>{count}</Text>
    <Button @click="on_tap">+1</Button>
</Column>
</widget>
<script>
let mut count: i32 = 0
micro on_tap() { count = count + 1 }
</script>"#
}

fn if_awsl() -> &'static str {
    r#"<widget if_demo>
    #if show
        <Text>yes</Text>
    #else
        <Text>no</Text>
    #endif
</widget>
<script>let mut show: bool = true</script>"#
}

#[test]
fn golden_counter_blob_has_magic_and_fields() {
    let root = AwslParser::parse_root(counter_awsl()).expect("parse");
    let component = lower_component(&root, "counter", "counter.awsl", &LoweringOptions::default());
    let blob = encode_mobile_ui_package(&[component]);
    assert!(blob.starts_with(UI_BIN_MAGIC));
    let text = String::from_utf8_lossy(&blob);
    assert!(text.contains("counter"));
    assert!(text.contains("on_tap"));
}

#[test]
fn golden_if_blob_encodes_branches() {
    let root = AwslParser::parse_root(if_awsl()).expect("parse");
    let component = lower_component(&root, "if-demo", "if-demo.awsl", &LoweringOptions::default());
    let blob = encode_mobile_ui_package(&[component]);
    assert!(blob.starts_with(UI_BIN_MAGIC));
    assert!(blob.len() > 32);
}

#[test]
fn section_framing_roundtrip_and_eight_byte_magic() {
    assert_eq!(UI_BIN_MAGIC.len(), 8);
    assert_eq!(HOST_NATIVE_MAGIC.len(), 8);
    assert_eq!(UI_BIN_MAGIC, b"ASGARDUI");
    assert_eq!(HOST_NATIVE_MAGIC, b"ASGARDNT");
    let mut blob = vec![9, 9, 9];
    embed_asgard_ui_section(&mut blob, b"pkg");
    embed_host_native_section(&mut blob, b"so!");
    assert_eq!(find_asgard_ui_section(&blob), Some(b"pkg".as_slice()));
    assert_eq!(find_asgard_native_section(&blob), Some(b"so!".as_slice()));
}

#[test]
fn runtime_cip_injects_magic_and_closed_loop_abi() {
    let ui_lit = magic_bytes_literal(UI_BIN_MAGIC);
    let nt_lit = magic_bytes_literal(HOST_NATIVE_MAGIC);

    let mp = generate_mp_runtime("asgard-app");
    assert!(mp.contains(&ui_lit), "mp runtime must inject UI_BIN_MAGIC bytes");
    assert!(mp.contains("skipAbi") || mp.contains("version === 0x02"));
    assert!(mp.contains("on_event"));
    assert!(mp.contains("syncFromWasm"));
    assert!(mp.contains("sigSubscribe"));
    assert!(mp.contains("storeSubscribe"));
    assert!(!mp.contains("handlers.on_tap"));
    assert!(!mp.contains("function on_tap"));

    let android = generate_android_compose_runtime();
    assert!(android.contains(&ui_lit));
    assert!(android.contains(&nt_lit));
    assert!(android.contains("skipAbi") || android.contains("version == 0x02"));
    assert!(android.contains("findAsgardNativeSection"));
    assert!(android.contains("System.load"));
    assert!(android.contains("fun mount("));
    assert!(android.contains("fun patch("));
    assert!(android.contains("fun on_event("));
    assert!(android.contains("patchFromNative"));
    assert!(!android.contains("ASGDUI"));

    let mut dex = asgard::codegen::resolve_android_compose_dex().expect("compose dex");
    embed_host_native_section(&mut dex, b"test-so");
    embed_asgard_ui_section(&mut dex, &encode_mobile_ui_package(&[]));
    asgard::platform_contract::validate_android_dist_bytes(&dex).expect("shipped dex contract");

    let ios = generate_ios_swiftui_runtime();
    assert!(ios.contains("on_event") || ios.contains("resolveCallExport"));
    assert!(ios.contains("invokeExport"));
    assert!(ios.contains("findAsgardNativeSection"));
    assert!(ios.contains("loadNativeFromExecutable"));
    assert!(ios.contains("dlopen"));
    assert!(!ios.contains("registerDefaultHandlers"));

    for platform in ["windows", "linux", "macos"] {
        let desktop = generate_desktop_native_runtime(platform);
        assert!(desktop.contains("on_event") || desktop.contains("OnEvent") || desktop.contains("asgard_desktop_on_event"));
        assert!(desktop.contains("asgard_invoke_export") || desktop.contains("InvokeExport"));
        assert!(!desktop.contains("asgard_linux_dispatch"));
    }

    assert_eq!(resolve_call_export("on_tap"), "awsl_call_on_tap");
}

#[test]
fn forbids_legacy_magic_strings() {
    let android = generate_android_compose_runtime();
    let mp = generate_mp_runtime("x");
    for src in [&android, &mp] {
        assert!(!src.contains("ASGDUI"));
        assert!(!src.contains("ASGDNAT"));
        assert!(!src.contains("ASGDHOST"));
    }
}

#[test]
fn desktop_package_embeds_native_and_ui_sections() {
    use asgard::{
        awsl::{LoweringOptions, lower_component},
        package::desktop::package_desktop_project,
    };
    let root = AwslParser::parse_root(counter_awsl()).expect("parse");
    let component = lower_component(&root, "counter", "counter.awsl", &LoweringOptions::default());
    let native = b"native-so-payload";
    let out = package_desktop_project(&[component], "app", "linux", native);
    let exe = out.files.iter().find(|(n, _)| n == "app").expect("exe");
    if let asgard::package::artifact::PackageArtifact::Bytes(bytes) = &exe.1 {
        assert_eq!(find_asgard_native_section(bytes), Some(native.as_slice()));
        assert!(find_asgard_ui_section(bytes).is_some());
    }
}
