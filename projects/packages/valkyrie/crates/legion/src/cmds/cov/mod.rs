//! `legion cov` / `legion coverage` — 语法特性覆盖矩阵。

use legion_workspace::planner::{WorkspaceResolver, collect_project_v_files};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::Args;
use miette::{IntoDiagnostic, Result, WrapErr, miette};
use serde_json::json;

use crate::{
    cmds::{
        project_input::resolve_project_path,
        report::{CoverageFeatureEntry, CoverageReport, atomic_write_all_text, finish_standalone_report, render_coverage_report},
    },
    script,
};

/// 已知语法特性（标识 → 显示名）。
pub fn known_features() -> Vec<(&'static str, &'static str)> {
    vec![
        ("micro", "micro 函数"),
        ("mezzo", "mezzo 函数"),
        ("structure", "structure 值类型"),
        ("class", "class 继承"),
        ("enums", "enums 枚举"),
        ("flags", "flags 位标志"),
        ("union", "union 联合类型"),
        ("unite", "unite 紧凑联合"),
        ("trait", "trait"),
        ("match", "模式匹配"),
        ("loop", "控制流循环"),
        ("closure", "闭包/lambda"),
        ("pipe", "管道表达式"),
        ("nullable", "可空类型"),
        ("integer", "整数类型"),
        ("float", "浮点类型"),
        ("if-expr", "if 表达式"),
        ("multi-file", "多文件编译"),
        ("test", "测试框架"),
        ("benchmark", "基准测试"),
        ("async", "异步 await/awake/block"),
        ("effect", "代数效应 raise/catch/resume"),
        ("macro", "macro 编译期赋值"),
    ]
}

/// `legion cov` 命令参数。
#[derive(Debug, Clone, Args)]
pub struct CovArgs {
    /// 项目或 workspace 目录。
    #[arg(value_name = "project-dir", default_value = ".")]
    pub project_dir: PathBuf,
    /// 详细输出。
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,
    /// 额外产出单文件 HTML（内联 css/js/wasm，便于离线打开）。
    #[arg(long, default_value_t = false)]
    pub standalone: bool,
}

/// 执行 `legion cov` / `legion coverage`。
pub fn run(args: &CovArgs) -> Result<ExitCode> {
    let project_input = resolve_project_path(&args.project_dir)?;
    let workspace = WorkspaceResolver::discover_for_project(&project_input).ok();
    let scan_root = if script::is_script_path(&project_input) {
        project_input.parent().map(Path::to_path_buf).unwrap_or_else(|| project_input.clone())
    }
    else {
        project_input.clone()
    };

    let (workspace_dir, members) = if let Some(workspace) = &workspace {
        if workspace.workspace_manifest.is_some() && same_path(&scan_root, &workspace.root_dir) {
            let members = workspace.member_manifest_dirs();
            if members.is_empty() {
                return Err(miette!("legions.von 中无 members"));
            }
            (workspace.root_dir.clone(), members)
        }
        else {
            (scan_root.clone(), vec![scan_root.clone()])
        }
    }
    else {
        (scan_root.clone(), vec![scan_root.clone()])
    };

    run_coverage_for_members(&workspace_dir, &members, args.verbose, args.standalone)
}

fn run_coverage_for_members(workspace_dir: &Path, members: &[PathBuf], verbose: bool, standalone: bool) -> Result<ExitCode> {
    let known = known_features();
    let mut coverage_map: BTreeMap<&str, (bool, Vec<String>)> = BTreeMap::new();
    for (feature, _) in &known {
        coverage_map.insert(*feature, (false, Vec::new()));
    }

    for member_dir in members {
        if !member_dir.exists() {
            continue;
        }
        let member_name = member_dir.file_name().and_then(|n| n.to_str()).unwrap_or("member").to_string();
        if verbose {
            println!("  扫描 {member_name} ...");
        }
        let inferred = infer_coverage_features(member_dir);
        for feature in inferred {
            if let Some(entry) = coverage_map.get_mut(feature.as_str()) {
                if !entry.1.iter().any(|p| p == &member_name) {
                    entry.1.push(member_name.clone());
                }
                entry.0 = true;
            }
        }
    }

    println!();
    println!("语法覆盖报告：");
    println!("{}", "-".repeat(72));
    println!("{:<24} {:<8} {}", "特性", "覆盖", "测试项目");
    println!("{}", "-".repeat(72));

    let mut covered_count = 0usize;
    let mut features = Vec::new();
    for (feature, display) in &known {
        let (is_covered, projects) = coverage_map.get(feature).cloned().unwrap_or((false, Vec::new()));
        let status = if is_covered { "✓" } else { "✗" };
        let projects_text = if !is_covered {
            "-".to_string()
        }
        else if projects.len() > 3 {
            format!("{} ... +{}", projects.iter().take(3).cloned().collect::<Vec<_>>().join(", "), projects.len() - 3)
        }
        else {
            projects.join(", ")
        };
        println!("{display:<24} {status:<8} {projects_text}");
        if is_covered {
            covered_count += 1;
        }
        features.push(CoverageFeatureEntry { feature: (*feature).to_string(), display: (*display).to_string(), covered: is_covered, projects });
    }

    let total = known.len();
    let pct = if total == 0 { 0.0 } else { covered_count as f64 / total as f64 * 100.0 };
    println!("{}", "-".repeat(72));
    println!("覆盖率：{covered_count}/{total}（{pct:.0}%）");

    let report = CoverageReport { covered: covered_count, total, percentage: pct, features };

    let cache_dir = workspace_dir.join(".cache").join("converge");
    fs::create_dir_all(&cache_dir).into_diagnostic().wrap_err_with(|| format!("创建缓存目录失败 {}", cache_dir.display()))?;
    let json_path = cache_dir.join("coverage.json");
    let json_content = build_coverage_json(&report);
    atomic_write_all_text(&json_path, &json_content)?;

    let report_dir = workspace_dir.join("dist").join("legion-converge");
    render_coverage_report(&report_dir, &report)?;
    println!("HTML 覆盖率报告已生成：{}", report_dir.join("index.html").display());
    finish_standalone_report(&report_dir, standalone)?;

    Ok(ExitCode::SUCCESS)
}

/// 推断项目使用的语法特性。
pub fn infer_coverage_features(project_dir: &Path) -> Vec<String> {
    let Ok(files) = collect_project_v_files(project_dir)
    else {
        return Vec::new();
    };

    let mut features = Vec::new();
    let mut add_once = |f: &str| {
        if !features.iter().any(|x| x == f) {
            features.push(f.to_string());
        }
    };

    for file in files {
        let Ok(content) = fs::read_to_string(&file)
        else {
            continue;
        };
        if content.contains("test micro ") || content.contains("tests ") || content.contains("[test]") {
            add_once("test");
        }
        if content.contains("micro ") {
            add_once("micro");
        }
        if content.contains("mezzo ") {
            add_once("mezzo");
        }
        if content.contains("macro ") {
            add_once("macro");
        }
        if content.contains("structure ") {
            add_once("structure");
        }
        if content.contains("class ") && content.contains('(') {
            add_once("class");
        }
        if line_has_type_declaration(&content, "enums") {
            add_once("enums");
        }
        if line_has_type_declaration(&content, "flags") {
            add_once("flags");
        }
        if line_has_type_declaration(&content, "union") {
            add_once("union");
        }
        if content.contains("unite ") {
            add_once("unite");
        }
        if content.contains("trait ") {
            add_once("trait");
        }
        if content.contains("match ") {
            add_once("match");
        }
        if content.contains("while ") || content.contains("loop ") || content.contains("for ") {
            add_once("loop");
        }
        if content.contains("=> ") || content.contains(".filter(") || content.contains(".map(") {
            add_once("closure");
        }
        if content.contains("|>") {
            add_once("pipe");
        }
        if content.contains(".await") || content.contains(".awake") || content.contains(".block") || content.contains("trait Future") {
            add_once("async");
        }
        if content.contains("raise ") || content.contains("catch ") || content.contains("resume ") {
            add_once("effect");
        }
        if has_nullable_type_syntax(&content) {
            add_once("nullable");
        }
        if content.contains(": i8") || content.contains(": i16") || content.contains(": i64") || content.contains(": u8") {
            add_once("integer");
        }
        if content.contains(": f32") || content.contains(": f64") {
            add_once("float");
        }
        if content.contains("= if ") && content.contains("else") {
            add_once("if-expr");
        }
        if content.contains("using ") {
            add_once("multi-file");
        }
        if content.contains("[benchmark]") {
            add_once("benchmark");
        }
    }

    features
}

fn line_has_type_declaration(content: &str, keyword: &str) -> bool {
    let prefix = format!("{keyword} ");
    content.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with(&prefix) && !trimmed[prefix.len()..].starts_with('=')
    })
}

fn has_nullable_type_syntax(content: &str) -> bool {
    content.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.contains("?") && (trimmed.contains(": ") || trimmed.contains("->")) && !trimmed.contains("?.") && !trimmed.starts_with("//")
    })
}

fn build_coverage_json(report: &CoverageReport) -> String {
    let features: Vec<_> = report
        .features
        .iter()
        .map(|f| {
            json!({
                "feature": f.feature,
                "display": f.display,
                "covered": f.covered,
                "projects": f.projects,
            })
        })
        .collect();
    serde_json::to_string_pretty(&json!({
        "covered": report.covered,
        "total": report.total,
        "percentage": (report.percentage * 10.0).round() / 10.0,
        "features": features,
    }))
    .unwrap_or_else(|_| "{}".into())
}

fn same_path(left: &Path, right: &Path) -> bool {
    let left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    left == right
}
#[cfg(test)]
#[path = "../../../tests/path_wired/cmds_cov_mod.rs"]
mod cmds_cov_mod_tests;
