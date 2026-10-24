//! 宿主逻辑字节码校验：生产路径禁止占位魔数。

use miette::{Result, miette};

use crate::compile::HostArtifactKind;

/// 测试占位魔数 `ASGDHOST\x01`。
pub const ASGDHOST_MAGIC: &[u8] = b"ASGDHOST\x01";

/// 占位类内部名。
pub const PLACEHOLDER_CLASS_NAME: &str = "asgard/Placeholder";

/// 校验宿主编译产物非占位字节。
pub fn validate_host_logic_bytes(bytes: &[u8], platform: &str, backend: &str, target: &str, kind: HostArtifactKind) -> Result<()> {
    if bytes.is_empty() {
        return Err(miette!("宿主编译产物为空 (platform={platform}, backend={backend}, target={target})"));
    }
    if bytes.starts_with(ASGDHOST_MAGIC) {
        return Err(miette!("宿主编译产物为占位魔数 ASGDHOST (platform={platform}, backend={backend}, target={target})"));
    }
    if bytes.windows(PLACEHOLDER_CLASS_NAME.len()).any(|w| w == PLACEHOLDER_CLASS_NAME.as_bytes()) {
        return Err(miette!("宿主编译产物含占位类 {PLACEHOLDER_CLASS_NAME} (platform={platform}, backend={backend}, target={target})"));
    }
    match kind {
        HostArtifactKind::JvmClass => {
            if !bytes.starts_with(&[0xCA, 0xFE, 0xBA, 0xBE]) {
                return Err(miette!("JVM 产物非有效 .class（缺少 CAFEBABE 魔数）(platform={platform}, backend={backend}, target={target})"));
            }
        }
        HostArtifactKind::NativeExecutable => {}
        HostArtifactKind::Wasm => {}
    }
    Ok(())
}
#[cfg(test)]
#[path = "../tests/path_wired/host_validation.rs"]
mod host_validation_tests;
