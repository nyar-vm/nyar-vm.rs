//! Migrated from `src/oak.rs` (`tests`).

use legion_workspace::oak::*;
use oak_core::{OakError, OakErrorKind};
use std::{fs, path::Path};

#[test]
fn source_point_span_preserves_oak_offset() {
    let error = OakError::new(OakErrorKind::SyntaxError { message: "expected field".to_string(), offset: 7, source_id: None });
    assert_eq!(source_point_span(&error), Some(7..8));
}
