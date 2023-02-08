//! S-W1 / ADR 0013: semantic-path string dispatch must not grow.
//!
//! Full elimination is S-W2–S-W6. This gate freezes a **baseline count** so new
//! `as_str() == "` / `.ends_with("` semantic lookups cannot land unnoticed.
//! Lower the baselines only when offenders are deleted (never raise them).

use std::{
    fs,
    path::{Path, PathBuf},
};

/// Crates whose `src/` must shrink string identity dispatch (ADR 0013).
const SEMANTIC_SRC_ROOTS: &[&str] = &[
    "../nyar-language/src",
    "../nyar-emitter/src",
    "../../runtimes/nyar-vm/src",
];

/// Frozen ceilings for `as_str() == "` occurrences (production `src/` only).
/// Re-count after deleting offenders and lower these constants.
const AS_STR_EQ_BASELINE: usize = 220;

/// Frozen ceilings for `.ends_with("` used as symbol/path heuristics.
const ENDS_WITH_BASELINE: usize = 80;

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
         Delete string semantic dispatch (ADR 0013); top offenders: {:?}",
        hits.iter().take(12).map(|(p, c)| (p.display().to_string(), *c)).collect::<Vec<_>>()
    );
}

#[test]
fn ends_with_symbol_heuristics_do_not_grow() {
    let (total, hits) = scan_semantic_src(".ends_with(\"");
    assert!(
        total <= ENDS_WITH_BASELINE,
        ".ends_with(\" count grew: {total} > baseline {ENDS_WITH_BASELINE}. \
         Delete suffix fallbacks (ADR 0013); top offenders: {:?}",
        hits.iter().take(12).map(|(p, c)| (p.display().to_string(), *c)).collect::<Vec<_>>()
    );
}

#[test]
fn frozen_identity_types_are_public() {
    use nyar_types::{
        AttributeKind, ImportCapability, ImportIndex, IntrinsicId, ItemId, OperatorFixity, OperatorId, OperatorRegistration,
        TypeInstanceId,
    };
    assert!(ItemId::from_index(0).is_some());
    assert!(TypeInstanceId::from_index(0).is_some());
    assert!(ImportIndex::from_index(0).is_some());
    assert!(OperatorId::from_index(0).is_some());
    assert_eq!(OperatorFixity::Infix, OperatorFixity::Infix);
    assert_eq!(IntrinsicId::ArrayLen.diagnostic_path(), "builtin.array.length");
    assert_eq!(AttributeKind::Main.diagnostic_name(), "main");
    assert_eq!(ImportCapability::new("wasi_snapshot_preview1", "fd_write").to_string(), "wasi_snapshot_preview1::fd_write");
}
