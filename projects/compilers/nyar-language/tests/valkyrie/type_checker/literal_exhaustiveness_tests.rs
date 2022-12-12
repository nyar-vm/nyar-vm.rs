use nyar_language::{SourceID, ValkyrieCompiler};

/// `match b { case true: ... case false: ... }` 覆盖整个 bool 值域，无需 else，应编译通过。
#[test]
fn bool_full_coverage_passes() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4200 });
    compiler
        .compile_source(
            r#"
micro describe(b: bool) -> utf8 {
    match b {
        case true:
            "true"
        case false:
            "false"
    }
}
"#,
        )
        .expect("covering both true and false should be exhaustive for bool");
}

/// `match b { case true: ... }` 缺少 false 分支且无 else，应报 non-exhaustive。
#[test]
fn bool_missing_arm_reports_non_exhaustive() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4200 });
    let error = compiler
        .compile_source(
            r#"
micro describe(b: bool) -> utf8 {
    match b {
        case true:
            "true"
    }
}
"#,
        )
        .expect_err("missing false arm without else should be non-exhaustive");
    let message = error.to_string();
    assert!(message.contains("non exhaustive"), "unexpected error: {message}");
}

/// `match n { case 0..=127: ... case 128..=255: ... }` 覆盖整个 u8 值域 [0, 255]，应编译通过。
///
/// 注：i8 的负数字面量在 range pattern 中暂不被解析器支持，故使用 u8 验证区间合并穷尽性。
#[test]
fn u8_range_full_coverage_passes() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4200 });
    compiler
        .compile_source(
            r#"
micro describe(n: u8) -> utf8 {
    match n {
        case 0..=127:
            "low"
        case 128..=255:
            "high"
    }
}
"#,
        )
        .expect("two ranges covering [0, 255] should be exhaustive for u8");
}

/// `match n { case 1: ... case 2: ... }` 无 wildcard，i32 大值域不可能穷尽，应报 non-exhaustive。
#[test]
fn i32_no_wildcard_reports_non_exhaustive() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4200 });
    let error = compiler
        .compile_source(
            r#"
micro describe(n: i32) -> utf8 {
    match n {
        case 1:
            "one"
        case 2:
            "two"
    }
}
"#,
        )
        .expect_err("i32 without wildcard cannot be exhaustive");
    let message = error.to_string();
    assert!(message.contains("non exhaustive"), "unexpected error: {message}");
}

/// `match n { case 1: ... case 1: ... }` 相同字面量出现两次，应报 duplicate。
#[test]
fn duplicate_literal_detected() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4200 });
    let error = compiler
        .compile_source(
            r#"
micro describe(n: i32) -> utf8 {
    match n {
        case 1:
            "one"
        case 1:
            "again"
    }
}
"#,
        )
        .expect_err("duplicate literal arm should be detected");
    let message = error.to_string();
    assert!(message.contains("duplicate"), "unexpected error: {message}");
}

/// `match n { case 0..=10: ... case 5..=15: ... }` 两个 range 区间相交，应报 overlap。
#[test]
fn range_overlap_detected() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4200 });
    let error = compiler
        .compile_source(
            r#"
micro describe(n: i32) -> utf8 {
    match n {
        case 0..=10:
            "low"
        case 5..=15:
            "high"
    }
}
"#,
        )
        .expect_err("overlapping ranges should be detected");
    let message = error.to_string();
    assert!(message.contains("overlap"), "unexpected error: {message}");
}

/// `match b { case true if true: ... case false if true: ... }` 两个 arm 均带 guard，
/// guard 可能失败，不构成无条件覆盖，应报 non-exhaustive。
#[test]
fn guard_does_not_count_as_full_coverage_for_literal() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4200 });
    let error = compiler
        .compile_source(
            r#"
micro describe(b: bool) -> utf8 {
    match b {
        case true if true:
            "true"
        case false if true:
            "false"
    }
}
"#,
        )
        .expect_err("guarded literal arms should not count as full coverage");
    let message = error.to_string();
    assert!(message.contains("non exhaustive"), "unexpected error: {message}");
}
