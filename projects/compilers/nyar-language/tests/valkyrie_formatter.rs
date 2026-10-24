use nyar_language::formatter::{FormatBatchOptions, FormatOptions, SourceKind, format_paths, format_source};

fn assert_idempotent(kind: SourceKind, source: &str) {
    let options = FormatOptions::default();
    let once = format_source(kind, source, &options).expect("format once");
    let twice = format_source(kind, &once, &options).expect("format twice");
    assert_eq!(once, twice, "format should be idempotent");
}

#[test]
fn source_kind_from_extension() {
    assert_eq!(SourceKind::from_extension("v"), Some(SourceKind::V));
    assert_eq!(SourceKind::from_extension("valkyrie"), Some(SourceKind::V));
    assert_eq!(SourceKind::from_extension("vx"), Some(SourceKind::Vx));
    assert_eq!(SourceKind::from_extension("von"), Some(SourceKind::Von));
    assert_eq!(SourceKind::from_extension("awsl"), Some(SourceKind::Awsl));
    assert_eq!(SourceKind::from_extension("vk"), None);
    assert_eq!(SourceKind::from_extension("rs"), None);
}

#[test]
fn format_v_idempotent() {
    assert_idempotent(SourceKind::V, "micro main(){let x=1}");
}

#[test]
fn format_awsl_idempotent() {
    let source = r#"
<template>
<div class="box"><span>{title}</span></div>
</template>
"#;
    assert_idempotent(SourceKind::Awsl, source);
}

#[test]
fn format_von_idempotent_with_comment() {
    assert_idempotent(SourceKind::Von, "# note\n{ x: 1 }\n");
}

#[test]
fn format_v_idempotent_with_comment() {
    assert_idempotent(SourceKind::V, "# note\nmicro main(){let x=1}\n");
}

#[test]
fn format_von_idempotent() {
    assert_idempotent(SourceKind::Von, r#"{ "x": 1, "y": [true, null] }"#);
}

#[test]
fn format_vx_idempotent() {
    assert_idempotent(SourceKind::Vx, "namespace demo;");
}

#[test]
fn format_paths_check_detects_dirty() {
    let dir = std::env::temp_dir().join(format!("nyar_fmt_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("sample.von");
    std::fs::write(&path, r#"{ "a":1 }"#).unwrap();

    let check = FormatBatchOptions { check: true, ..FormatBatchOptions::default() };
    let report = format_paths(&[dir.clone()], &check).expect("check");
    assert_eq!(report.checked, 1);
    assert_eq!(report.changed, 1);

    let write = FormatBatchOptions { check: false, ..FormatBatchOptions::default() };
    let report = format_paths(&[dir.clone()], &write).expect("write");
    assert_eq!(report.changed, 1);

    let report = format_paths(&[dir.clone()], &check).expect("recheck");
    assert_eq!(report.changed, 0);

    let _ = std::fs::remove_dir_all(&dir);
}
