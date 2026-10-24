//! Migrated from `src/host_validation.rs` (`tests`).

use super::*;

#[test]
fn rejects_asgdhost_magic() {
    let mut bytes = ASGDHOST_MAGIC.to_vec();
    bytes.push(b'a');
    assert!(validate_host_logic_bytes(&bytes, "android", "AndroidCompose", "jvm", HostArtifactKind::JvmClass).is_err());
}

#[test]
fn accepts_valid_class_magic() {
    let bytes = vec![0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 0];
    assert!(validate_host_logic_bytes(&bytes, "android", "AndroidCompose", "jvm", HostArtifactKind::JvmClass).is_ok());
}
