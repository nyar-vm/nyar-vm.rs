//! Canonical Semantic MIR 前端一致性守卫：禁止按名字重建前端语义。

use std::{fs, path::PathBuf};

/// 收集 `emitter/pipeline/canonical/` 子树（门面文件 + 全部子模块）的生产代码。
///
/// canonical lowering 拆为子模块后，守卫必须扫描整棵子树；SMIR-G002 针对
/// descriptor 语义回退面。
fn canonical_semantic_sources() -> Vec<(String, String)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/compiler/emitter/pipeline/canonical");
    let mut paths = Vec::new();
    collect_rust_files(&root, &mut paths);
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            let relative = path.strip_prefix(&root).expect("path must live under the canonical root").display().to_string().replace('\\', "/");
            (format!("canonical/{relative}"), production_source(source))
        })
        .collect()
}

/// 收集 `mir/validation/` 子树（门面文件 + 全部子模块）的生产代码。
///
/// 校验器重构后拆为子模块，门面 `validation.rs` 只声明 `mod` 与 `pub use`，
/// 因此守卫必须扫描整棵子树，而不是单个文件的前缀。
fn semantic_validator_sources() -> Vec<(String, String)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/compiler/mir");
    let mut paths = vec![root.join("validation.rs")];
    collect_rust_files(&root.join("validation"), &mut paths);
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            let relative = path.strip_prefix(&root).expect("path must live under the mir root").display().to_string().replace('\\', "/");
            (format!("mir/{relative}"), production_source(source))
        })
        .collect()
}

fn collect_rust_files(directory: &PathBuf, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(directory).unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()));
    for entry in entries {
        let path = entry.expect("directory entry must be readable").path();
        if path.is_dir() {
            collect_rust_files(&path, out);
        }
        else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

fn production_source(source: String) -> String {
    source.split("#[cfg(test)]").next().expect("source prefix must exist").to_string()
}

#[test]
fn formal_semantic_path_forbids_semantic_reconstruction_patterns() {
    let mut sources = Vec::new();
    for (path, source) in canonical_semantic_sources() {
        sources.push(("SMIR-G002", format!("descriptor semantic fallback in `{path}`"), source));
    }
    for (path, source) in semantic_validator_sources() {
        sources.push(("SMIR-G001", format!("name-based semantic dispatch in `{path}`"), source));
    }
    let forbidden = [
        ("SMIR-G001", ".symbol.contains("),
        ("SMIR-G001", ".symbol.starts_with("),
        ("SMIR-G001", ".symbol.ends_with("),
        ("SMIR-G001", "\"von\""),
        ("SMIR-G001", "\"Option\""),
        ("SMIR-G001", "\"Result\""),
        ("SMIR-G002", "descriptor"),
        ("SMIR-G003", "infer_output"),
        ("SMIR-G004", "operand_stack"),
        ("SMIR-G004", "stack_shape"),
        ("SMIR-G005", "java.lang.String"),
        ("SMIR-G005", "System.String"),
        ("SMIR-G005", "JS string"),
        ("SMIR-G005", "WASM_GC_ANYREF"),
        ("SMIR-G006", "unwrap_or(NyarType::Integer32"),
        ("SMIR-G006", "unwrap_or(ValkyrieType::Integer32"),
        ("SMIR-G006", "default_i32"),
        ("SMIR-G006", "default_void"),
        ("SMIR-G006", "default_object"),
        ("SMIR-G006", "default_anyref"),
    ];
    for (rule, description, source) in sources {
        for (forbidden_rule, pattern) in forbidden {
            if rule == forbidden_rule || matches!(forbidden_rule, "SMIR-G003" | "SMIR-G004" | "SMIR-G006") {
                assert!(
                    !source.contains(pattern),
                    "{rule}: {description} must not contain forbidden semantic reconstruction pattern `{pattern}`"
                );
            }
        }
    }
}
