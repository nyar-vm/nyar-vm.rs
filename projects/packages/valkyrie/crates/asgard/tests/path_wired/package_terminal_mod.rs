//! Migrated from `src/package/terminal/mod.rs` (`tests`).

use super::*;

use crate::{
    awsl::{LoweredComponent, LoweringOptions, lower_component},
    codegen::build_awsl_terminal_source,
};
use oak_awsl::AwslParser;

/// 测试用运行时桩：真实 `tui.v` 由 package graph 提供。
const TEST_RUNTIME: &str = "structure TuiRuntime { }\nmicro widget_column(): unit { }\nmicro asgard_terminal_run(): i32 { return 0 }";

fn demo_component() -> LoweredComponent {
    let source = r#"<widget Demo><text>hi</text></widget>"#;
    let root = AwslParser::parse_root(source).expect("parse");
    lower_component(&root, "demo", "demo.awsl", &LoweringOptions::default())
}

#[test]
fn terminal_package_has_all_artifacts() {
    let component = demo_component();
    let out = package_terminal_project(build_awsl_terminal_source(&[component], TEST_RUNTIME), "app", &[]);
    assert!(out.files.iter().any(|(n, _)| n == "manifest.toml"), "missing manifest.toml");
    assert!(out.files.iter().any(|(n, _)| n == "app.v"), "missing app.v");
    let exe_count = out.files.iter().filter(|(n, _)| n.starts_with("app")).count();
    assert!(exe_count >= 1, "missing executable");
}

#[test]
fn app_v_contains_widget_runtime() {
    let component = demo_component();
    let out = package_terminal_project(build_awsl_terminal_source(&[component], TEST_RUNTIME), "app", &[]);
    let (_, artifact) = out.files.iter().find(|(n, _)| n == "app.v").expect("app.v");
    let text = match artifact {
        PackageArtifact::Text(text) => text,
        _ => panic!("app.v must be text"),
    };
    assert!(text.contains("structure TuiRuntime"), "app.v 必须包含 TuiRuntime: {text}");
    assert!(text.contains("widget_text"), "app.v 必须包含 widget_text 调用");
}

#[test]
fn manifest_declares_terminal_platform() {
    let component = demo_component();
    let out = package_terminal_project(build_awsl_terminal_source(&[component], TEST_RUNTIME), "app", &[]);
    let (_, artifact) = out.files.iter().find(|(n, _)| n == "manifest.toml").expect("manifest.toml");
    let text = match artifact {
        PackageArtifact::Text(text) => text,
        _ => panic!("manifest must be text"),
    };
    assert!(text.contains("platform: terminal"), "manifest must declare platform: terminal");
}

#[test]
fn no_desktop_or_mobile_artifacts() {
    let component = demo_component();
    let out = package_terminal_project(build_awsl_terminal_source(&[component], TEST_RUNTIME), "app", &[]);
    for (name, _) in &out.files {
        let lower = name.to_ascii_lowercase();
        assert!(!lower.contains("index.html"), "unexpected index.html: {name}");
        assert!(!lower.contains("classes.dex"), "unexpected classes.dex: {name}");
        assert!(!lower.contains("asgardhost"), "unexpected AsgardHost: {name}");
        assert!(!lower.contains("asgardterminalruntime.c"), "unexpected C runtime: {name}");
    }
}

#[test]
fn app_v_has_host_contract_declarations() {
    let component = demo_component();
    let out = package_terminal_project(build_awsl_terminal_source(&[component], TEST_RUNTIME), "app", &[]);
    let (_, artifact) = out.files.iter().find(|(n, _)| n == "app.v").expect("app.v");
    let text = match artifact {
        PackageArtifact::Text(text) => text,
        _ => panic!("app.v must be text"),
    };
    assert!(text.contains("[host_contract] micro clear"), "app.v 缺少 host_contract 声明");
    assert!(text.contains("[host_contract] micro put_char"), "app.v 缺少 put_char 声明");
}

#[test]
fn manifest_declares_app_v_source() {
    let component = demo_component();
    let out = package_terminal_project(build_awsl_terminal_source(&[component], TEST_RUNTIME), "app", &[]);
    let (_, artifact) = out.files.iter().find(|(n, _)| n == "manifest.toml").expect("manifest.toml");
    let text = match artifact {
        PackageArtifact::Text(text) => text,
        _ => panic!("manifest must be text"),
    };
    assert!(text.contains("source: app.v"), "manifest must declare source: app.v");
}
