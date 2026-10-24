//! Artifact-set bundle serialization for `legion build` IR cache.

use std::{
    fs,
    path::{Path, PathBuf},
};

use nyar_emitter::{DriverCompileReport, DriverRunContract};
use nyar_workspace::{combined_hash, files_hash};
use rusty_valkyrie::ArtifactSet;
use serde::{Deserialize, Serialize};

use super::{CompilationCache, IrCacheEntry};

const ARTIFACT_KIND: &str = "artifact-set";

/// Alias used by build tests and call sites.
pub type BuildBundle = CachedBuildBundle;

/// Cached build outputs: report metadata + relative files under the output directory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CachedBuildBundle {
    /// Artifact descriptors from the driver report.
    pub artifacts: ArtifactSet,
    /// Optional entry symbol.
    pub entry_symbol: Option<String>,
    /// Run contracts.
    pub run_contracts: Vec<CachedRunContract>,
    /// Relative path → file bytes.
    pub files: Vec<(String, Vec<u8>)>,
}

/// Serializable run contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CachedRunContract {
    /// Logical entry name.
    pub logical_entry: String,
    /// Physical entry file.
    pub physical_entry: String,
    /// Invocation command.
    pub invocation: String,
    /// Validation command.
    pub validate: String,
}

impl From<&DriverRunContract> for CachedRunContract {
    fn from(value: &DriverRunContract) -> Self {
        Self {
            logical_entry: value.logical_entry.clone(),
            physical_entry: value.physical_entry.clone(),
            invocation: value.invocation.clone(),
            validate: value.validate.clone(),
        }
    }
}

impl From<&CachedRunContract> for DriverRunContract {
    fn from(value: &CachedRunContract) -> Self {
        Self {
            logical_entry: value.logical_entry.clone(),
            physical_entry: value.physical_entry.clone(),
            invocation: value.invocation.clone(),
            validate: value.validate.clone(),
        }
    }
}

/// Toolchain id embedded in artifact keys (invalidates cache on legion upgrades
/// and on identity / MIR / layout / bytecode contract version bumps).
pub fn toolchain_fingerprint() -> String {
    format!(
        "legion={};{};bytecode={}",
        env!("CARGO_PKG_VERSION"),
        nyar_types::contract_version_fingerprint(),
        odin_bytecode::BYTECODE_FORMAT_VERSION,
    )
}

/// Compute artifact cache hash from sources + target + build flags + toolchain.
pub fn compute_artifact_hash(
    source_files: &[PathBuf],
    manifest_files: &[PathBuf],
    canonical_triple: &str,
    wat: bool,
    runtime_async: bool,
) -> Result<String, String> {
    let sources = files_hash(source_files).map_err(|e| e.to_string())?;
    let manifests = files_hash(manifest_files).map_err(|e| e.to_string())?;
    let flags = format!("{}{}", if wat { '1' } else { '0' }, if runtime_async { '1' } else { '0' });
    let toolchain = toolchain_fingerprint();
    Ok(combined_hash(&[&sources, &manifests, canonical_triple, &flags, &toolchain]))
}

/// Collect output directory files into a cacheable bundle.
pub fn collect_build_bundle(output_dir: &Path, report: &DriverCompileReport) -> Result<CachedBuildBundle, String> {
    let mut paths = report.artifacts.artifacts.iter().map(|artifact| artifact.path.clone()).collect::<Vec<_>>();
    paths.extend(report.run_contracts.iter().map(|contract| contract.physical_entry.clone()));
    paths.sort();
    paths.dedup();
    let mut files = Vec::with_capacity(paths.len());
    for relative in paths {
        let path = Path::new(&relative);
        if relative.is_empty() || path.components().any(|component| !matches!(component, std::path::Component::Normal(_))) {
            return Err(format!("编译器报告包含非法产物路径：{relative}"));
        }
        let absolute = output_dir.join(path);
        if !absolute.is_file() {
            return Err(format!("编译器报告声明的产物不存在：{relative}"));
        }
        files.push((relative, fs::read(&absolute).map_err(|error| error.to_string())?));
    }
    Ok(CachedBuildBundle {
        artifacts: report.artifacts.clone(),
        entry_symbol: report.entry_symbol.clone(),
        run_contracts: report.run_contracts.iter().map(CachedRunContract::from).collect(),
        files,
    })
}

/// Persist a build bundle under the IR cache key.
pub fn store_cached_build(
    cache: &CompilationCache,
    module_name: &str,
    canonical_triple: &str,
    ir_hash: &str,
    payload: &CachedBuildBundle,
) -> Result<(), String> {
    let ir_data = serde_json::to_vec(payload).map_err(|e| e.to_string())?;
    cache.put_ir(
        module_name,
        canonical_triple,
        ir_hash,
        &IrCacheEntry { ir_kind: ARTIFACT_KIND.into(), ir_data, ir_hash: ir_hash.to_string(), canonical_triple: canonical_triple.to_string() },
    )
}
#[cfg(test)]
#[path = "../../tests/path_wired/cache_artifact.rs"]
mod cache_artifact_tests;
