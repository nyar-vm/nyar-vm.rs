//! 生成本地 HTML 报告预览（`target/report-preview/`）。

use std::path::PathBuf;

use legion::cmds::report::{
    BenchReport, BenchResultRow, CoverageFeatureEntry, CoverageReport, TestResultEntry, render_bench_report, render_coverage_report,
    render_test_report,
};

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/report-preview");

    render_test_report(
        &root.join("legion-test"),
        "demo.project",
        &[
            TestResultEntry { name: "add_two".into(), status: "pass".into(), error: None, target: "legion".into() },
            TestResultEntry { name: "sub_two".into(), status: "pass".into(), error: None, target: "clr".into() },
            TestResultEntry {
                name: "div_zero".into(),
                status: "fail".into(),
                error: Some("assertion failed: 1 / 0 == 1".into()),
                target: "legion".into(),
            },
            TestResultEntry {
                name: "parse_json".into(),
                status: "compile_error".into(),
                error: Some("expected `;`, found `}`".into()),
                target: "jvm".into(),
            },
            TestResultEntry {
                name: "bench_seed".into(),
                status: "skip".into(),
                error: Some("[benchmark] 标注".into()),
                target: "legion".into(),
            },
            TestResultEntry {
                name: "filtered_case".into(), status: "skip".into(), error: Some("被过滤器排除".into()), target: "node".into()
            },
        ],
    )
    .expect("test report");

    let features = [
        ("micro", "micro 函数", true, vec!["demo.project", "std.core"]),
        ("mezzo", "mezzo 函数", true, vec!["demo.project"]),
        ("structure", "structure 值类型", true, vec!["demo.project"]),
        ("class", "class 继承", false, vec![]),
        ("enums", "enums 枚举", true, vec!["std.enums"]),
        ("flags", "flags 位标志", false, vec![]),
        ("union", "union 联合类型", false, vec![]),
        ("unite", "unite 紧凑联合", false, vec![]),
        ("trait", "trait", true, vec!["demo.project"]),
        ("match", "模式匹配", true, vec!["demo.project", "std.core"]),
        ("loop", "控制流循环", true, vec!["demo.project"]),
        ("closure", "闭包/lambda", false, vec![]),
        ("pipe", "管道表达式", true, vec!["demo.project"]),
        ("nullable", "可空类型", false, vec![]),
        ("integer", "整数类型", true, vec!["demo.project"]),
        ("float", "浮点类型", true, vec!["math.lib"]),
        ("if-expr", "if 表达式", true, vec!["demo.project"]),
        ("multi-file", "多文件编译", true, vec!["demo.project"]),
        ("test", "测试框架", true, vec!["demo.project"]),
        ("benchmark", "基准测试", true, vec!["demo.project"]),
    ];
    let covered = features.iter().filter(|f| f.2).count();
    let total = features.len();
    let cov = CoverageReport {
        covered,
        total,
        percentage: covered as f64 / total as f64 * 100.0,
        features: features
            .into_iter()
            .map(|(feature, display, covered, projects)| CoverageFeatureEntry {
                feature: feature.into(),
                display: display.into(),
                covered,
                projects: projects.into_iter().map(str::to_string).collect(),
            })
            .collect(),
    };
    render_coverage_report(&root.join("legion-converge"), &cov).expect("cov report");

    let bench = BenchReport {
        runs: 3,
        rows: vec![
            BenchResultRow {
                project: "demo.project".into(),
                test: "fib_30".into(),
                target: "legion".into(),
                compile_ms: 42.3,
                runtime_ms: 1.8,
            },
            BenchResultRow { project: "demo.project".into(), test: "fib_30".into(), target: "clr".into(), compile_ms: 118.5, runtime_ms: 0.4 },
            BenchResultRow {
                project: "math.lib".into(),
                test: "matrix_mul".into(),
                target: "legion".into(),
                compile_ms: 55.0,
                runtime_ms: 12.6,
            },
            BenchResultRow { project: "math.lib".into(), test: "matrix_mul".into(), target: "jvm".into(), compile_ms: 210.2, runtime_ms: 8.1 },
        ],
    };
    render_bench_report(&root.join("legion-benchmark"), &bench).expect("bench report");

    println!("reports written to {}", root.display());
    println!("  test:  {}/legion-test/index.html", root.display());
    println!("  cov:   {}/legion-converge/index.html", root.display());
    println!("  bench: {}/legion-benchmark/index.html", root.display());
}
