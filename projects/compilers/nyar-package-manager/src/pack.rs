use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use flate2::{Compression, write::GzEncoder};
use nyar_package_registry::sha256_hex;
use serde::Deserialize;
use std_data::text::von::from_str;
use tar::Builder;

use crate::Result;

/// Packed package artifact.
#[derive(Debug, Clone)]
pub struct PackResult {
    pub tarball_data: Vec<u8>,
    pub sha256: String,
    pub size: usize,
    pub file_count: usize,
}

/// Metadata written into registry manifests (`package.json` / `jsr.json`).
#[derive(Debug, Clone)]
pub struct PackMeta {
    pub name: String,
    pub version: String,
    pub description: String,
    pub license: Option<String>,
}

/// Options for packing a built registry artifact directory.
#[derive(Debug, Clone)]
pub struct RegistryPackOptions {
    pub artifact_dir: PathBuf,
    pub package_root: PathBuf,
    pub registry: String,
    pub meta: PackMeta,
    /// Glob patterns from the package manifest `files` field (merged with run-contract artifacts).
    pub include_files: Vec<String>,
    /// When true, tarball entries omit the registry `package/` prefix (flat layout).
    pub flat_layout: bool,
    /// Product-supplied on-disk layout (manifest / ignore filenames, entry aliases).
    pub layout: crate::ProjectLayout,
}

/// One publishable CLI entry derived from run contracts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryBinEntry {
    pub bin_name: String,
    pub relative_path: String,
}

/// Simple pack-ignore matcher (exact name / suffix / prefix `*`).
#[derive(Debug, Clone, Default)]
pub struct PackageIgnore {
    patterns: Vec<String>,
}

impl PackageIgnore {
    pub fn load(package_path: &Path, layout: crate::ProjectLayout) -> Self {
        let path = layout.ignore_file_path(package_path);
        let mut patterns = default_ignore_patterns(layout.lockfile);
        if let Ok(content) = std::fs::read_to_string(path) {
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                patterns.push(line.to_string());
            }
        }
        Self { patterns }
    }

    pub fn is_ignored(&self, relative: &str) -> bool {
        let relative = relative.replace('\\', "/");
        self.patterns.iter().any(|pattern| match_pattern(pattern, &relative))
    }
}

fn default_ignore_patterns(lockfile: &str) -> Vec<String> {
    vec![
        "vendors/".to_string(),
        ".cache/".to_string(),
        "node_modules/".to_string(),
        lockfile.to_string(),
        ".git/".to_string(),
        "target/".to_string(),
    ]
}

fn match_pattern(pattern: &str, relative: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('/') {
        return relative == prefix || relative.starts_with(&format!("{prefix}/"));
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        return relative.ends_with(suffix);
    }
    relative == pattern || relative.ends_with(&format!("/{pattern}"))
}

/// Pack a built `dist/{canonical}` directory for npm/jsr publish.
pub fn pack_registry_artifact(options: &RegistryPackOptions) -> Result<PackResult> {
    let artifact_files = list_artifact_files(&options.artifact_dir)?;
    let contracts = load_run_contracts(&options.artifact_dir)?;
    let bin_entries = registry_bin_entries(&contracts, &options.artifact_dir, &artifact_files, options.layout)?;

    let mut entries: BTreeMap<String, PathBuf> = BTreeMap::new();
    for entry in &bin_entries {
        entries.insert(entry.relative_path.clone(), options.artifact_dir.join(&entry.relative_path));
        if let Some(wasm_path) = paired_wasm_path(&entry.relative_path) {
            let wasm_file = options.artifact_dir.join(&wasm_path);
            if wasm_file.is_file() {
                entries.insert(wasm_path, wasm_file);
            }
        }
    }

    for optional in ["readme.md", "README.md", "LICENSE", "license"] {
        let path = options.package_root.join(optional);
        if path.is_file() {
            entries.insert(optional.to_string(), path);
        }
    }

    for (relative, source) in
        collect_manifest_include_paths(&options.include_files, &options.package_root, &options.artifact_dir, options.layout)?
    {
        entries.entry(relative).or_insert(source);
    }

    let mut buffer = Vec::new();
    let mut file_count = 0usize;
    {
        let encoder = GzEncoder::new(&mut buffer, Compression::default());
        let mut archive = Builder::new(encoder);

        for (relative, source) in &entries {
            if !source.is_file() {
                continue;
            }
            append_tar_file(&mut archive, source, relative, options.flat_layout)?;
            file_count += 1;
        }

        let package_json = build_package_json(&options.meta, &bin_entries, &options.registry);
        append_tar_bytes(&mut archive, "package.json", package_json.as_bytes(), options.flat_layout)?;
        file_count += 1;

        if options.registry.eq_ignore_ascii_case("jsr") {
            let jsr_json = build_jsr_json(&options.meta, &bin_entries);
            append_tar_bytes(&mut archive, "jsr.json", jsr_json.as_bytes(), options.flat_layout)?;
            file_count += 1;
        }

        archive.finish()?;
    }

    let sha256 = sha256_hex(&buffer);
    Ok(PackResult { size: buffer.len(), file_count, tarball_data: buffer, sha256 })
}

/// Pack a package directory into gzipped tar (`package/` prefix).
pub fn pack(package_directory: &Path, meta: &PackMeta, layout: crate::ProjectLayout) -> Result<PackResult> {
    let ignore = PackageIgnore::load(package_directory, layout);
    let files = collect_files(package_directory, &ignore)?;
    let mut buffer = Vec::new();
    {
        let encoder = GzEncoder::new(&mut buffer, Compression::default());
        let mut archive = Builder::new(encoder);
        for file in &files {
            let relative = file.strip_prefix(package_directory).unwrap_or(file.as_path());
            let relative = relative.to_string_lossy().replace('\\', "/");
            if relative == "package.json" {
                continue;
            }
            let archive_path = format!("package/{relative}");
            let mut file_handle = File::open(file)?;
            let mut data = Vec::new();
            file_handle.read_to_end(&mut data)?;
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            archive.append_data(&mut header, archive_path, data.as_slice())?;
        }

        let package_json = serde_json::json!({
            "name": meta.name,
            "version": meta.version,
            "description": meta.description,
            "license": meta.license.clone().unwrap_or_else(|| "UNLICENSED".to_string()),
            "main": "package.nyar",
            "files": ["**/*"],
        })
        .to_string();
        append_tar_bytes(&mut archive, "package.json", package_json.as_bytes(), false)?;
        archive.finish()?;
    }
    let sha256 = sha256_hex(&buffer);
    Ok(PackResult { size: buffer.len(), file_count: files.len().saturating_add(1), tarball_data: buffer, sha256 })
}

fn append_tar_file<W: Write>(archive: &mut Builder<W>, source: &Path, relative: &str, flat_layout: bool) -> Result<()> {
    let mut file_handle = File::open(source)?;
    let mut data = Vec::new();
    file_handle.read_to_end(&mut data)?;
    append_tar_bytes(archive, relative, &data, flat_layout)
}

fn append_tar_bytes<W: Write>(archive: &mut Builder<W>, relative: &str, data: &[u8], flat_layout: bool) -> Result<()> {
    let archive_path = if flat_layout { relative.to_string() } else { format!("package/{relative}") };
    let mut header = tar::Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    archive.append_data(&mut header, archive_path, data)?;
    Ok(())
}

fn collect_manifest_include_paths(
    patterns: &[String],
    package_root: &Path,
    artifact_dir: &Path,
    layout: crate::ProjectLayout,
) -> Result<BTreeMap<String, PathBuf>> {
    if patterns.is_empty() {
        return Ok(BTreeMap::new());
    }

    let ignore = PackageIgnore::load(package_root, layout);
    let mut candidates: Vec<(String, PathBuf)> = Vec::new();
    for file in collect_files(package_root, &ignore)? {
        let relative = file.strip_prefix(package_root).unwrap_or(&file).to_string_lossy().replace('\\', "/");
        candidates.push((relative, file));
    }
    for file in list_artifact_files(artifact_dir)? {
        let artifact_relative = file.strip_prefix(artifact_dir).unwrap_or(&file).to_string_lossy().replace('\\', "/");
        if !candidates.iter().any(|(_, path)| path == &file) {
            candidates.push((artifact_relative.clone(), file.clone()));
        }
        if let Ok(root_relative) = file.strip_prefix(package_root) {
            let root_relative = root_relative.to_string_lossy().replace('\\', "/");
            if !candidates.iter().any(|(relative, path)| path == &file && relative == &root_relative) {
                candidates.push((root_relative, file));
            }
        }
    }

    let mut packed = BTreeMap::new();
    for pattern in patterns {
        let pattern = pattern.trim().replace('\\', "/");
        if pattern.is_empty() {
            continue;
        }
        for (relative, source) in &candidates {
            if !glob_match(&pattern, relative) {
                continue;
            }
            let archive_relative = if source.starts_with(artifact_dir) {
                source.strip_prefix(artifact_dir).unwrap_or(source).to_string_lossy().replace('\\', "/")
            }
            else {
                relative.clone()
            };
            packed.entry(archive_relative).or_insert_with(|| source.clone());
        }
    }
    Ok(packed)
}

fn glob_match(pattern: &str, path: &str) -> bool {
    let pattern_parts: Vec<&str> = pattern.split('/').collect();
    let path_parts: Vec<&str> = path.split('/').collect();
    glob_match_parts(&pattern_parts, &path_parts)
}

fn glob_match_parts(pattern: &[&str], path: &[&str]) -> bool {
    match (pattern.first().copied(), path.first().copied()) {
        (None, None) => true,
        (None, Some(_)) => false,
        (Some("**"), _) => {
            if pattern.len() == 1 {
                return true;
            }
            let rest = &pattern[1..];
            for index in 0..=path.len() {
                if glob_match_parts(rest, &path[index..]) {
                    return true;
                }
            }
            false
        }
        (Some(segment), Some(value)) => {
            if glob_segment_match(segment, value) {
                glob_match_parts(&pattern[1..], &path[1..])
            }
            else {
                false
            }
        }
        (Some(_), None) => false,
    }
}

fn glob_segment_match(pattern: &str, segment: &str) -> bool {
    if pattern == "*" {
        return !segment.is_empty();
    }
    if !pattern.contains('*') {
        return pattern == segment;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut remainder = segment;
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if index == 0 {
            if !remainder.starts_with(part) {
                return false;
            }
            remainder = &remainder[part.len()..];
        }
        else if index == parts.len() - 1 {
            if !remainder.ends_with(part) {
                return false;
            }
        }
        else if let Some(found) = remainder.find(part) {
            remainder = &remainder[found + part.len()..];
        }
        else {
            return false;
        }
    }
    true
}

fn build_package_json(meta: &PackMeta, bins: &[RegistryBinEntry], registry: &str) -> String {
    let mut bin_map = BTreeMap::new();
    let mut exports = BTreeMap::new();
    for entry in bins {
        let path = format!("./{}", entry.relative_path);
        bin_map.insert(entry.bin_name.clone(), path.clone());
        exports.insert(format!("./{}", entry.bin_name), path.clone());
    }
    if let Some(first) = bins.first() {
        exports.insert(".".to_string(), format!("./{}", first.relative_path));
    }

    let mut doc = serde_json::json!({
        "name": meta.name,
        "version": meta.version,
        "description": meta.description,
        "license": meta.license.clone().unwrap_or_else(|| "UNLICENSED".to_string()),
        "type": "module",
        "bin": bin_map,
        "exports": exports,
    });

    if registry.eq_ignore_ascii_case("npm") {
        doc["engines"] = serde_json::json!({ "node": ">=18" });
    }

    serde_json::to_string_pretty(&doc).unwrap_or_else(|_| doc.to_string())
}

fn build_jsr_json(meta: &PackMeta, bins: &[RegistryBinEntry]) -> String {
    let mut exports = BTreeMap::new();
    for entry in bins {
        exports.insert(format!("./{}", entry.bin_name), format!("./{}", entry.relative_path));
    }
    let doc = serde_json::json!({
        "name": meta.name,
        "version": meta.version,
        "exports": exports,
    });
    serde_json::to_string_pretty(&doc).unwrap_or_else(|_| doc.to_string())
}

fn registry_bin_entries(
    contracts: &[RunContract],
    artifact_dir: &Path,
    files: &[PathBuf],
    layout: crate::ProjectLayout,
) -> Result<Vec<RegistryBinEntry>> {
    if contracts.is_empty() {
        return Ok(fallback_bin_entries_from_mjs(files, artifact_dir));
    }

    let mut entries = Vec::new();
    for contract in contracts {
        let Some(path) = resolve_contract_artifact(artifact_dir, files, &contract.physical_entry, layout)
        else {
            continue;
        };
        let relative = path.strip_prefix(artifact_dir).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        if !is_publishable_artifact(&relative) {
            continue;
        }
        entries.push(RegistryBinEntry { bin_name: bin_name_from_logical(&contract.logical_entry), relative_path: relative });
    }
    if entries.is_empty() {
        return Ok(fallback_bin_entries_from_mjs(files, artifact_dir));
    }
    Ok(entries)
}

fn fallback_bin_entries_from_mjs(files: &[PathBuf], artifact_dir: &Path) -> Vec<RegistryBinEntry> {
    files
        .iter()
        .filter(|path| path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("mjs")))
        .filter_map(|path| {
            let relative = path.strip_prefix(artifact_dir).ok()?.to_string_lossy().replace('\\', "/");
            let stem = path.file_stem()?.to_str()?;
            let bin_name = stem.rsplit('_').next().unwrap_or(stem).to_string();
            Some(RegistryBinEntry { bin_name, relative_path: relative })
        })
        .collect()
}

fn bin_name_from_logical(logical_entry: &str) -> String {
    let normalized = logical_entry.replace('\u{2237}', "::");
    normalized.rsplit("::").next().unwrap_or(&normalized).trim().to_string()
}

fn paired_wasm_path(mjs_relative: &str) -> Option<String> {
    let path = Path::new(mjs_relative);
    let stem = path.file_stem()?.to_str()?;
    let parent = path.parent().map(|value| value.to_string_lossy().replace('\\', "/")).filter(|value| !value.is_empty());
    let wasm_name = format!("{stem}.wasm");
    Some(match parent {
        Some(prefix) => format!("{prefix}/{wasm_name}"),
        None => wasm_name,
    })
}

fn is_publishable_artifact(relative: &str) -> bool {
    let name = Path::new(relative).file_name().and_then(|value| value.to_str()).unwrap_or(relative);
    if name.starts_with("run-contract") {
        return false;
    }
    if name.starts_with("wasmtime-") {
        return false;
    }
    let lower = relative.to_ascii_lowercase();
    !(lower.ends_with(".msil") || lower.contains("/wasmtime-"))
}

fn list_artifact_files(artifact_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_files_flat(artifact_dir, artifact_dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_files_flat(root: &Path, current: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|value| value.to_str()).unwrap_or_default();
            if name.starts_with("wasmtime") {
                continue;
            }
            collect_files_flat(root, &path, files)?;
        }
        else if path.is_file() {
            let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            if is_publishable_artifact(&relative) {
                files.push(path);
            }
        }
    }
    Ok(())
}

fn resolve_contract_artifact(artifact_dir: &Path, files: &[PathBuf], physical_entry: &str, layout: crate::ProjectLayout) -> Option<PathBuf> {
    if physical_entry.is_empty() {
        return None;
    }
    for candidate in (layout.entry_aliases)(physical_entry).iter().chain(std::iter::once(&physical_entry)) {
        let direct = artifact_dir.join(candidate);
        if direct.is_file() {
            return Some(direct);
        }
        if let Some(found) = find_contract_artifact(files, candidate) {
            return Some(found);
        }
    }
    let normalized = normalize_entry_name(physical_entry);
    find_contract_artifact(files, &normalized)
}

fn find_contract_artifact(files: &[PathBuf], physical_entry: &str) -> Option<PathBuf> {
    files.iter().find_map(|path| {
        let file_name = path.file_name()?.to_str()?;
        let stem = path.file_stem()?.to_str()?;
        if file_name.eq_ignore_ascii_case(physical_entry) || stem.eq_ignore_ascii_case(physical_entry) { Some(path.clone()) } else { None }
    })
}

fn normalize_entry_name(entry: &str) -> String {
    entry.replace('\u{2237}', "_").replace("::", "_").replace('.', "_")
}

#[derive(Debug, Clone, Deserialize)]
struct ExecutionManifest {
    #[serde(default)]
    run_contracts: Vec<RunContract>,
}

#[derive(Debug, Clone, Deserialize)]
struct RunContract {
    logical_entry: String,
    physical_entry: String,
}

fn load_run_contracts(artifact_dir: &Path) -> Result<Vec<RunContract>> {
    let contracts_path = artifact_dir.join("run-contracts.txt");
    if contracts_path.is_file() {
        let source = std::fs::read_to_string(&contracts_path)?;
        let manifest: ExecutionManifest = from_str(&source)?;
        if !manifest.run_contracts.is_empty() {
            return Ok(manifest.run_contracts);
        }
    }

    let legacy_path = artifact_dir.join("run-contract.txt");
    if legacy_path.is_file() {
        let source = std::fs::read_to_string(&legacy_path)?;
        let contract: RunContract = from_str(&source)?;
        return Ok(vec![contract]);
    }

    Ok(Vec::new())
}

fn collect_files(package_directory: &Path, ignore: &PackageIgnore) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    visit(package_directory, package_directory, ignore, &mut files)?;
    files.sort();
    Ok(files)
}

fn visit(root: &Path, current: &Path, ignore: &PackageIgnore, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        if ignore.is_ignored(&relative) {
            continue;
        }
        if path.is_dir() {
            visit(root, &path, ignore, files)?;
        }
        else if path.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

/// Unpack gzipped tar bytes into `target_directory`.
pub fn unpack(tarball_data: &[u8], target_directory: &Path) -> Result<usize> {
    nyar_package_registry::extract_tarball(tarball_data, target_directory)?;
    Ok(std::fs::read_dir(target_directory).map(|entries| entries.count()).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bin_name_from_logical_entry() {
        assert_eq!(bin_name_from_logical("demo::cli"), "cli");
        assert_eq!(bin_name_from_logical("demo::main"), "main");
    }

    #[test]
    fn resolves_unicode_scope_entry_to_mjs() {
        let dir = tempfile::tempdir().expect("temp");
        let artifact = dir.path().join("dist");
        std::fs::create_dir_all(&artifact).expect("mkdir");
        std::fs::write(artifact.join("demo_cli.mjs"), "export {}").expect("write mjs");
        std::fs::write(artifact.join("demo_cli.wasm"), b"\0asm").expect("write wasm");
        std::fs::write(
            artifact.join("run-contracts.txt"),
            "{\n    run_contracts: [\n        {\n            logical_entry: \"demo::cli\",\n            physical_entry: \"demo_cli.mjs\",\n            invocation: \"node\",\n            validate: \"node demo_cli.mjs\"\n        }\n    ]\n}\n",
        )
        .expect("write contract");

        let options = RegistryPackOptions {
            artifact_dir: artifact.clone(),
            package_root: dir.path().to_path_buf(),
            registry: "npm".to_string(),
            meta: PackMeta {
                name: "@scope/pkg".to_string(),
                version: "1.0.0".to_string(),
                description: "test".to_string(),
                license: Some("MIT".to_string()),
            },
            include_files: Vec::new(),
            flat_layout: false,
            layout: crate::ProjectLayout::neutral(),
        };
        let packed = pack_registry_artifact(&options).expect("pack");
        assert!(packed.file_count >= 3);

        let unpack_dir = dir.path().join("unpacked");
        std::fs::create_dir_all(&unpack_dir).expect("unpack dir");
        let count = crate::pack::unpack(&packed.tarball_data, &unpack_dir).expect("unpack");
        assert!(count >= 3);
        let package_json = std::fs::read_to_string(unpack_dir.join("package.json")).expect("package.json");
        assert!(package_json.contains("demo_cli.mjs"));
        assert!(package_json.contains("\"cli\""));
    }

    #[test]
    fn glob_match_supports_star_and_double_star() {
        assert!(glob_match("README.md", "README.md"));
        assert!(glob_match("docs/*.md", "docs/guide.md"));
        assert!(glob_match("dist/**/*.mjs", "dist/wasm32-node-unknown-wasm/demo.mjs"));
        assert!(!glob_match("docs/*.md", "docs/sub/guide.md"));
    }

    #[test]
    fn packs_manifest_files_and_jsr_flat_layout() {
        let dir = tempfile::tempdir().expect("temp");
        let artifact = dir.path().join("dist").join("wasm32-node-unknown-wasm");
        std::fs::create_dir_all(artifact.join("docs")).expect("mkdir");
        std::fs::write(artifact.join("demo_cli.mjs"), "export {}").expect("write mjs");
        std::fs::write(artifact.join("demo_cli.wasm"), b"\0asm").expect("write wasm");
        std::fs::write(artifact.join("docs").join("extra.md"), "# extra").expect("write extra");
        std::fs::write(dir.path().join("NOTICE"), "notice").expect("write notice");
        std::fs::write(
            artifact.join("run-contracts.txt"),
            "{\n    run_contracts: [\n        {\n            logical_entry: \"demo::cli\",\n            physical_entry: \"demo_cli.mjs\",\n            invocation: \"node\",\n            validate: \"node demo_cli.mjs\"\n        }\n    ]\n}\n",
        )
        .expect("write contract");

        let options = RegistryPackOptions {
            artifact_dir: artifact.clone(),
            package_root: dir.path().to_path_buf(),
            registry: "jsr".to_string(),
            meta: PackMeta {
                name: "@scope/pkg".to_string(),
                version: "1.0.0".to_string(),
                description: "test".to_string(),
                license: Some("MIT".to_string()),
            },
            include_files: vec!["NOTICE".into(), "dist/**/docs/*.md".into()],
            flat_layout: true,
            layout: crate::ProjectLayout::neutral(),
        };
        let packed = pack_registry_artifact(&options).expect("pack");

        let unpack_dir = dir.path().join("unpacked-jsr");
        std::fs::create_dir_all(&unpack_dir).expect("unpack dir");
        crate::pack::unpack(&packed.tarball_data, &unpack_dir).expect("unpack");
        assert!(unpack_dir.join("docs/extra.md").is_file());
        assert!(unpack_dir.join("NOTICE").is_file());
        assert!(unpack_dir.join("jsr.json").is_file());
        assert!(!unpack_dir.join("package").is_dir());
    }
}
