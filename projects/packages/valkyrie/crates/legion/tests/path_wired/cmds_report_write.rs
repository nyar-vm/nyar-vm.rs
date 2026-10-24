//! Migrated from `src/cmds/report/write.rs` (`tests`).

use super::*;

use tempfile::tempdir;

#[test]
fn atomic_write_creates_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("nested").join("index.html");
    atomic_write_all_text(&path, "<html></html>").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "<html></html>");
}
