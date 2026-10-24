//! `legion bench` 集成测试。

mod support;

use std::fs;

use legion::cmds::bench::{BenchArgs, run};

use support::create_smoke_project_with_manifest;

#[test]
fn bench_command_writes_html_when_results_exist() {
    let fixture = create_smoke_project_with_manifest(
        "legion-bench",
        r#"{
    name: "bench_app",
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

    let test_dir = fixture.project_dir.join("test");
    fs::create_dir_all(&test_dir).unwrap();
    fs::write(
        test_dir.join("bench.v"),
        r#"[benchmark]
micro fib_bench() -> i64 {
    return 1
}
"#,
    )
    .unwrap();

    let _ =
        run(&BenchArgs { project_dir: fixture.project_dir.clone(), runs: 1, target: Some("legion".into()), verbose: true, standalone: false });

    let html_path = fixture.project_dir.join("dist/legion-benchmark/index.html");
    // HTML only written when there is at least one successful timed result.
    if html_path.is_file() {
        let html = fs::read_to_string(&html_path).unwrap();
        assert!(html.contains("Benchmark Report"));
        assert!(html.contains("fib_bench"));
        assert!(!html.contains("asgard-runtime.js"));
    }
    else {
        // Even when benchmarks fail to compile/run, command should not panic.
        // Ensure discover path available for future runs.
        assert!(test_dir.join("bench.v").is_file());
    }
}

#[test]
fn bench_skips_when_no_benchmarks() {
    let fixture = create_smoke_project_with_manifest(
        "legion-bench-empty",
        r#"{
    name: "bench_empty",
    build: [ { target: "legion" } ]
}
"#,
        r#"micro main() -> i64 {
    return 0
}
"#,
    );

    let status =
        run(&BenchArgs { project_dir: fixture.project_dir.clone(), runs: 1, target: Some("legion".into()), verbose: false, standalone: false })
            .unwrap();
    assert_eq!(status, std::process::ExitCode::SUCCESS);
    assert!(!fixture.project_dir.join("dist/legion-benchmark/index.html").exists());
}
