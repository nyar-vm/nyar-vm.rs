//! 报告文件写出。

use std::{fs, path::Path};

use miette::{IntoDiagnostic, Result, WrapErr};

/// 原子写出文本（临时文件 + rename）。
pub fn atomic_write_all_text(file_path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).into_diagnostic().wrap_err_with(|| format!("创建目录失败 {}", parent.display()))?;
    }

    let parent = file_path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("index.html");
    let temp_path = parent.join(format!(".{file_name}.{}.tmp", std::process::id()));

    fs::write(&temp_path, content).into_diagnostic().wrap_err_with(|| format!("写入临时文件失败 {}", temp_path.display()))?;
    fs::rename(&temp_path, file_path)
        .into_diagnostic()
        .wrap_err_with(|| format!("原子重命名失败 {} -> {}", temp_path.display(), file_path.display()))?;
    Ok(())
}
#[cfg(test)]
#[path = "../../../tests/path_wired/cmds_report_write.rs"]
mod cmds_report_write_tests;
