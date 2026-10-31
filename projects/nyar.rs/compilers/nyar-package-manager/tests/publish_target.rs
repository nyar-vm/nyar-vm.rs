use nyar_package_manager::*;
use tempfile::TempDir;

#[test]
fn transport_channel_does_not_rewrite_publish_target() {
    // 运输渠道不得改写编译目标：发布 `wasm` 到 npm/jsr 仍解析 `wasm` 产物目录，
    // 不因渠道是 npm 就偷偷换成 `node`。
    let dir = TempDir::new().expect("temp");
    let artifact = dir.path().join("dist").join("wasm32-unknown-unknown-wasm");
    std::fs::create_dir_all(&artifact).expect("mkdir");
    let resolved = resolve_publish_artifact(dir.path(), "wasm").expect("resolve");
    assert_eq!(resolved.canonical_label, "wasm32-unknown-unknown-wasm");
}

#[test]
fn missing_dist_errors_with_hint() {
    let dir = TempDir::new().expect("temp");
    let error = resolve_publish_artifact(dir.path(), "node").expect_err("missing dist");
    assert!(error.to_string().contains("build --target"));
}
