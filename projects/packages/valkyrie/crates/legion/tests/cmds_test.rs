//! `legion test` 集成测试。

mod support;

use std::{fs, process::ExitCode};

use legion::cmds::{
    test::{TestArgs, run},
    test_engine::{discover_project_tests, resolve_test_targets},
};

use support::create_smoke_project_with_manifest;

fn write_test_suite(project_dir: &std::path::Path) {
    let test_dir = project_dir.join("test");
    fs::create_dir_all(&test_dir).unwrap();
    fs::write(
        test_dir.join("suite.v"),
        r#"[test]
micro add_two() -> i64 {
    return 2
}

[test]
micro always_pass() -> i64 {
    return 0
}

[benchmark]
micro fib_bench() -> i64 {
    return 1
}
"#,
    )
    .unwrap();
}

#[test]
fn discover_finds_test_and_benchmark() {
    let fixture = create_smoke_project_with_manifest(
        "legion-test-discover",
        r#"{
    name: "test_app",
    build: [ { target: "legion" } ]
}
"#,
        r#"micro main() -> i64 {
    return 0
}
"#,
    );
    write_test_suite(&fixture.project_dir);
    let functions = discover_project_tests(&fixture.project_dir);
    assert_eq!(functions.iter().filter(|f| f.is_test).count(), 2);
    assert_eq!(functions.iter().filter(|f| f.is_benchmark).count(), 1);
}

#[test]
fn resolve_targets_defaults_and_all() {
    assert_eq!(resolve_test_targets(None), vec!["legion".to_string()]);
    assert_eq!(resolve_test_targets(Some("all")).len(), 4);
}

#[test]
fn test_command_writes_html_report() {
    let fixture = create_smoke_project_with_manifest(
        "legion-test-report",
        r#"{
    name: "test_app",
    dependencies: {
        "std": false,
        "core": false
    },
    build: [ { target: "legion" } ]
}
"#,
        r#"micro main() -> i64 {
    return 0
}
"#,
    );
    write_test_suite(&fixture.project_dir);

    let _status = run(&TestArgs {
        project_dir: fixture.project_dir.clone(),
        filter: None,
        target: Some("legion".into()),
        runner: Vec::new(),
        verbose: true,
        standalone: false,
    });

    let html_path = fixture.project_dir.join("dist/legion-test/index.html");
    assert!(html_path.is_file(), "missing HTML report at {}", html_path.display());
    let html = fs::read_to_string(&html_path).unwrap();
    assert!(html.contains("Test Report"));
    assert!(html.contains("add_two") || html.contains("always_pass") || html.contains("编译"));
    assert!(!html.contains("asgard-runtime.js"));
    assert!(!html.contains("LEGION_REPORT_SLOT"));
}

#[test]
fn test_filter_skips_non_matching() {
    let fixture = create_smoke_project_with_manifest(
        "legion-test-filter",
        r#"{
    name: "filter_app",
    dependencies: {
        "std": false,
        "core": false
    },
    build: [ { target: "legion" } ]
}
"#,
        r#"micro main() -> i64 {
    return 0
}
"#,
    );
    write_test_suite(&fixture.project_dir);

    let status = run(&TestArgs {
        project_dir: fixture.project_dir.clone(),
        filter: Some("no_such_test".into()),
        target: Some("legion".into()),
        runner: Vec::new(),
        verbose: true,
        standalone: false,
    })
    .unwrap();
    assert_eq!(status, ExitCode::SUCCESS);

    let html = fs::read_to_string(fixture.project_dir.join("dist/legion-test/index.html")).unwrap();
    assert!(html.contains("跳过") || html.contains("skip") || html.contains("被过滤器排除") || html.contains("summary-card"));
}
