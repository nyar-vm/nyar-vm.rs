//! 测试函数发现（对齐 C# discover_test_functions）。

use std::{fs, path::Path};

/// 已发现的测试/基准函数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredFunction {
    /// 函数名。
    pub name: String,
    /// 是否为 `[test]`。
    pub is_test: bool,
    /// 是否为 `[benchmark]`。
    pub is_benchmark: bool,
    /// 是否仅用于编译期验收（`test/compile_only/`，不参与 test build bundle）。
    pub compile_only: bool,
}

/// 从测试文件列表中发现测试函数。
pub fn discover_test_functions(test_files: &[impl AsRef<Path>]) -> Vec<DiscoveredFunction> {
    let mut functions = Vec::new();
    for file in test_files {
        let path = file.as_ref();
        let Ok(content) = fs::read_to_string(path)
        else {
            continue;
        };
        let compile_only = path.components().any(|component| component.as_os_str() == "compile_only");
        discover_in_content(&content, compile_only, &mut functions);
    }
    functions
}

fn discover_in_content(content: &str, compile_only: bool, functions: &mut Vec<DiscoveredFunction>) {
    let mut pending_attr = "";
    let mut pending_modifier = "";

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("[test]") {
            pending_attr = "test";
            continue;
        }
        if trimmed.starts_with("[benchmark]") {
            pending_attr = "benchmark";
            continue;
        }

        // 同行修饰符：`test micro foo()` / `benchmark micro foo()`。
        if trimmed.starts_with("test micro ") {
            if let Some(name) = extract_function_name(trimmed) {
                functions.push(DiscoveredFunction { name, is_test: true, is_benchmark: false, compile_only });
            }
            pending_attr = "";
            pending_modifier = "";
            continue;
        }
        if trimmed.starts_with("benchmark micro ") {
            if let Some(name) = extract_function_name(trimmed) {
                functions.push(DiscoveredFunction { name, is_test: false, is_benchmark: true, compile_only });
            }
            pending_attr = "";
            pending_modifier = "";
            continue;
        }

        if trimmed.contains(" micro ") || trimmed.starts_with("micro ") {
            let is_test = pending_attr == "test" || pending_modifier == "test";
            let is_benchmark = pending_attr == "benchmark" || pending_modifier == "benchmark";
            if is_test || is_benchmark {
                if let Some(name) = extract_function_name(trimmed) {
                    functions.push(DiscoveredFunction { name, is_test, is_benchmark, compile_only });
                }
            }
            pending_attr = "";
            pending_modifier = "";
            continue;
        }

        if trimmed.starts_with("test ") {
            pending_modifier = "test";
            continue;
        }
        if trimmed.starts_with("benchmark ") {
            pending_modifier = "benchmark";
            continue;
        }

        if trimmed.starts_with("tests ") {
            if let Some(name) = extract_function_name(trimmed) {
                functions.push(DiscoveredFunction { name, is_test: true, is_benchmark: false, compile_only });
            }
            continue;
        }

        pending_attr = "";
    }
}

/// 从行文本中提取函数名。
pub fn extract_function_name(line: &str) -> Option<String> {
    let mut line = line.trim();
    if let Some(idx) = line.find(" micro ") {
        line = &line[idx + 7..];
    }
    else if let Some(rest) = line.strip_prefix("micro ") {
        line = rest;
    }
    else if let Some(rest) = line.strip_prefix("tests ") {
        line = rest;
    }
    else {
        return None;
    }

    let paren = line.find('(')?;
    if paren == 0 {
        return None;
    }
    let name = line[..paren].trim().trim_matches('`');
    if name.is_empty() { None } else { Some(name.to_string()) }
}

/// 扫描项目 `test/` 目录；单脚本工程则扫描项目根 `.v`。
pub fn discover_project_tests(project_dir: &Path) -> Vec<DiscoveredFunction> {
    let mut files = Vec::new();
    let test_dir = project_dir.join("test");
    if test_dir.exists() {
        let _ = collect_all_test_v_files(&test_dir, &mut files);
    }
    else if !project_dir.join("source").exists() {
        let _ = collect_root_script_v_files(project_dir, &mut files);
    }
    discover_test_functions(&files)
}

fn collect_root_script_v_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "v") {
            files.push(path);
        }
    }
    Ok(())
}

fn collect_all_test_v_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_all_test_v_files(&path, files)?;
            continue;
        }
        if path.extension().is_some_and(|ext| ext == "v") {
            files.push(path);
        }
    }
    Ok(())
}
#[cfg(test)]
#[path = "../../../tests/path_wired/cmds_test_engine_discover.rs"]
mod cmds_test_engine_discover_tests;
