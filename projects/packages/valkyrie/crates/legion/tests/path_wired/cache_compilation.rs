//! Migrated from `src/cache/compilation.rs` (`tests`).

use super::*;

use tempfile::tempdir;

#[test]
fn ir_round_trip() {
    let dir = tempdir().unwrap();
    let cache = CompilationCache::open(dir.path().join(".cache"));
    let entry = IrCacheEntry { ir_kind: "artifact-set".into(), ir_data: vec![1, 2, 3], ir_hash: "h1".into(), canonical_triple: "clr".into() };
    cache.put_ir("mod", "clr", "h1", &entry).unwrap();
    let got = cache.try_get_ir("mod", "clr", "h1").expect("hit");
    assert_eq!(got.ir_data, vec![1, 2, 3]);
    assert_eq!(got.ir_kind, "artifact-set");
}

#[test]
fn entry_slice_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let cache = CompilationCache::open(dir.path().join(".cache"));
    let slice = EntrySliceCacheEntry {
        entry_name: "main".into(),
        reachable_functions: vec!["main".into(), "helper".into()],
        call_graph_hash: "cg".into(),
        canonical_triple: "clr".into(),
    };
    cache.put_entry_slice("main", "clr", "cg", &slice).unwrap();
    let got = cache.try_get_entry_slice("main", "clr", "cg").unwrap();
    assert_eq!(got.reachable_functions, vec!["main", "helper"]);
}
