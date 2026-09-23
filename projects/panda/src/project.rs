//! Project discovery — parameter-driven compat from `pyproject.toml` (no lockfile sniffing).

use std::{
    fs,
    path::{Path, PathBuf},
};

use miette::{IntoDiagnostic, Result, miette};
use toml_edit::{DocumentMut, Item};

/// Which install *behavior* panda emulates for this package (from pyproject params).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PythonCompat {
    /// uv-shaped dependency workflow.
    Uv,
    /// Poetry-shaped workflow.
    Poetry,
    /// pip-shaped workflow.
    Pip,
}

impl PythonCompat {
    /// Log label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Uv => "uv",
            Self::Poetry => "poetry",
            Self::Pip => "pip",
        }
    }

    /// Parse `[tool.panda] manager = "…"`.
    pub fn from_field(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "uv" => Some(Self::Uv),
            "poetry" => Some(Self::Poetry),
            "pip" => Some(Self::Pip),
            _ => None,
        }
    }
}

/// A panda-managed Python project root.
#[derive(Debug, Clone)]
pub struct PandaProject {
    /// Absolute project directory.
    pub root: PathBuf,
    /// Compat profile from pyproject parameters.
    pub compat: PythonCompat,
    /// True when `pyproject.toml` exists.
    pub has_pyproject: bool,
}

impl PandaProject {
    /// Discover from `dir` upward.
    pub fn discover(dir: impl AsRef<Path>) -> Result<Self> {
        let start = fs::canonicalize(dir.as_ref()).into_diagnostic().unwrap_or_else(|_| dir.as_ref().to_path_buf());
        let mut cursor = start.clone();
        loop {
            if is_project_root(&cursor) {
                return Self::open(&cursor);
            }
            if !cursor.pop() {
                break;
            }
        }
        Err(miette!("未找到 Python 项目（需要 pyproject.toml / requirements.txt / setup.py；从 {} 向上搜索）", start.display()))
    }

    /// Open a known project root.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = fs::canonicalize(root.as_ref()).into_diagnostic().unwrap_or_else(|_| root.as_ref().to_path_buf());
        if !is_project_root(&root) {
            return Err(miette!("不是 Python 项目根目录: {}", root.display()));
        }
        let has_pyproject = root.join("pyproject.toml").is_file();
        let compat = resolve_compat(&root);
        Ok(Self { root, compat, has_pyproject })
    }
}

fn is_project_root(dir: &Path) -> bool {
    dir.join("pyproject.toml").is_file()
        || dir.join("requirements.txt").is_file()
        || dir.join("requirements-dev.txt").is_file()
        || dir.join("setup.py").is_file()
        || dir.join("Pipfile").is_file()
}

fn resolve_compat(root: &Path) -> PythonCompat {
    if let Ok(text) = fs::read_to_string(root.join("pyproject.toml")) {
        if let Ok(document) = text.parse::<DocumentMut>() {
            if let Some(m) = extract_panda_manager_field(&document) {
                return m;
            }
            if document.get("tool").and_then(|tool| tool.get("uv")).is_some() {
                return PythonCompat::Uv;
            }
            if document.get("tool").and_then(|tool| tool.get("poetry")).is_some() {
                return PythonCompat::Poetry;
            }
        }
    }
    PythonCompat::Pip
}

fn extract_panda_manager_field(document: &DocumentMut) -> Option<PythonCompat> {
    document
        .get("tool")
        .and_then(|tool| tool.get("panda"))
        .and_then(|panda| panda.get("manager"))
        .and_then(Item::as_str)
        .and_then(PythonCompat::from_field)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn panda_field_overrides() {
        let document = "[tool.poetry]\nname=\"x\"\n\n[tool.panda]\nmanager = \"uv\"\n".parse::<DocumentMut>().expect("toml");
        assert_eq!(extract_panda_manager_field(&document), Some(PythonCompat::Uv));
    }

    #[test]
    fn discovers_requirements_dev_only_tree() {
        let root = tempdir().expect("tempdir");
        fs::write(root.path().join("requirements-dev.txt"), "mypy>=1.0\n").expect("write");
        let project = PandaProject::open(root.path()).expect("open");
        assert_eq!(project.compat, PythonCompat::Pip);
        assert!(!project.has_pyproject);
    }

    #[test]
    fn pip_compat_for_plain_requirements() {
        let root = tempdir().expect("tempdir");
        fs::write(root.path().join("requirements.txt"), "httpx\n").expect("write");
        let project = PandaProject::open(root.path()).expect("open");
        assert_eq!(project.compat, PythonCompat::Pip);
    }
}
