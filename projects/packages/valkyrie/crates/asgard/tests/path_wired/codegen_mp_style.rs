//! Migrated from `src/codegen/mp_style.rs` (`tests`).

use super::*;

#[test]
fn converts_px_to_rpx() {
    let css = ".box { padding: 12px; margin: 8.5px; }";
    let wxss = px_to_rpx(css);
    assert!(wxss.contains("24rpx"));
    assert!(wxss.contains("17rpx"));
}
