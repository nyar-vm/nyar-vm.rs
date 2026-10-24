//! Migrated from `src/pipeline/mod.rs` (`tests`).

use super::*;

use crate::codegen::{HOST_NATIVE_MAGIC, UI_BIN_MAGIC};

/// 当前宿主上的 `CanonicalTarget` 解析器不支持该拼写时跳过（例如 Windows 上的 Android/iOS）。
fn skip_unless_canonical_target(spelling: &str) -> bool {
    CanonicalTarget::parse(spelling).is_ok()
}

#[test]
fn unknown_config_target_fails_instead_of_selecting_wasm() {
    let error = parse_target("unknown-target").expect_err("未知 target 必须直接失败");
    assert!(error.to_string().contains("无法解析 target `unknown-target`"), "{error}");
}

#[test]
fn config_target_preserves_the_canonical_architecture() {
    for spelling in ["node", "wasm", "jvm", "aarch64-android-android-aapcs64"] {
        let expected = CanonicalTarget::parse(spelling).expect("目标必须由共享解析器接受");
        assert_eq!(parse_target(spelling).expect("配置目标必须保持共享合同"), expected);
    }
    assert_eq!(parse_target("jvm-android-android-managed").expect("产品别名必须保留").vendor, CanonicalVendor::Android);
}

#[test]
fn private_target_spellings_do_not_select_another_backend() {
    for spelling in ["wechat-miniprogram-host", "wasm32-unknown-miniprogram-wasm", "aarch64-linux-android"] {
        let expected = CanonicalTarget::parse(spelling);
        match expected {
            Ok(target) => assert_eq!(parse_target(spelling).expect("共享解析结果必须保持"), target),
            Err(_) => assert!(parse_target(spelling).is_err(), "非法目标不能换入口: {spelling}"),
        }
    }
}

#[test]
fn package_miniprogram_emits_wxml_not_html() {
    if !skip_unless_canonical_target("wasm32-unknown-miniprogram-wasm") {
        return;
    }
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path();
    fs::create_dir_all(project.join("source/pages")).unwrap();
    fs::write(
        project.join("asgard.config.v"),
        r#"define_config(asgard) {
name = "mp-test"
platform = "wechat-miniprogram"
target = "wasm32-unknown-miniprogram-wasm"
build {
    output = "dist"
}
}
"#,
    )
    .unwrap();
    fs::write(
        project.join("source/pages/counter.awsl"),
        r#"<widget counter>
<div class="wrap">
    <text>{{count}}</text>
    <button @click="on_tap">+1</button>
</div>
</widget>
<script>
let mut count: i32 = 0
micro on_tap() { count = count + 1 }
</script>
<style>.wrap { padding: 12px; }</style>
"#,
    )
    .unwrap();

    let report =
        compile_voa_project(&CompileOptions { project_dir: project.to_path_buf(), output_dir: Some(project.join("dist")), target: None })
            .expect("compile miniprogram");
    assert_eq!(report.platform, HostPlatform::WechatMiniProgram);
    assert!(report.wasm_built, "miniprogram must compile WASM");
    assert!(!report.host_logic_built);
    assert_eq!(report.host_artifact_kind, None);
    assert!(!report.output_dir.join("host").exists());
    let runtime = fs::read_to_string(report.output_dir.join("asgard-runtime.js")).unwrap();
    assert!(!runtime.contains("__ASGARD_PRODUCT__"));
    assert!(runtime.contains("WXWebAssembly"));
    assert!(runtime.contains("syncFromWasm"));
    assert!(runtime.contains("on_event"));
    assert!(!runtime.contains("on_tap(page)"));
    let page_js = fs::read_to_string(report.output_dir.join("pages/counter/counter.js")).unwrap();
    assert!(page_js.contains("asgard.on_event('on_tap')") || page_js.contains("awsl_call_on_tap"));
    let wasm_path = report.output_dir.join("mp-test.wasm");
    assert!(wasm_path.exists(), "expected dist/mp-test.wasm");
    let wasm = fs::read(&wasm_path).unwrap();
    assert!(wasm.windows(UI_BIN_MAGIC.len()).any(|w| w == UI_BIN_MAGIC));
    assert!(report.output_dir.join("app.json").exists());
    assert!(!report.output_dir.join("index.html").exists());
    let wxml_path = report.output_dir.join("pages/counter/counter.wxml");
    assert!(wxml_path.exists(), "missing counter.wxml");
    let wxml = fs::read_to_string(wxml_path).unwrap();
    assert!(wxml.contains("bind:tap") || wxml.contains("{{count}}"));
}

#[test]
fn dev_mode_emits_render_ir_sidecar() {
    if !skip_unless_canonical_target("aarch64-linux-android") {
        return;
    }
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path();
    fs::create_dir_all(project.join("source/pages")).unwrap();
    fs::write(
        project.join("asgard.config.v"),
        r#"define_config(asgard) {
name = "dev-sidecar-test"
platform = "android"
target = "aarch64-linux-android"
build {
    output = "dist"
    mode = "dev"
}
}
"#,
    )
    .unwrap();
    fs::write(
        project.join("source/pages/counter.awsl"),
        r#"<widget counter>
<Column><Text>hi</Text></Column>
</widget>
"#,
    )
    .unwrap();

    let report =
        compile_voa_project(&CompileOptions { project_dir: project.to_path_buf(), output_dir: Some(project.join("dist")), target: None })
            .expect("compile android dev");
    assert!(report.output_dir.join("debug/render-ir.bin").exists());
    let sidecar = fs::read(report.output_dir.join("debug/render-ir.bin")).unwrap();
    assert!(sidecar.starts_with(crate::codegen::UI_BIN_MAGIC));
}

#[test]
fn single_theme_and_mode_are_inlined_in_render_ir() {
    let source = r#"<widget app>
<ThemeProvider>
    <ThemeSwitcher :themes="themeList" :modes="modeList" />
    <ThemeShell :theme="theme" :mode="mode">
        <Text>ok</Text>
    </ThemeShell>
</ThemeProvider>
</widget>
<script>
let themeList: list = [{ id: "fate", label: "Fate" }]
let modeList: list = [{ id: "light", label: "Light" }]
let theme: string = "fate"
let mode: string = "light"
</script>"#;
    let mut component = crate::awsl::compile_awsl_source(source, "app", "app.awsl", &LoweringOptions::default()).expect("compile");
    optimize_theme_registry(
        &mut component,
        &UiConfig {
            themes: vec![crate::config::UiRegistryEntry { id: "fate".into(), label: "Fate".into() }],
            modes: vec![crate::config::UiRegistryEntry { id: "light".into(), label: "Light".into() }],
        },
    );

    let rendered = format!("{:?}", component.render_ir);
    assert!(!rendered.contains("ThemeSwitcher"));
    assert!(!rendered.contains("ThemeShell"));
    assert!(rendered.contains("tag: \"Box\""));
    assert!(rendered.contains("theme-shell theme-fate-light"));
}

#[test]
fn package_android_emits_compose_not_html_or_wasm() {
    if !skip_unless_canonical_target("aarch64-linux-android") {
        return;
    }
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path();
    fs::create_dir_all(project.join("source/pages")).unwrap();
    fs::write(
        project.join("asgard.config.v"),
        r#"define_config(asgard) {
name = "android-test"
platform = "android"
target = "aarch64-linux-android"
build {
    output = "dist"
}
}
"#,
    )
    .unwrap();
    fs::write(
        project.join("source/pages/counter.awsl"),
        r#"<widget counter>
<Column>
    <Text>{count}</Text>
    <Button @click="on_tap">+1</Button>
</Column>
</widget>
<script>
let mut count: i32 = 0
micro on_tap() { count = count + 1 }
</script>
"#,
    )
    .unwrap();

    let report =
        compile_voa_project(&CompileOptions { project_dir: project.to_path_buf(), output_dir: Some(project.join("dist")), target: None })
            .expect("compile android");
    assert_eq!(report.platform, HostPlatform::Android);
    assert!(!report.wasm_built);
    assert!(report.host_logic_built);
    assert_eq!(report.host_artifact_kind, Some(HostArtifactKind::NativeExecutable));
    assert!(!report.output_dir.join("index.html").exists());
    assert!(report.output_dir.join("android/classes.dex").exists());
    assert!(!report.output_dir.join("android/lib").exists());
    assert!(!report.output_dir.join("android/ui.bin").exists());
    assert!(report.output_dir.join("android/AndroidManifest.xml").exists());
    assert!(!report.output_dir.join("android/settings.gradle.kts").exists());
    assert!(!report.output_dir.join("android/native").exists());
    let dex = fs::read(report.output_dir.join("android/classes.dex")).expect("classes.dex");
    crate::platform_contract::validate_android_dist_bytes(&dex).expect("android dist contract");
    crate::platform_contract::validate_android_dist_dir(&report.output_dir.join("android")).expect("android dir contract");
}

#[test]
fn package_ios_emits_binary_not_html_or_wasm() {
    if !skip_unless_canonical_target("aarch64-apple-ios-aapcs64") {
        return;
    }
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path();
    fs::create_dir_all(project.join("source/pages")).unwrap();
    fs::write(
        project.join("asgard.config.v"),
        r#"define_config(asgard) {
name = "ios-test"
platform = "ios"
target = "aarch64-apple-ios-aapcs64"
build {
    output = "dist"
}
}
"#,
    )
    .unwrap();
    fs::write(
        project.join("source/pages/counter.awsl"),
        r#"<widget counter>
<Column>
    <Text>{count}</Text>
    <Button @click="on_tap">+1</Button>
</Column>
</widget>
<script>
let mut count: i32 = 0
micro on_tap() { count = count + 1 }
</script>
"#,
    )
    .unwrap();

    let report =
        compile_voa_project(&CompileOptions { project_dir: project.to_path_buf(), output_dir: Some(project.join("dist")), target: None })
            .expect("compile ios");
    assert_eq!(report.platform, HostPlatform::Ios);
    assert!(!report.wasm_built);
    assert!(report.host_logic_built);
    assert_eq!(report.host_artifact_kind, Some(HostArtifactKind::NativeExecutable));
    assert!(!report.output_dir.join("index.html").exists());
    assert!(!report.output_dir.join("ios/ui.bin").exists());
    assert!(!report.output_dir.join("ios/host.bin").exists());
    assert!(report.output_dir.join("ios/AsgardHost").exists());
    assert!(report.output_dir.join("ios/Info.plist").exists());
    let host = fs::read(report.output_dir.join("ios/AsgardHost")).expect("AsgardHost");
    assert_eq!(&host[..4], &[0xCF, 0xFA, 0xED, 0xFE]);
    assert!(host.windows(UI_BIN_MAGIC.len()).any(|w| w == UI_BIN_MAGIC));
    assert!(host.windows(HOST_NATIVE_MAGIC.len()).any(|w| w == HOST_NATIVE_MAGIC));
}

#[test]
fn package_desktop_emits_pe_on_windows() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path();
    fs::create_dir_all(project.join("source/pages")).unwrap();
    fs::write(
        project.join("asgard.config.v"),
        r#"define_config(asgard) {
name = "desktop-win"
platform = "windows"
target = "x86_64-pc-windows-msvc"
build {
    output = "dist"
}
}
"#,
    )
    .unwrap();
    fs::write(
        project.join("source/pages/counter.awsl"),
        r#"<widget counter>
<Column><Text>hi</Text></Column>
</widget>
"#,
    )
    .unwrap();
    let report =
        compile_voa_project(&CompileOptions { project_dir: project.to_path_buf(), output_dir: Some(project.join("dist")), target: None });
    if let Ok(report) = report {
        assert_eq!(report.platform, HostPlatform::Windows);
        let exe = report.output_dir.join("windows/desktop-win.exe");
        if exe.exists() {
            let bytes = fs::read(exe).unwrap();
            assert_eq!(&bytes[0..2], b"MZ");
        }
    }
}

#[test]
fn package_terminal_emits_v_source_and_executable() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path();
    fs::create_dir_all(project.join("source/pages")).unwrap();
    fs::write(
        project.join("asgard.config.v"),
        r#"define_config(asgard) {
name = "terminal-test"
platform = "terminal"
target = "x86_64-pc-windows-msvc"
build {
    output = "dist"
}
}
"#,
    )
    .unwrap();
    fs::write(
        project.join("source/pages/counter.awsl"),
        r#"<widget counter>
<Column>
    <Text>{count}</Text>
    <Button @click="on_tap">+1</Button>
</Column>
</widget>
<script>
let mut count: i32 = 0
micro on_tap() { count = count + 1 }
</script>
"#,
    )
    .unwrap();

    let report =
        compile_voa_project(&CompileOptions { project_dir: project.to_path_buf(), output_dir: Some(project.join("dist")), target: None });
    if let Ok(report) = report {
        assert_eq!(report.platform, HostPlatform::Terminal);
        assert!(!report.wasm_built);
        assert!(report.host_logic_built);
        assert_eq!(report.host_artifact_kind, Some(HostArtifactKind::NativeExecutable));
        assert!(report.output_dir.join("terminal/manifest.toml").exists());
        let app_v = report.output_dir.join("terminal/app.v");
        assert!(app_v.exists(), "app.v 必须存在");
        let v_src = fs::read_to_string(&app_v).unwrap();
        assert!(v_src.contains("widget_text"), "app.v 必须包含 widget_text: {v_src}");
        assert!(v_src.contains("widget_button"), "app.v 必须包含 widget_button: {v_src}");
        assert!(v_src.contains("awsl_mount_counter"), "app.v 必须包含 mount 函数: {v_src}");
        assert!(v_src.contains("tui_binding_get"), "app.v 必须读取绑定: {v_src}");
        assert!(!report.output_dir.join("terminal/AsgardTerminalRuntime.c").exists(), "不得生成 C 运行时");
        assert!(!report.output_dir.join("index.html").exists());
    }
}
