//! Product-layer Python import path wiring for traditional layouts.
//!
//! Package manager installs land under `vendors/` (not `.venv` / `site-packages`).
//! Panda prepends local sources + vendor roots to `PYTHONPATH` for run/test/build.

use std::path::{Path, PathBuf};

/// Ordered import roots for a panda project.
///
/// Includes the project root, `src/` when present (editable src layout), and each
/// `vendors/{registry}/{name}@{version}` install directory when present.
pub fn python_import_paths(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    paths.push(root.to_path_buf());
    let src = root.join("src");
    if src.is_dir() {
        paths.push(src);
    }
    collect_vendor_package_roots(&root.join("vendors"), &mut paths);
    paths
}

/// Join import paths into a `PYTHONPATH` value for the current platform.
pub fn format_pythonpath(paths: &[PathBuf]) -> String {
    let sep = if cfg!(windows) { ';' } else { ':' };
    paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(&sep.to_string())
}

/// Env pairs for [`nyar_package_manager::ScriptRunner::run_with_env`].
///
/// Preserves any existing `PYTHONPATH` by appending after panda's roots.
pub fn python_runtime_env(root: &Path) -> Vec<(String, String)> {
    let mut paths = python_import_paths(root);
    if let Ok(existing) = std::env::var("PYTHONPATH") {
        for part in std::env::split_paths(&existing) {
            if !part.as_os_str().is_empty() && !paths.iter().any(|p| p == &part) {
                paths.push(part);
            }
        }
    }
    vec![("PYTHONPATH".into(), format_pythonpath(&paths))]
}

fn collect_vendor_package_roots(vendors: &Path, out: &mut Vec<PathBuf>) {
    if !vendors.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(vendors)
    else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        // Nested: vendors/{registry}/{name}@{version}
        let Ok(children) = std::fs::read_dir(&path)
        else {
            continue;
        };
        let mut nested = false;
        for child in children.flatten() {
            let child_path = child.path();
            if child_path.is_dir() {
                out.push(child_path);
                nested = true;
            }
        }
        // Flat: vendors/{name} or vendors/{name}@{version}
        if !nested {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn includes_src_and_vendor_roots() {
        let root = tempdir().expect("tempdir");
        fs::create_dir_all(root.path().join("src").join("demo")).expect("src");
        fs::create_dir_all(root.path().join("vendors").join("conda").join("rich@13.7.0")).expect("vendors");
        let paths = python_import_paths(root.path());
        assert!(paths.iter().any(|p| p == root.path()));
        assert!(paths.iter().any(|p| p.ends_with("src")));
        assert!(paths.iter().any(|p| p.ends_with("rich@13.7.0")));
        let joined = format_pythonpath(&paths);
        assert!(joined.contains("src"));
        assert!(joined.contains("rich@13.7.0"));
    }

    #[test]
    fn runtime_env_sets_pythonpath() {
        let root = tempdir().expect("tempdir");
        fs::create_dir_all(root.path().join("src")).expect("src");
        let env = python_runtime_env(root.path());
        assert_eq!(env.len(), 1);
        assert_eq!(env[0].0, "PYTHONPATH");
        assert!(env[0].1.contains("src"));
    }
}
