//! Migrated from `src/cmds/report/standalone.rs` (`tests`).

use super::*;

use tempfile::tempdir;

#[test]
fn escape_script_body_breaks_out_tags() {
    assert_eq!(escape_script_body("a</script>b"), "a<\\/script>b");
}

#[test]
fn build_standalone_inlines_assets() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(
        root.join("index.html"),
        r#"<!DOCTYPE html><html><head><link rel="stylesheet" href="legion-test.css"></head><body><p>ok</p><script src="boot.js"></script></body></html>"#,
    )
    .unwrap();
    fs::write(root.join("legion-test.css"), ".x{color:red}").unwrap();
    fs::write(root.join("manifest.json"), r#"{"css":["legion-test.css"],"wasm":[{"glue":"legion-test.mjs","url":"legion-test.wasm"}],"components":[{"js":"c/chart.js","name":"chart-status"}]}"#).unwrap();
    fs::write(root.join("boot.js"), "start('manifest.json');").unwrap();
    fs::write(root.join("legion-test.wasm"), b"\0asm\x01").unwrap();
    fs::create_dir_all(root.join("c")).unwrap();
    fs::write(root.join("c/chart.js"), "__voa.registerComponent('chart-status', function(){});").unwrap();

    build_standalone_html(root, "standalone.html").unwrap();

    let html = fs::read_to_string(root.join("standalone.html")).unwrap();
    assert!(!html.contains(r#"src="boot.js""#));
    assert!(html.contains("start('manifest.json')"));
    assert!(html.contains("__voaStandaloneWasmBytes"));
    assert!(html.contains("globalThis.fetch"));
    assert!(html.contains(".x{color:red}"));
    assert!(!html.contains(r#"href="legion-test.css""#));
    assert!(html.contains("registerComponent"));
}

#[test]
fn build_standalone_without_boot_copies_index() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("index.html"), "<html><body>static</body></html>").unwrap();
    build_standalone_html(root, "standalone.html").unwrap();
    let html = fs::read_to_string(root.join("standalone.html")).unwrap();
    assert_eq!(html, "<html><body>static</body></html>");
}
