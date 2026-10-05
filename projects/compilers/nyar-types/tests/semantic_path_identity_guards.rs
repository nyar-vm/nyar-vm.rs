//! 语义路径上的字符串分派不得继续增长。
//!
//! 本门禁冻结一份**基线计数**：新增的 `as_str() == "` / `.ends_with("`
//! 语义查找不得悄然合入。仅在删除违规用法后下调基线（严禁上调）。

use std::{
    fs,
    path::{Path, PathBuf},
};

/// 必须收缩字符串身份分派的 crate 的 `src/` 根（相对本测试文件）。
const SEMANTIC_SRC_ROOTS: &[&str] = &["../nyar-language/src", "../nyar-emitter/src", "../../runtimes/nyar-vm/src"];

/// `as_str() == "` 出现次数的冻结上限（仅生产 `src/`）。
/// 删除违规用法后重新计数并下调这些常量（严禁上调）。
const AS_STR_EQ_BASELINE: usize = 91;

/// 用作符号/路径启发式的 `.ends_with("` 出现次数的冻结上限。
const ENDS_WITH_BASELINE: usize = 44;

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if !dir.is_dir() {
        return;
    }
    for entry in fs::read_dir(dir).unwrap_or_else(|error| panic!("failed to read {}: {error}", dir.display())) {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        }
        else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

fn count_needle(source: &str, needle: &str) -> usize {
    source.match_indices(needle).count()
}

fn production_body(source: &str) -> &str {
    source.split("#[cfg(test)]").next().unwrap_or(source)
}

fn scan_semantic_src(needle: &str) -> (usize, Vec<(PathBuf, usize)>) {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut total = 0;
    let mut hits = Vec::new();
    for relative in SEMANTIC_SRC_ROOTS {
        let root = manifest.join(relative);
        let mut files = Vec::new();
        collect_rs_files(&root, &mut files);
        for path in files {
            let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            let count = count_needle(production_body(&source), needle);
            if count > 0 {
                total += count;
                hits.push((path, count));
            }
        }
    }
    hits.sort_by(|a, b| b.1.cmp(&a.1));
    (total, hits)
}

#[test]
fn as_str_eq_dispatch_does_not_grow() {
    let (total, hits) = scan_semantic_src("as_str() == \"");
    assert!(
        total <= AS_STR_EQ_BASELINE,
        "as_str() == \" count grew: {total} > baseline {AS_STR_EQ_BASELINE}. \
         删除语义路径字符串分派后再合入；top offenders: {:?}",
        hits.iter().take(12).map(|(p, c)| (p.display().to_string(), *c)).collect::<Vec<_>>()
    );
}

#[test]
fn ends_with_symbol_heuristics_do_not_grow() {
    let (total, hits) = scan_semantic_src(".ends_with(\"");
    assert!(
        total <= ENDS_WITH_BASELINE,
        ".ends_with(\" count grew: {total} > baseline {ENDS_WITH_BASELINE}. \
         删除路径后缀启发式后再合入；top offenders: {:?}",
        hits.iter().take(12).map(|(p, c)| (p.display().to_string(), *c)).collect::<Vec<_>>()
    );
}

#[test]
fn frozen_identity_types_are_public() {
    use nyar_types::{
        AttributeId, AttributeRegistration, ImportCapability, ImportIndex, IntrinsicId, ItemId, OperatorFixity, OperatorId, TypeInstanceId,
        builtin_attribute,
    };
    assert!(ItemId::from_index(0).is_some());
    assert!(TypeInstanceId::from_index(0).is_some());
    assert!(ImportIndex::from_index(0).is_some());
    assert!(OperatorId::from_index(0).is_some());
    assert!(AttributeId::from_index(0).is_some());
    assert_eq!(OperatorFixity::Infix, OperatorFixity::Infix);
    assert_eq!(IntrinsicId::ArrayLen.diagnostic_path(), "builtin.array.length");
    assert_eq!(builtin_attribute::main().index(), 1);
    assert_eq!(AttributeRegistration { id: builtin_attribute::export(), name: "export".into() }.name, "export");
    assert_eq!(ImportCapability::new("wasi_snapshot_preview1", "fd_write").to_string(), "wasi_snapshot_preview1::fd_write");
}
