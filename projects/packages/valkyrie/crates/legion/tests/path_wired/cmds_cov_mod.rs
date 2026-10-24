//! Migrated from `src/cmds/cov/mod.rs` (`tests`).

use super::*;

use tempfile::tempdir;

#[test]
fn infers_micro_and_test() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("main.v"), "micro main() -> unit {}\n").unwrap();
    let test = dir.path().join("test");
    fs::create_dir_all(&test).unwrap();
    fs::write(test.join("t.v"), "[test]\nmicro add() -> unit {}\n").unwrap();
    let features = infer_coverage_features(dir.path());
    assert!(features.iter().any(|f| f == "micro"));
    assert!(features.iter().any(|f| f == "test"));
}

#[test]
fn infers_flags_and_nullable_from_declarations() {
    let dir = tempdir().unwrap();
    let test = dir.path().join("test");
    fs::create_dir_all(&test).unwrap();
    fs::write(
        test.join("matrix.v"),
        r#"
flags FilePerm { Read = 1 }
micro f(x: i64?) -> i64? { x }
[benchmark]
micro bench() -> unit {}
micro g() { future.await }
catch raise "x" { case _: }
"#,
    )
    .unwrap();
    let features = infer_coverage_features(dir.path());
    assert!(features.iter().any(|f| f == "flags"));
    assert!(features.iter().any(|f| f == "nullable"));
    assert!(features.iter().any(|f| f == "benchmark"));
    assert!(features.iter().any(|f| f == "async"));
    assert!(features.iter().any(|f| f == "effect"));
}
