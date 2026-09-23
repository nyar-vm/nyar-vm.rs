//! `noodle create` — scaffold a recognizable npm- or pnpm-flavored Node package tree.

use std::{fs, path::PathBuf, process::ExitCode};

use clap::Args;
use miette::{IntoDiagnostic, Result, miette};

use crate::project::PmCompat;

/// `noodle create` arguments.
#[derive(Debug, Clone, Args)]
pub struct CreateArgs {
    /// New package directory / name.
    pub name: String,
    /// Parent directory (default: `.`).
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
    /// Compat profile written into `package.json` (`npm` / `pnpm` / `yarn` / `bun`).
    #[arg(long, default_value = "npm", value_parser = parse_compat)]
    pub compat: PmCompat,
}

fn parse_compat(s: &str) -> std::result::Result<PmCompat, String> {
    PmCompat::from_compat_id(s).ok_or_else(|| format!("unknown compat `{s}` (expected npm|pnpm|yarn|bun)"))
}

const INDEX_JS: &str = r#"/** @returns {string} */
export function hello() {
  return "hello from noodle";
}

console.log(hello());
"#;

const GITIGNORE: &str = r#"# Dependencies (PM vendors + Node resolve tree)
node_modules/
vendors/

# Noodle local home / caches (project-local)
.noodle/

# Build / coverage
dist/
build/
coverage/
*.log
.DS_Store
"#;

fn package_manager_field(compat: PmCompat) -> &'static str {
    match compat {
        PmCompat::Npm => "npm@10.9.0",
        PmCompat::Pnpm => "pnpm@9.15.0",
        PmCompat::Yarn => "yarn@4.0.0",
        PmCompat::Bun => "bun@1.1.0",
    }
}

/// Run `noodle create`.
pub fn run(args: &CreateArgs) -> Result<ExitCode> {
    let root = args.path.join(&args.name);
    if root.exists() {
        return Err(miette!("目标已存在: {}", root.display()));
    }
    fs::create_dir_all(root.join("src")).into_diagnostic()?;

    let compat_label = args.compat.label();
    let pkg = serde_json::json!({
        "name": args.name,
        "version": "0.1.0",
        "private": true,
        "type": "module",
        "main": "src/index.js",
        "exports": {
            ".": "./src/index.js"
        },
        "packageManager": package_manager_field(args.compat),
        "noodle": {
            "compat": compat_label
        },
        "scripts": {
            "start": "node src/index.js",
            "build": "node src/index.js",
            "test": "node --test",
            "fmt": "noodle fmt",
            "lint": "noodle lint",
            "check": "noodle check"
        }
    });
    fs::write(root.join("package.json"), format!("{}\n", serde_json::to_string_pretty(&pkg).into_diagnostic()?)).into_diagnostic()?;
    fs::write(root.join("src/index.js"), INDEX_JS).into_diagnostic()?;
    fs::write(root.join(".gitignore"), GITIGNORE).into_diagnostic()?;

    let layout_blurb = match args.compat {
        PmCompat::Pnpm => {
            "- After install: `vendors/` (PM) + `node_modules/.pnpm/…` (noodle 自研 pnpm-like adapter ≠ pnpm CLI)\n- Lockfile: `noodle-lock.von` (`pnpm-lock.yaml` ignored)\n"
        }
        _ => {
            "- After install: `vendors/` (PM cache layout) + flat `node_modules/` (Node resolve links)\n- Lockfile: `noodle-lock.von` (not `package-lock.json`)\n"
        }
    };
    fs::write(
        root.join("readme.md"),
        format!(
            "# {}\n\nCreated by `noodle create --compat {}`.\n\n```bash\nnoodle install\nnoodle check\nnoodle build\nnoodle test\n```\n\n## Layout\n\n- `package.json` — `packageManager` / `noodle.compat` = `{compat}`\n- `src/` — application sources\n{layout}\n`noodle` 形态借鉴 Vite+（统一入口 + 自带 fmt/lint/check），不是逐命令对齐 npm/pnpm。\n",
            args.name,
            compat_label,
            compat = compat_label,
            layout = layout_blurb,
        ),
    )
    .into_diagnostic()?;

    println!("created {} (compat={})", root.display(), compat_label);
    println!("next: cd {} && noodle install && noodle check", args.name);
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn create_writes_npm_style_tree() {
        let dir = tempdir().unwrap();
        let code = run(&CreateArgs { name: "demo-pkg".into(), path: dir.path().to_path_buf(), compat: PmCompat::Npm }).unwrap();
        assert_eq!(code, ExitCode::SUCCESS);
        let root = dir.path().join("demo-pkg");
        let pkg: serde_json::Value = serde_json::from_str(&fs::read_to_string(root.join("package.json")).unwrap()).unwrap();
        assert_eq!(pkg["name"], "demo-pkg");
        assert_eq!(pkg["main"], "src/index.js");
        assert_eq!(pkg["exports"]["."], "./src/index.js");
        assert_eq!(pkg["noodle"]["compat"], "npm");
        assert_eq!(pkg["packageManager"], "npm@10.9.0");
        assert!(pkg["scripts"]["start"].as_str().unwrap().contains("node"));
        assert!(pkg["scripts"]["check"].as_str().unwrap().contains("noodle check"));
        let index = fs::read_to_string(root.join("src/index.js")).unwrap();
        assert!(index.contains("hello from noodle"));
        let gi = fs::read_to_string(root.join(".gitignore")).unwrap();
        assert!(gi.contains("node_modules/"));
        assert!(gi.contains("vendors/"));
        assert!(root.join("readme.md").is_file());
        assert!(pkg.get("dependencies").is_none());
    }

    #[test]
    fn create_pnpm_compat_scaffold() {
        let dir = tempdir().unwrap();
        let code = run(&CreateArgs { name: "pnpm-demo".into(), path: dir.path().to_path_buf(), compat: PmCompat::Pnpm }).unwrap();
        assert_eq!(code, ExitCode::SUCCESS);
        let root = dir.path().join("pnpm-demo");
        let pkg: serde_json::Value = serde_json::from_str(&fs::read_to_string(root.join("package.json")).unwrap()).unwrap();
        assert_eq!(pkg["noodle"]["compat"], "pnpm");
        assert_eq!(pkg["packageManager"], "pnpm@9.15.0");
        let readme = fs::read_to_string(root.join("readme.md")).unwrap();
        assert!(readme.contains("pnpm-like") || readme.contains(".pnpm"));
        assert!(readme.contains("≠ pnpm CLI") || readme.contains("自研"));
    }
}
