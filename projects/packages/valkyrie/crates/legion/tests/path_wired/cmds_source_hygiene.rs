//! Migrated from `src/cmds/source_hygiene.rs` (`tests`).

use super::*;

#[test]
fn line_limit_same_code_escalates_severity() {
    let soft = "x\n".repeat(SOURCE_LINE_WARN + 1);
    let hard = "x\n".repeat(SOURCE_LINE_ERROR + 1);
    let soft_report = scan_source_text(Path::new("soft.v"), &soft);
    let hard_report = scan_source_text(Path::new("hard.v"), &hard);
    assert_eq!(soft_report.warnings, 1);
    assert_eq!(soft_report.errors, 0);
    assert_eq!(hard_report.warnings, 0);
    assert_eq!(hard_report.errors, 1);
}

#[test]
fn replacement_char_is_encoding_error() {
    let report = scan_source_text(Path::new("bad.v"), "函\u{FFFD}数");
    assert_eq!(report.errors, 1);
}
