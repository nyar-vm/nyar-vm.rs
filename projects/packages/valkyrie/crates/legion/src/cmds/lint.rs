use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::Args;
use miette::{IntoDiagnostic, Result, miette};
use rusty_valkyrie::{ValkyrieCompiler, lint_valkyrie_source, lint_valkyrie_vx_source};

use crate::cmds::source_hygiene;
use legion_workspace::planner::WorkspaceResolver;

/// `legion lint` arguments.
#[derive(Debug, Clone, Args)]
pub struct LintArgs {
    /// Project directory.
    #[arg(value_name = "project-dir", default_value = ".")]
    pub project_dir: PathBuf,
    /// Lint all workspace members.
    #[arg(long, default_value_t = false)]
    pub workspace: bool,
    /// Lint target format (`awsl` for AWSL style rules; default: Valkyrie `.v`/`.vx` compile lint).
    #[arg(long, value_name = "FORMAT")]
    pub format: Option<String>,
    /// Apply auto-fixes for the selected format.
    #[arg(long, default_value_t = false)]
    pub fix: bool,
}

/// Run `legion lint`.
pub fn run(args: &LintArgs) -> Result<ExitCode> {
    let lint_awsl = args.format.as_deref().is_some_and(|f| f.eq_ignore_ascii_case("awsl"));

    let mut files = Vec::new();
    if args.workspace {
        let workspace = WorkspaceResolver::discover(&args.project_dir)?;
        for member in workspace.member_manifest_dirs() {
            collect_lint_files(&member, lint_awsl, &mut files)?;
        }
    }
    else {
        collect_lint_files(&args.project_dir, lint_awsl, &mut files)?;
    }
    files.sort();
    files.dedup();

    if files.is_empty() {
        let hint = if lint_awsl {
            "没有找到可 lint 的 AWSL 文件（*.awsl）"
        }
        else {
            "没有找到可 lint 的源码文件（source/test/script 下 *.v|*.vx；AWSL 请使用 --format awsl）"
        };
        return Err(miette!("{hint}"));
    }

    let compiler = ValkyrieCompiler::default();
    let mut failed = 0usize;
    let mut awsl_issues = 0usize;
    let mut hygiene_errors = 0usize;
    let mut hygiene_warnings = 0usize;

    for file in files {
        let bytes = fs::read(&file).into_diagnostic().map_err(|e| e.wrap_err(format!("读取源码失败 {}", file.display())))?;

        let source = match std::str::from_utf8(&bytes) {
            Ok(text) => text.to_owned(),
            Err(_) if !lint_awsl => {
                let report = source_hygiene::scan_source_path(&file)?;
                hygiene_errors += report.errors;
                hygiene_warnings += report.warnings;
                failed += report.errors;
                continue;
            }
            Err(error) => {
                return Err(miette!("{}: invalid UTF-8 at byte {}", file.display(), error.valid_up_to()));
            }
        };

        if !lint_awsl {
            let hygiene = source_hygiene::scan_source_text(&file, &source);
            hygiene_warnings += hygiene.warnings;
            hygiene_errors += hygiene.errors;
            failed += hygiene.errors;
        }

        match file.extension().and_then(|e| e.to_str()) {
            Some("awsl") if lint_awsl => {
                let (fixed, issues) = lint_awsl_redundant_parens_in_controls(&source);
                if issues > 0 {
                    awsl_issues += issues;
                    if args.fix {
                        fs::write(&file, fixed).into_diagnostic().map_err(|e| e.wrap_err(format!("写入修复结果失败 {}", file.display())))?;
                        println!("fixed: {} ({issues})", file.display());
                    }
                    else {
                        eprintln!("awsl lint: {} ({issues})", file.display());
                    }
                }
            }
            Some("vx") if !lint_awsl => {
                if let Err(error) = lint_valkyrie_vx_source(&compiler, &source) {
                    failed += 1;
                    eprintln!("lint error: {} -> {}", file.display(), error);
                }
            }
            Some("v") if !lint_awsl => {
                if let Err(error) = lint_valkyrie_source(&compiler, &source) {
                    failed += 1;
                    eprintln!("lint error: {} -> {}", file.display(), error);
                }
            }
            _ => {}
        };
    }

    if failed == 0 && awsl_issues == 0 {
        if hygiene_warnings > 0 {
            println!("lint ok ({hygiene_warnings} hygiene warning(s))");
        }
        else {
            println!("lint ok");
        }
        Ok(ExitCode::SUCCESS)
    }
    else if lint_awsl && args.fix && awsl_issues > 0 {
        println!("lint fixed: {awsl_issues} issue(s)");
        Ok(ExitCode::SUCCESS)
    }
    else if lint_awsl {
        Err(miette!("lint failed: awsl style issues: {awsl_issues} (run with --fix to apply)"))
    }
    else if awsl_issues == 0 {
        Err(miette!("lint failed: {failed} file(s)/issue(s) (hygiene errors: {hygiene_errors}, warnings: {hygiene_warnings})"))
    }
    else {
        Err(miette!("lint failed: {failed} file(s), awsl style issues: {awsl_issues}"))
    }
}

fn collect_lint_files(project_dir: &Path, awsl_only: bool, files: &mut Vec<PathBuf>) -> Result<()> {
    if project_dir.is_file() {
        let Some(ext) = project_dir.extension().and_then(|v| v.to_str())
        else {
            return Ok(());
        };
        if matches_extension(ext, awsl_only) {
            files.push(project_dir.to_path_buf());
        }
        return Ok(());
    }

    // Prefer conventional project layout, but fall back to scanning the directory itself.
    let mut found_conventional_root = false;
    for relative in ["source", "test", "script"] {
        let root = project_dir.join(relative);
        if root.is_dir() {
            found_conventional_root = true;
            collect_files_recursive(&root, awsl_only, files)?;
        }
    }

    if !found_conventional_root {
        collect_files_recursive(project_dir, awsl_only, files)?;
    }
    Ok(())
}

fn collect_files_recursive(root: &Path, awsl_only: bool, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root).into_diagnostic().map_err(|e| e.wrap_err(format!("读取目录失败 {}", root.display())))? {
        let entry = entry.into_diagnostic()?;
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursive(&path, awsl_only, files)?;
            continue;
        }
        let Some(ext) = path.extension().and_then(|value| value.to_str())
        else {
            continue;
        };
        if matches_extension(ext, awsl_only) {
            files.push(path);
        }
    }
    Ok(())
}

fn matches_extension(ext: &str, awsl_only: bool) -> bool {
    if awsl_only { ext.eq_ignore_ascii_case("awsl") } else { ext.eq_ignore_ascii_case("v") || ext.eq_ignore_ascii_case("vx") }
}

fn lint_awsl_redundant_parens_in_controls(source: &str) -> (String, usize) {
    let mut out = String::with_capacity(source.len());
    let mut issues = 0usize;

    let mut in_script = false;
    for line in source.split_inclusive('\n') {
        let raw = line;
        let trimmed = raw.trim();

        if trimmed == "<script>" {
            in_script = true;
            out.push_str(raw);
            continue;
        }
        if trimmed == "</script>" {
            in_script = false;
            out.push_str(raw);
            continue;
        }

        if !in_script {
            out.push_str(raw);
            continue;
        }

        let (fixed, changed) = fix_control_parens_single_line(raw);
        if changed {
            issues += 1;
            out.push_str(&fixed);
        }
        else {
            out.push_str(raw);
        }
    }

    (out, issues)
}

fn fix_control_parens_single_line(line: &str) -> (String, bool) {
    // Target `if (cond) {`, `else if (cond) {`, `while (cond) {` anywhere on the line
    // (including `} else if (cond) {`).

    let newline = if line.ends_with("\r\n") {
        "\r\n"
    }
    else if line.ends_with('\n') {
        "\n"
    }
    else {
        ""
    };
    let body = line.strip_suffix(newline).unwrap_or(line);

    let keywords = ["else if", "while", "if"];
    let mut changed = false;
    let mut result = body.to_string();

    loop {
        let mut best: Option<(usize, usize, String)> = None;
        for kw in keywords {
            let mut search_from = 0usize;
            while let Some(rel) = result[search_from..].find(kw) {
                let start = search_from + rel;
                let after_kw = start + kw.len();
                let rest = result[after_kw..].trim_start();
                let paren_offset = result[after_kw..].len() - rest.len();
                if !rest.starts_with('(') {
                    search_from = after_kw;
                    continue;
                }
                let open = after_kw + paren_offset;
                let Some(close) = find_matching_paren(&result[open..]).map(|i| open + i)
                else {
                    search_from = after_kw;
                    continue;
                };
                let condition = result[open + 1..close].trim();
                let after_close = result[close + 1..].trim_start();
                if !after_close.starts_with('{') {
                    search_from = after_kw;
                    continue;
                }
                let replacement = format!("{kw} {condition}");
                best = Some((start, close + 1, replacement));
                break;
            }
            if best.is_some() {
                break;
            }
        }

        let Some((start, end, replacement)) = best
        else {
            break;
        };
        result.replace_range(start..end, &replacement);
        changed = true;
    }

    if !changed {
        return (line.to_string(), false);
    }
    let rebuilt = if newline.is_empty() { result } else { format!("{result}{newline}") };
    (rebuilt, true)
}

fn find_matching_paren(s: &str) -> Option<usize> {
    if !s.starts_with('(') {
        return None;
    }
    let mut depth = 0i32;
    for (i, ch) in s.char_indices() {
        if ch == '(' {
            depth += 1;
        }
        else if ch == ')' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}
