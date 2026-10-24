//! 源码分流：AWSL 与 V 逻辑分离。

use std::{fs, path::Path};

use miette::{IntoDiagnostic, Result, WrapErr};

/// 加载单个 AWSL 文件内容。
pub fn read_awsl_file(path: &Path) -> Result<String> {
    let content = fs::read_to_string(path).into_diagnostic().wrap_err_with(|| format!("读取 AWSL 失败: {}", path.display()))?;
    Ok(content.strip_prefix('\u{FEFF}').unwrap_or(&content).to_string())
}
