//! `asgard add` / `asgard remove` — 编辑 `legion.von` 依赖。

use std::{fs, path::Path};

use miette::{IntoDiagnostic, Result, WrapErr};

/// 向 `legion.von` 添加依赖。
pub fn add_dependency(project_dir: &Path, name: &str, version: &str) -> Result<()> {
    let path = project_dir.join("legion.von");
    let mut content = if path.exists() {
        fs::read_to_string(&path).into_diagnostic()?
    }
    else {
        format!(
            r#"{{
    name: "{}",
    dependencies: {{}}
}}
"#,
            project_dir.file_name().and_then(|n| n.to_str()).unwrap_or("app")
        )
    };
    let entry = format!("\n        {name}: \"{version}\",");
    if content.contains(&format!("{name}:")) {
        return Err(miette::miette!("依赖 `{name}` 已存在"));
    }
    if let Some(idx) = content.find("dependencies:") {
        if let Some(open) = content[idx..].find('{') {
            let insert_at = idx + open + 1;
            content.insert_str(insert_at, &entry);
        }
        else {
            content.push_str(&format!("\n    dependencies: {{{entry}\n    }}"));
        }
    }
    else {
        content.push_str(&format!("\n    dependencies: {{{entry}\n    }}"));
    }
    fs::write(&path, content).into_diagnostic().wrap_err("写入 legion.von 失败")?;
    Ok(())
}

/// 从 `legion.von` 移除依赖。
pub fn remove_dependency(project_dir: &Path, name: &str) -> Result<()> {
    let path = project_dir.join("legion.von");
    let content = fs::read_to_string(&path).into_diagnostic().wrap_err("读取 legion.von 失败")?;
    let needle = format!("{name}:");
    if !content.contains(&needle) {
        return Err(miette::miette!("依赖 `{name}` 不存在"));
    }
    let filtered: String = content.lines().filter(|line| !line.trim().starts_with(&needle)).collect::<Vec<_>>().join("\n");
    fs::write(&path, filtered).into_diagnostic()?;
    Ok(())
}
#[cfg(test)]
#[path = "../tests/path_wired/deps.rs"]
mod deps_tests;
