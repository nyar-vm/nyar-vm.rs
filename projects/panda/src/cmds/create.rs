//! `panda create` — scaffold a traditional pip / PEP 621 tree (`src/` layout).

use std::{fs, path::PathBuf, process::ExitCode};

use clap::Args;
use miette::{IntoDiagnostic, Result, miette};

/// `panda create` arguments.
#[derive(Debug, Clone, Args)]
pub struct CreateArgs {
    /// New package directory / import name (snake_case recommended).
    pub name: String,
    /// Parent directory (default: `.`).
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
}

/// Run `panda create`.
pub fn run(args: &CreateArgs) -> Result<ExitCode> {
    let root = args.path.join(&args.name);
    if root.exists() {
        return Err(miette!("目标已存在: {}", root.display()));
    }
    let pkg = args.name.replace('-', "_");
    fs::create_dir_all(root.join("src").join(&pkg)).into_diagnostic()?;
    fs::create_dir_all(root.join("tests")).into_diagnostic()?;
    fs::create_dir_all(root.join("scripts")).into_diagnostic()?;

    let pyproject = format!(
        r#"[project]
name = "{name}"
version = "0.1.0"
description = "Created by panda create"
requires-python = ">=3.10"
dependencies = []

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[tool.hatch.build.targets.wheel]
packages = ["src/{pkg}"]

[tool.panda]
manager = "pip"
dev-dependencies = []
"#,
        name = args.name,
        pkg = pkg
    );
    fs::write(root.join("pyproject.toml"), pyproject).into_diagnostic()?;
    fs::write(
        root.join("src").join(&pkg).join("__init__.py"),
        format!("\"\"\"{pkg} package.\"\"\"\n\n__version__ = \"0.1.0\"\n\n\ndef hello() -> str:\n    return \"hello from panda\"\n"),
    )
    .into_diagnostic()?;
    // stdlib unittest (matches `panda test`); not pytest.
    fs::write(
        root.join("tests").join("test_hello.py"),
        format!(
            "import unittest\n\nfrom {pkg} import hello\n\n\nclass HelloTests(unittest.TestCase):\n    def test_hello(self) -> None:\n        self.assertEqual(hello(), \"hello from panda\")\n\n\nif __name__ == \"__main__\":\n    unittest.main()\n"
        ),
    )
    .into_diagnostic()?;
    fs::write(
        root.join("scripts").join("build.py"),
        "# Project build entry for `panda build` (ScriptRunner; no hatch/uv/poetry shell).\nprint(\"panda build: ok\")\n",
    )
    .into_diagnostic()?;
    fs::write(
        root.join(".gitignore"),
        "\
# panda / package-manager
vendors/
panda-lock.von
.panda/

# local Python envs (user-owned; panda does not materialize .venv)
.venv/
venv/
__pycache__/
*.py[cod]
*.egg-info/
dist/
build/
",
    )
    .into_diagnostic()?;
    fs::write(
        root.join("readme.md"),
        format!(
            "# {name}\n\nCreated by `panda create` (PEP 621 + `src/` layout).\n\n\
Dependencies install under `vendors/` (not `.venv`). `panda run` / `test` / `build` set `PYTHONPATH` \
to include `src/` and vendor roots.\n\n```bash\npanda install\npanda check\npanda test\npanda build\n```\n",
            name = args.name
        ),
    )
    .into_diagnostic()?;

    println!("created {}", root.display());
    println!("layout: src/{pkg}/  tests/  scripts/  pyproject.toml");
    println!("next: cd {} && panda install && panda check && panda test", args.name);
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn create_scaffolds_src_layout_unittest() {
        let parent = tempdir().expect("tempdir");
        let name = "demo_pkg";
        run(&CreateArgs { name: name.into(), path: parent.path().to_path_buf() }).expect("create");
        let root = parent.path().join(name);
        let pyproject = fs::read_to_string(root.join("pyproject.toml")).expect("pyproject");
        assert!(!pyproject.contains("pytest"));
        assert!(pyproject.contains("[tool.panda]"));
        assert!(pyproject.contains("dev-dependencies"));
        assert!(pyproject.contains("packages = [\"src/demo_pkg\"]"));
        assert!(root.join("src").join("demo_pkg").join("__init__.py").is_file());
        assert!(!root.join("demo_pkg").join("__init__.py").exists());
        let test_src = fs::read_to_string(root.join("tests").join("test_hello.py")).expect("test");
        assert!(test_src.contains("unittest.TestCase"));
        assert!(root.join("scripts").join("build.py").is_file());
        assert!(root.join(".gitignore").is_file());
        let gitignore = fs::read_to_string(root.join(".gitignore")).expect("gitignore");
        assert!(gitignore.contains("vendors/"));
        assert!(gitignore.contains(".venv/"));
        assert!(!root.join("package.von").exists());
        assert!(!root.join("legion.von").exists());
    }
}
