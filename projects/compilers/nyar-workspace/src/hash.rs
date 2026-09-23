//! Content hashing helpers aligned with the C# compilation cache.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::{Result, WorkspaceCacheError};

/// SHA256 of UTF-8 parts joined by `\0`, returned as lowercase hex.
pub fn combined_hash(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            hasher.update([0u8]);
        }
        hasher.update(part.as_bytes());
    }
    hex_encode(hasher.finalize().as_slice())
}

/// SHA256 of a file's raw bytes, lowercase hex.
pub fn file_hash(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).map_err(|source| WorkspaceCacheError::io(path, source))?;
    Ok(hex_encode(Sha256::digest(&bytes)))
}

/// Combined content hash of multiple files.
///
/// Paths are canonicalized when possible, sorted with case-insensitive ordering,
/// then each contributes `path\0fileHash\0` before a final SHA256.
pub fn files_hash(paths: &[PathBuf]) -> Result<String> {
    let mut ordered: Vec<(String, PathBuf)> = paths
        .iter()
        .map(|path| {
            let full = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            let key = full.to_string_lossy().to_ascii_lowercase();
            (key, full)
        })
        .collect();
    ordered.sort_by(|a, b| a.0.cmp(&b.0));

    let mut combined = String::new();
    for (_, path) in ordered {
        let path_text = path.to_string_lossy();
        combined.push_str(&path_text);
        combined.push('\0');
        combined.push_str(&file_hash(&path)?);
        combined.push('\0');
    }

    Ok(hex_encode(Sha256::digest(combined.as_bytes())))
}

fn hex_encode(bytes: impl AsRef<[u8]>) -> String {
    let bytes = bytes.as_ref();
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn combined_hash_is_stable_and_order_sensitive() {
        let a = combined_hash(&["a", "b"]);
        let b = combined_hash(&["a", "b"]);
        let c = combined_hash(&["b", "a"]);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn file_hash_same_content_same_hash() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.v");
        std::fs::write(&path, b"hello").unwrap();
        assert_eq!(file_hash(&path).unwrap(), file_hash(&path).unwrap());
    }

    #[test]
    fn files_hash_order_independent() {
        let dir = tempdir().unwrap();
        let p1 = dir.path().join("a.v");
        let p2 = dir.path().join("z.v");
        std::fs::write(&p1, b"one").unwrap();
        std::fs::write(&p2, b"two").unwrap();
        let forward = files_hash(&[p1.clone(), p2.clone()]).unwrap();
        let reverse = files_hash(&[p2, p1]).unwrap();
        assert_eq!(forward, reverse);
    }
}
