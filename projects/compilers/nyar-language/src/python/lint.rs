//! First-party Python lint rules.

use std::path::{Path, PathBuf};

/// Lint options.
#[derive(Debug, Clone, Default)]
pub struct PythonLintOptions {
    /// Reserved for future autofixes.
    pub fix: bool,
}

/// One lint finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonLintIssue {
    /// 1-based line.
    pub line: usize,
    /// Message.
    pub message: String,
}

/// Lint Python source text.
pub fn lint_python_source(source: &str, _options: &PythonLintOptions) -> Vec<PythonLintIssue> {
    let mut issues = Vec::new();
    for (idx, line) in source.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = line.trim();
        if trimmed == "import *" || (trimmed.starts_with("from ") && trimmed.ends_with(" import *")) {
            issues.push(PythonLintIssue { line: line_no, message: "avoid star imports".into() });
        }
        if trimmed == "breakpoint()" || trimmed.starts_with("pdb.set_trace(") {
            issues.push(PythonLintIssue { line: line_no, message: "avoid leftover debugger calls".into() });
        }
        if line.chars().count() > 120 {
            issues.push(PythonLintIssue { line: line_no, message: "line longer than 120 characters".into() });
        }
        if line.contains('\t') && line.contains("    ") {
            issues.push(PythonLintIssue { line: line_no, message: "mixed tabs and spaces".into() });
        }
    }
    issues
}

/// Lint a file path.
pub fn lint_python_path(path: &Path, options: &PythonLintOptions) -> Result<Vec<PythonLintIssue>, std::io::Error> {
    let source = std::fs::read_to_string(path)?;
    Ok(lint_python_source(&source, options))
}

/// Collect `.py` files under `roots`.
pub fn collect_python_files(roots: &[PathBuf]) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut files = Vec::new();
    for root in roots {
        collect(root, &mut files)?;
    }
    Ok(files)
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), std::io::Error> {
    if path.is_file() {
        if path.extension().and_then(|e| e.to_str()) == Some("py") {
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
        if matches!(name, ".venv" | "venv" | ".git" | "__pycache__" | "dist" | "build" | ".eggs") {
            continue;
        }
        collect(&p, out)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_star_import() {
        let issues = lint_python_source("from os import *\n", &PythonLintOptions::default());
        assert_eq!(issues.len(), 1);
    }
}
