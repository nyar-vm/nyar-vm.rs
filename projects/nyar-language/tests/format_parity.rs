//! Format parity integration tests (`.editorconfig`, idempotence, comments).

use std::{fs, path::PathBuf};

use nyar_analyzer::format::FormatConfigLoader;
use nyar_language::formatter::{FormatOptions, SourceKind, format_source};

fn temp_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("nyar_fmt_parity_{name}_{}", std::process::id()))
}

#[test]
fn editorconfig_maps_to_format_options() {
    let dir = temp_dir("ec_map");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join(".editorconfig"),
        r#"
root = true
[*.v]
indent_style = tab
indent_size = 2
tab_width = 4
max_line_length = 120
insert_final_newline = false
"#,
    )
    .unwrap();
    let file = dir.join("sample.v");
    fs::write(&file, "micro main(){}").unwrap();

    let res = FormatConfigLoader::for_path(&file);
    assert_eq!(res.options.indent_width, 2);
    assert_eq!(res.options.tab_size, 4);
    assert!(!res.options.insert_spaces);
    assert_eq!(res.options.max_width, 120);
    assert!(!res.options.ensure_trailing_newline);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn format_idempotent_for_v_with_comment() {
    let source = "# header\nmicro main(){let x=1}\n";
    let options = FormatOptions::default();
    let once = format_source(SourceKind::V, source, &options).expect("format once");
    let twice = format_source(SourceKind::V, &once, &options).expect("format twice");
    assert_eq!(once, twice);
    assert!(once.contains('#'), "comment should be preserved");
}

#[test]
fn editorconfig_indent_applied_to_von_output() {
    let dir = temp_dir("ec_von");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(".editorconfig"), "[*.von]\nindent_size = 2\n").unwrap();
    let file = dir.join("data.von");
    fs::write(&file, r#"{ "a": 1 }"#).unwrap();

    let opts = FormatConfigLoader::for_path(&file).options;
    assert_eq!(opts.indent_width, 2);
    let formatted = format_source(SourceKind::Von, r#"{ "a": 1 }"#, &opts).expect("von fmt");
    let again = format_source(SourceKind::Von, &formatted, &opts).expect("von idempotent");
    assert_eq!(formatted, again);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn golden_v_micro_main_idempotent() {
    const INPUT: &str = "micro main(){let x=1}";
    let options = FormatOptions::default();
    let out = format_source(SourceKind::V, INPUT, &options).unwrap();
    assert_eq!(format_source(SourceKind::V, &out, &options).unwrap(), out);
}

#[test]
fn flags_members_align_equals() {
    const INPUT: &str = "flags FilePerm { Read = 1 Write = 2 }";
    let options = FormatOptions::default();
    let out = format_source(SourceKind::V, INPUT, &options).expect("format flags");
    assert!(out.contains("Read  = 1"), "expected aligned '=', got:\n{out}");
    assert!(out.contains("Write = 2"), "expected aligned '=', got:\n{out}");
    assert_eq!(format_source(SourceKind::V, &out, &options).unwrap(), out);
}

#[test]
fn enums_variants_align_equals() {
    const INPUT: &str = "enums Color { RED=2 GREEN=4 BLUE=6 }";
    let options = FormatOptions::default();
    let out = format_source(SourceKind::V, INPUT, &options).expect("format enums");
    assert!(out.contains("RED   = 2"), "expected aligned '=', got:\n{out}");
    assert!(out.contains("GREEN = 4"), "expected aligned '=', got:\n{out}");
    assert!(out.contains("BLUE  = 6"), "expected aligned '=', got:\n{out}");
    assert_eq!(format_source(SourceKind::V, &out, &options).unwrap(), out);
}
