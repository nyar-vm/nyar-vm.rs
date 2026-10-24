//! Generic `.nyar` bucket store under a cache root.

use std::{
    fs,
    path::{Path, PathBuf},
};

use odin_bytecode::{NYAR_VERSION, NyarModuleData, decode_module, encode_module};

use crate::{Result, WorkspaceCacheError, sanitize_bucket_name};

/// Content-addressed disk cache rooted at `{cache_root}/`.
#[derive(Debug, Clone)]
pub struct WorkspaceCache {
    /// Cache root directory (typically `{workspace}/.cache`).
    pub root: PathBuf,
}

impl WorkspaceCache {
    /// Open a cache at the given root (directory is created on first write).
    pub fn open(cache_root: impl AsRef<Path>) -> Self {
        Self { root: cache_root.as_ref().to_path_buf() }
    }

    /// Path for a single entry: `{root}/{safe_bucket}/{key_hash}.nyar`.
    pub fn entry_path(&self, bucket: &str, key_hash: &str) -> PathBuf {
        self.root.join(sanitize_bucket_name(bucket)).join(format!("{key_hash}.nyar"))
    }

    /// Read a payload when the entry exists and its type tag matches.
    ///
    /// Decode failures and type mismatches return `Ok(None)` (cache miss).
    pub fn get(&self, bucket: &str, key_hash: &str, expected_type: &str) -> Result<Option<Vec<u8>>> {
        let path = self.entry_path(bucket, key_hash);
        if !path.is_file() {
            return Ok(None);
        }

        let raw = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(source) => return Err(WorkspaceCacheError::io(path, source)),
        };

        match decode_module(&raw) {
            Ok(module) if module.name == expected_type => Ok(Some(module.code_bytes)),
            Ok(_) | Err(_) => Ok(None),
        }
    }

    /// Write a typed payload into the bucket.
    pub fn put(&self, bucket: &str, key_hash: &str, type_tag: &str, payload: &[u8]) -> Result<()> {
        let path = self.entry_path(bucket, key_hash);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| WorkspaceCacheError::io(parent, source))?;
        }

        let module = NyarModuleData {
            version: NYAR_VERSION,
            name: type_tag.to_string(),
            constants: Vec::new(),
            functions: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
            witness_entries: Vec::new(),
            code_bytes: payload.to_vec(),
            globals: Vec::new(),
            init_function_indices: Vec::new(),
            layouts: Vec::new(),
        };
        let encoded = encode_module(&module);
        fs::write(&path, encoded).map_err(|source| WorkspaceCacheError::io(path, source))
    }

    /// Delete one bucket directory.
    pub fn invalidate_bucket(&self, bucket: &str) -> Result<()> {
        let dir = self.root.join(sanitize_bucket_name(bucket));
        if dir.is_dir() {
            fs::remove_dir_all(&dir).map_err(|source| WorkspaceCacheError::io(dir, source))?;
        }
        Ok(())
    }

    /// Delete the entire cache root.
    pub fn invalidate_all(&self) -> Result<()> {
        if self.root.exists() {
            fs::remove_dir_all(&self.root).map_err(|source| WorkspaceCacheError::io(&self.root, source))?;
        }
        Ok(())
    }
}
