//! Migrated from `src/cmds/test_engine/discover.rs` (`tests`).

use super::*;

#[test]
fn discovers_attribute_and_modifier_forms() {
    let source = r#"
[test]
micro add_two() -> unit {
}

test micro sub_two() -> unit {
}

[benchmark]
micro fib_30() -> unit {
}
"#;
    let mut functions = Vec::new();
    discover_in_content(source, false, &mut functions);
    assert_eq!(functions.len(), 3);
    assert!(functions.iter().any(|f| f.name == "add_two" && f.is_test));
    assert!(functions.iter().any(|f| f.name == "sub_two" && f.is_test));
    assert!(functions.iter().any(|f| f.name == "fib_30" && f.is_benchmark));
}

#[test]
fn extract_name_from_micro_line() {
    assert_eq!(extract_function_name("micro add_two() -> unit").as_deref(), Some("add_two"));
    assert_eq!(extract_function_name("tests easy_if()").as_deref(), Some("easy_if"));
}

#[test]
fn discovers_single_script_project_root() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("solution.v"),
        r#"
[test]
micro root_test() -> unit {
}

[benchmark]
micro root_bench() -> unit {
}
"#,
    )
    .unwrap();

    let functions = discover_project_tests(dir.path());
    assert!(functions.iter().any(|f| f.name == "root_test" && f.is_test));
    assert!(functions.iter().any(|f| f.name == "root_bench" && f.is_benchmark));
}
