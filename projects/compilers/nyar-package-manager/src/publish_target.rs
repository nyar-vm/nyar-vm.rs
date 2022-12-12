use std::path::{Path, PathBuf};

use nyar_language::CanonicalTarget;

use crate::{PackageManagerError, Result};

/// Resolved publish artifact location under `dist/{canonical}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPublishArtifact {
    pub canonical_target: CanonicalTarget,
    pub canonical_label: String,
    pub artifact_dir: PathBuf,
}

/// Resolve `publish[].target` to a canonical triple and `dist/` subdirectory.
///
/// For npm/jsr, `wasm` is remapped to `node` because registry packages run on Node hosts.
pub fn resolve_publish_artifact(package_path: &Path, build_target: &str, registry: &str) -> Result<ResolvedPublishArtifact> {
    let mut target_input = build_target.trim();
    if target_input.is_empty() {
        target_input = "node";
    }

    let registry = registry.trim().to_ascii_lowercase();
    let mut remapped = false;
    if matches!(registry.as_str(), "npm" | "jsr") && target_input.eq_ignore_ascii_case("wasm") {
        println!("注意：npm/jsr 发布将 `wasm` 映射为 `node`（wasm32-node-unknown-wasm）");
        target_input = "node";
        remapped = true;
    }

    let canonical_target = CanonicalTarget::parse(target_input).map_err(|error| {
        PackageManagerError::message(format!("无法解析 publish target `{target_input}`：{error}{}", if remapped { "" } else { "" }))
    })?;
    let canonical_label = canonical_target.as_canonical_str();
    let artifact_dir = package_path.join("dist").join(&canonical_label);
    if !artifact_dir.is_dir() {
        return Err(PackageManagerError::message(format!(
            "构建产物目录不存在：{}\n请先构建目标：build --target {}",
            artifact_dir.display(),
            publish_build_hint(target_input, &canonical_label)
        )));
    }

    Ok(ResolvedPublishArtifact { canonical_target, canonical_label, artifact_dir })
}

fn publish_build_hint(requested: &str, canonical: &str) -> String {
    if requested.eq_ignore_ascii_case(canonical) { requested.to_string() } else { format!("{requested}（或 {canonical}）") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn wasm_maps_to_node_for_npm() {
        let dir = TempDir::new().expect("temp");
        let artifact = dir.path().join("dist").join("wasm32-node-unknown-wasm");
        std::fs::create_dir_all(&artifact).expect("mkdir");
        let resolved = resolve_publish_artifact(dir.path(), "wasm", "npm").expect("resolve");
        assert_eq!(resolved.canonical_label, "wasm32-node-unknown-wasm");
    }

    #[test]
    fn missing_dist_errors_with_hint() {
        let dir = TempDir::new().expect("temp");
        let error = resolve_publish_artifact(dir.path(), "node", "npm").expect_err("missing dist");
        assert!(error.to_string().contains("build --target"));
    }
}
