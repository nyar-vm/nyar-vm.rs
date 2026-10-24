//! Migrated from `src/cmds/doc/discover.rs` (`tests`).

use super::*;

#[test]
fn discovers_flat_zh_hans_docs() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = root.join("../../../../valkyrie.v/projects/legion._/projects/legion.tools");
    if fixture.join("documentation/pages/zh-hans/workflow.md").is_file() {
        let sections = discover_sections(&fixture);
        assert!(!sections.is_empty());
    }
}
