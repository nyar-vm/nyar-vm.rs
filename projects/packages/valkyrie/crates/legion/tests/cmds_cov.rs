//! `legion cov` 集成测试。

mod support;

use std::{fs, process::ExitCode};

use legion::cmds::cov::{CovArgs, infer_coverage_features, run};

use support::create_smoke_project_with_manifest;

#[test]
fn coverage_generates_json_and_html() {
    let fixture = create_smoke_project_with_manifest(
        "legion-cov",
        r#"{
    name: "cov_app",
    build: [
        { target: "legion" }
    ]
}
"#,
        r#"micro main() -> i64 {
    return 0;
}
"#,
    );

    let test_dir = fixture.project_dir.join("test");
    fs::create_dir_all(&test_dir).unwrap();
    fs::write(
        test_dir.join("suite.v"),
        r#"[test]
micro add_two() -> unit {
}

[benchmark]
micro fib() -> unit {
}
"#,
    )
    .unwrap();

    let status = run(&CovArgs { project_dir: fixture.project_dir.clone(), verbose: true, standalone: false }).unwrap();
    assert_eq!(status, ExitCode::SUCCESS);

    let json_path = fixture.project_dir.join(".cache/converge/coverage.json");
    let html_path = fixture.project_dir.join("dist/legion-converge/index.html");
    assert!(json_path.is_file(), "missing {}", json_path.display());
    assert!(html_path.is_file(), "missing {}", html_path.display());

    let json = fs::read_to_string(&json_path).unwrap();
    assert!(json.contains("\"covered\""));
    assert!(json.contains("micro"));
    assert!(json.contains("test"));

    let html = fs::read_to_string(&html_path).unwrap();
    assert!(html.contains("Coverage Report"));
    assert!(html.contains("micro 函数"));
    assert!(!html.contains("asgard-runtime.js"));
}

#[test]
fn infer_detects_core_features() {
    let fixture = create_smoke_project_with_manifest(
        "legion-cov-infer",
        r#"{
    name: "infer_app",
    build: [ { target: "legion" } ]
}
"#,
        r#"micro main() -> i64 {
    let x: f64 = 1.0
    match x {
        _ => 0
    }
}
"#,
    );
    let features = infer_coverage_features(&fixture.project_dir);
    assert!(features.iter().any(|f| f == "micro"));
    assert!(features.iter().any(|f| f == "match"));
    assert!(features.iter().any(|f| f == "float"));
}
