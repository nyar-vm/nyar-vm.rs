//! First-party JavaScript / TypeScript lint rules.

use std::path::{Path, PathBuf};

/// Lint options.
#[derive(Debug, Clone, Default)]
pub struct JavascriptLintOptions {
    /// Apply safe autofixes (loose equality → strict).
    pub fix: bool,
}

/// One lint finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavascriptLintIssue {
    /// 1-based line.
    pub line: usize,
    /// Message.
    pub message: String,
    /// Whether an autofix was applied to the buffer.
    pub fixed: bool,
}

/// Lint `source`; returns rewritten text + issues.
pub fn lint_javascript_source(source: &str, options: &JavascriptLintOptions) -> (String, Vec<JavascriptLintIssue>) {
    let mut out_lines = Vec::new();
    let mut issues = Vec::new();

    for (idx, line) in source.split_inclusive('\n').enumerate() {
        let line_no = idx + 1;
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
        let trimmed = body.trim();
        let mut rewritten = body.to_string();

        if trimmed == "debugger" || trimmed == "debugger;" {
            issues.push(JavascriptLintIssue { line: line_no, message: "avoid `debugger`".into(), fixed: false });
        }

        if !trimmed.starts_with("//") && !trimmed.starts_with('*') {
            if let Some(next) = try_fix_loose_equality(&rewritten) {
                if next != rewritten {
                    rewritten = next;
                    issues.push(JavascriptLintIssue { line: line_no, message: "prefer `===` / `!==` over `==` / `!=`".into(), fixed: true });
                }
            }
            else if has_loose_equality(trimmed) {
                issues.push(JavascriptLintIssue { line: line_no, message: "prefer `===` / `!==` over `==` / `!=`".into(), fixed: false });
            }
        }

        if rewritten.contains('\t') && rewritten.contains("  ") {
            issues.push(JavascriptLintIssue { line: line_no, message: "mixed tabs and spaces".into(), fixed: false });
        }

        if options.fix {
            out_lines.push(format!("{rewritten}{newline}"));
        }
        else {
            out_lines.push(line.to_string());
        }
    }

    (out_lines.concat(), issues)
}

/// Lint a file path; optionally write fixes.
pub fn lint_javascript_path(path: &Path, options: &JavascriptLintOptions) -> Result<(String, Vec<JavascriptLintIssue>), std::io::Error> {
    let source = std::fs::read_to_string(path)?;
    let (fixed, issues) = lint_javascript_source(&source, options);
    if options.fix && fixed != source {
        std::fs::write(path, &fixed)?;
    }
    Ok((fixed, issues))
}

/// Collect JS/TS files under `roots`.
pub fn collect_javascript_files(roots: &[PathBuf]) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut files = Vec::new();
    for root in roots {
        collect(root, &mut files)?;
    }
    Ok(files)
}

fn has_loose_equality(line: &str) -> bool {
    (contains_op(line, "==") && !contains_op(line, "===")) || (contains_op(line, "!=") && !contains_op(line, "!=="))
}

fn contains_op(line: &str, op: &str) -> bool {
    line.contains(op)
}

fn try_fix_loose_equality(line: &str) -> Option<String> {
    let mut chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let mut changed = false;
    while i + 1 < chars.len() {
        if chars[i] == '=' && chars[i + 1] == '=' {
            let prev_eq = i > 0 && chars[i - 1] == '=';
            let next_eq = i + 2 < chars.len() && chars[i + 2] == '=';
            if !prev_eq && !next_eq {
                chars.insert(i + 2, '=');
                changed = true;
                i += 3;
                continue;
            }
        }
        if chars[i] == '!' && chars[i + 1] == '=' {
            let next_eq = i + 2 < chars.len() && chars[i + 2] == '=';
            if !next_eq {
                chars.insert(i + 2, '=');
                changed = true;
                i += 3;
                continue;
            }
        }
        i += 1;
    }
    if changed { Some(chars.into_iter().collect()) } else { None }
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), std::io::Error> {
    if path.is_file() {
        if is_js_like(path) {
            out.push(path.to_path_buf());
        }
        return Ok(());
    }
    if !path.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let p = entry.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if matches!(name, "node_modules" | "dist" | ".git" | "coverage" | ".next" | "build") {
            continue;
        }
        collect(&p, out)?;
    }
    Ok(())
}

fn is_js_like(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
        Some("js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixes_loose_equality() {
        let (out, issues) = lint_javascript_source("if (a == b) {\n}\n", &JavascriptLintOptions { fix: true });
        assert!(out.contains("==="));
        assert_eq!(issues.len(), 1);
        assert!(issues[0].fixed);
    }
}
