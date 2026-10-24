//! 独立维护性 lint；不得参与 Compiler 或 build/cache 的成功准入。
//!
//! - `VSRC001`: source file too long (warning above soft limit, error above hard limit)
//! - `VSRC002`: encoding corruption (invalid UTF-8, U+FFFD, mojibake, PS escape residue)
//!
//! 源码解码失败由输入层报告；这里的行数和疑似乱码检查仅供维护工具使用。

use std::{
    fs,
    path::{Path, PathBuf},
};

use miette::{IntoDiagnostic, Result, miette};

/// Soft ceiling for `.v` / `.vx` source size (lines). Exceeding emits `VSRC001` at warning.
pub const SOURCE_LINE_WARN: usize = 1024;
/// Hard ceiling for `.v` / `.vx` source size (lines). Exceeding emits `VSRC001` at error.
pub const SOURCE_LINE_ERROR: usize = 4096;

/// Aggregated hygiene findings for one scan pass.
#[derive(Debug, Default, Clone)]
pub struct HygieneReport {
    /// `VSRC001` warnings (over soft line limit).
    pub warnings: usize,
    /// Hard failures (`VSRC001` error and any `VSRC002`).
    pub errors: usize,
}

impl HygieneReport {
    /// Merge another report into this one.
    pub fn merge(&mut self, other: HygieneReport) {
        self.warnings += other.warnings;
        self.errors += other.errors;
    }

    /// True when any hard failure was recorded.
    pub fn has_errors(&self) -> bool {
        self.errors > 0
    }
}

/// Read one source path and emit hygiene diagnostics.
pub fn scan_source_path(path: &Path) -> Result<HygieneReport> {
    let bytes = fs::read(path).into_diagnostic().map_err(|e| miette!("读取源码失败 {}：{e}", path.display()))?;
    match std::str::from_utf8(&bytes) {
        Ok(source) => Ok(scan_source_text(path, source)),
        Err(error) => {
            eprintln!(
                "VSRC002 error: {}: invalid UTF-8 at byte {} (encoding corruption; re-save as UTF-8)",
                path.display(),
                error.valid_up_to()
            );
            Ok(HygieneReport { warnings: 0, errors: 1 })
        }
    }
}

/// Emit hygiene diagnostics for already-decoded source text.
pub fn scan_source_text(path: &Path, source: &str) -> HygieneReport {
    let mut warnings = 0usize;
    let mut errors = 0usize;
    let line_count = source.lines().count();

    // VSRC001: 超过硬限制统一失败，不按 target 或文件名降级。
    if line_count > SOURCE_LINE_ERROR {
        errors += 1;
        eprintln!(
            "VSRC001 error: {} has {line_count} lines (hard limit {SOURCE_LINE_ERROR}; warn at {SOURCE_LINE_WARN}); split the module",
            path.display()
        );
    }
    else if line_count > SOURCE_LINE_WARN {
        warnings += 1;
        eprintln!("VSRC001 warning: {} has {line_count} lines (soft limit {SOURCE_LINE_WARN}; hard limit {SOURCE_LINE_ERROR})", path.display());
    }

    for (idx, line) in source.lines().enumerate() {
        if looks_like_encoding_corruption(line) {
            errors += 1;
            eprintln!(
                "VSRC002 error: {}:{line_no}: source encoding corruption (mojibake / replacement char / PS escape residue)",
                path.display(),
                line_no = idx + 1
            );
        }
    }

    HygieneReport { warnings, errors }
}

fn looks_like_encoding_corruption(line: &str) -> bool {
    // Primary signal: U+FFFD already baked in (truncated multi-byte CJK, console/PS mangling).
    if line.contains('\u{FFFD}') {
        return true;
    }
    // Classic GBK/UTF-8 mis-decode placeholders.
    if line.contains("锟斤拷") || line.contains("Ã©") || line.contains("â€™") {
        return true;
    }
    // Secondary: PowerShell escape residue dumped as literal text.
    line.contains("`r`n") || line.contains("`n`r")
}
#[cfg(test)]
#[path = "../../tests/path_wired/cmds_source_hygiene.rs"]
mod cmds_source_hygiene_tests;
