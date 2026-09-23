//! Translate Python-native manifests to and from the neutral package-manager view.

use std::{collections::BTreeMap, fs, path::Path};

use miette::{IntoDiagnostic, Result, miette};
use nyar_package_manager::{DependencySpec, PackageManifest};
use toml_edit::{Array, DocumentMut, Item, Table, Value, value};

/// Load project dependencies into a neutral manifest.
pub fn load_package_manifest(root: &Path) -> Result<PackageManifest> {
    if root.join("pyproject.toml").is_file() {
        return load_from_pyproject(root);
    }
    if root.join("requirements.txt").is_file() || root.join("requirements-dev.txt").is_file() {
        return load_from_requirements(root);
    }
    Err(miette!("缺少 pyproject.toml / requirements.txt"))
}

/// Persist dependency changes to the project's native Python manifest.
pub fn save_dependencies(root: &Path, manifest: &PackageManifest) -> Result<()> {
    if root.join("pyproject.toml").is_file() {
        return save_pyproject(root, manifest);
    }
    if root.join("requirements.txt").is_file() || root.join("requirements-dev.txt").is_file() || !manifest.dependencies.is_empty() {
        return save_requirements(root, manifest);
    }
    Err(miette!("缺少可写的 pyproject.toml / requirements.txt"))
}

fn load_from_pyproject(root: &Path) -> Result<PackageManifest> {
    let path = root.join("pyproject.toml");
    let text = fs::read_to_string(&path).into_diagnostic()?;
    let document = text.parse::<DocumentMut>().into_diagnostic()?;
    let project = document.get("project").and_then(Item::as_table);
    let poetry = document.get("tool").and_then(|v| v.get("poetry")).and_then(Item::as_table);
    let name = string_field(project, "name").or_else(|| string_field(poetry, "name")).unwrap_or_else(|| "unnamed".into());
    let version = string_field(project, "version").or_else(|| string_field(poetry, "version")).unwrap_or_else(|| "0.0.0".into());
    let dependencies = if let Some(array) = project.and_then(|p| p.get("dependencies")).and_then(Item::as_array) {
        array.iter().filter_map(Value::as_str).map(split_req).map(|(name, version)| (name, DependencySpec::Version(version))).collect()
    }
    else {
        poetry_dependencies(poetry)
    };
    let mut dev_dependencies = panda_dev_dependencies(&document);
    if dev_dependencies.is_empty() {
        // Prefer reading traditional / PEP layouts; poetry group.dev is rewritten on poetry saves.
        dev_dependencies = dependency_groups_dev(&document);
    }
    if dev_dependencies.is_empty() {
        dev_dependencies = optional_dependencies_dev(project);
    }
    if dev_dependencies.is_empty() {
        dev_dependencies = poetry_dev_dependencies(&document);
    }
    Ok(neutral_manifest(name, version, dependencies, dev_dependencies))
}

fn panda_dev_dependencies(document: &DocumentMut) -> BTreeMap<String, DependencySpec> {
    document
        .get("tool")
        .and_then(|v| v.get("panda"))
        .and_then(|v| v.get("dev-dependencies"))
        .and_then(Item::as_array)
        .map(|array| {
            array.iter().filter_map(Value::as_str).map(split_req).map(|(name, version)| (name, DependencySpec::Version(version))).collect()
        })
        .unwrap_or_default()
}

/// PEP 735 `[dependency-groups] dev = [...]`.
fn dependency_groups_dev(document: &DocumentMut) -> BTreeMap<String, DependencySpec> {
    document
        .get("dependency-groups")
        .and_then(|v| v.get("dev"))
        .and_then(Item::as_array)
        .map(|array| {
            array.iter().filter_map(Value::as_str).map(split_req).map(|(name, version)| (name, DependencySpec::Version(version))).collect()
        })
        .unwrap_or_default()
}

/// Classic `[project.optional-dependencies] dev = [...]`.
fn optional_dependencies_dev(project: Option<&Table>) -> BTreeMap<String, DependencySpec> {
    project
        .and_then(|p| p.get("optional-dependencies"))
        .and_then(Item::as_table)
        .and_then(|t| t.get("dev"))
        .and_then(Item::as_array)
        .map(|array| {
            array.iter().filter_map(Value::as_str).map(split_req).map(|(name, version)| (name, DependencySpec::Version(version))).collect()
        })
        .unwrap_or_default()
}

fn poetry_dev_dependencies(document: &DocumentMut) -> BTreeMap<String, DependencySpec> {
    let poetry = document.get("tool").and_then(|v| v.get("poetry"));
    // Prefer Poetry 1.2+ groups, then legacy `[tool.poetry.dev-dependencies]`.
    let table = poetry
        .and_then(|v| v.get("group"))
        .and_then(|v| v.get("dev"))
        .and_then(|v| v.get("dependencies"))
        .and_then(Item::as_table)
        .or_else(|| poetry.and_then(|v| v.get("dev-dependencies")).and_then(Item::as_table));
    table
        .map(|deps| {
            deps.iter()
                .map(|(name, item)| {
                    let version = item.as_str().unwrap_or("*").to_string();
                    (name.to_string(), DependencySpec::Version(version))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn load_from_requirements(root: &Path) -> Result<PackageManifest> {
    let dependencies = parse_requirements_file(&root.join("requirements.txt"));
    let mut dev_dependencies = parse_requirements_file(&root.join("requirements-dev.txt"));
    if dev_dependencies.is_empty() {
        // Common alternate name.
        dev_dependencies = parse_requirements_file(&root.join("requirements_dev.txt"));
    }
    let name = root.file_name().and_then(|n| n.to_str()).unwrap_or("unnamed").to_string();
    Ok(neutral_manifest(name, "0.0.0".into(), dependencies, dev_dependencies))
}

fn parse_requirements_file(path: &Path) -> BTreeMap<String, DependencySpec> {
    let Ok(text) = fs::read_to_string(path)
    else {
        return BTreeMap::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !is_pip_option_line(line))
        .map(split_req)
        .map(|(name, version)| (name, DependencySpec::Version(version)))
        .collect()
}

/// Skip pip CLI flags / includes (`-r`, `-e`, `-c`, `--hash`, …); no full pip resolver.
fn is_pip_option_line(line: &str) -> bool {
    line.starts_with('-')
}

fn save_pyproject(root: &Path, manifest: &PackageManifest) -> Result<()> {
    let path = root.join("pyproject.toml");
    let text = fs::read_to_string(&path).into_diagnostic()?;
    let mut document = text.parse::<DocumentMut>().into_diagnostic()?;
    let uses_poetry =
        document.get("tool").and_then(|v| v.get("poetry")).is_some() && document.get("project").and_then(|v| v.get("dependencies")).is_none();
    if uses_poetry {
        let dependencies = ensure_table_path(&mut document, &["tool", "poetry", "dependencies"]);
        dependencies.clear();
        dependencies.insert("python", value("*"));
        for (name, spec) in &manifest.dependencies {
            dependencies.insert(name, value(spec.version_constraint().unwrap_or("*")));
        }
        // Poetry save parity: rewrite group.dev (and drop legacy table) alongside `[tool.panda]`.
        save_poetry_group_dev(&mut document, &manifest.dev_dependencies);
    }
    else {
        let project = ensure_table_path(&mut document, &["project"]);
        project["dependencies"] = Item::Value(Value::Array(dependency_array(&manifest.dependencies)));
    }
    let panda = ensure_table_path(&mut document, &["tool", "panda"]);
    panda["dev-dependencies"] = Item::Value(Value::Array(dependency_array(&manifest.dev_dependencies)));
    fs::write(path, document.to_string()).into_diagnostic()
}

/// Write `[tool.poetry.group.dev.dependencies]` from the neutral manifest.
///
/// Clears legacy `[tool.poetry.dev-dependencies]` when present so load priority stays unambiguous.
fn save_poetry_group_dev(document: &mut DocumentMut, dev_dependencies: &BTreeMap<String, DependencySpec>) {
    if let Some(poetry) = document.get_mut("tool").and_then(|v| v.get_mut("poetry")).and_then(Item::as_table_like_mut) {
        poetry.remove("dev-dependencies");
    }
    let group_dev = ensure_table_path(document, &["tool", "poetry", "group", "dev", "dependencies"]);
    group_dev.clear();
    for (name, spec) in dev_dependencies {
        group_dev.insert(name, value(spec.version_constraint().unwrap_or("*")));
    }
}

fn save_requirements(root: &Path, manifest: &PackageManifest) -> Result<()> {
    write_requirements_file(&root.join("requirements.txt"), &manifest.dependencies)?;
    if !manifest.dev_dependencies.is_empty() || root.join("requirements-dev.txt").is_file() || root.join("requirements_dev.txt").is_file() {
        let path = if root.join("requirements_dev.txt").is_file() && !root.join("requirements-dev.txt").is_file() {
            root.join("requirements_dev.txt")
        }
        else {
            root.join("requirements-dev.txt")
        };
        write_requirements_file(&path, &manifest.dev_dependencies)?;
    }
    Ok(())
}

fn write_requirements_file(path: &Path, dependencies: &BTreeMap<String, DependencySpec>) -> Result<()> {
    let mut output = String::new();
    for (name, spec) in dependencies {
        output.push_str(&format_requirement(name, spec));
        output.push('\n');
    }
    fs::write(path, output).into_diagnostic()
}

fn neutral_manifest(
    name: String,
    version: String,
    dependencies: BTreeMap<String, DependencySpec>,
    dev_dependencies: BTreeMap<String, DependencySpec>,
) -> PackageManifest {
    PackageManifest {
        name,
        version,
        description: None,
        homepage: None,
        author: None,
        license: None,
        dependencies,
        dev_dependencies,
        peer_dependencies: BTreeMap::new(),
        scripts: BTreeMap::new(),
        hooks: BTreeMap::new(),
        publish_config: Default::default(),
        publish: Vec::new(),
        files: Vec::new(),
    }
}

fn poetry_dependencies(table: Option<&Table>) -> BTreeMap<String, DependencySpec> {
    table
        .and_then(|t| t.get("dependencies"))
        .and_then(Item::as_table)
        .map(|deps| {
            deps.iter()
                .filter(|(name, _)| *name != "python")
                .map(|(name, item)| {
                    let version = item.as_str().unwrap_or("*").to_string();
                    (name.to_string(), DependencySpec::Version(version))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn ensure_table_path<'a>(document: &'a mut DocumentMut, path: &[&str]) -> &'a mut Table {
    let mut item = document.as_item_mut();
    for segment in path {
        if !item.get(segment).is_some_and(Item::is_table) {
            item[segment] = Item::Table(Table::new());
        }
        item = &mut item[segment];
    }
    item.as_table_mut().expect("table path was initialized")
}

fn dependency_array(dependencies: &BTreeMap<String, DependencySpec>) -> Array {
    let mut array = Array::new();
    for (name, spec) in dependencies {
        array.push(format_requirement(name, spec));
    }
    array
}

fn format_requirement(name: &str, spec: &DependencySpec) -> String {
    let version = spec.version_constraint().unwrap_or("*");
    if version == "*" || version == "latest" {
        name.to_string()
    }
    else if version.starts_with(|ch: char| ch.is_ascii_digit()) {
        format!("{name}=={version}")
    }
    else {
        format!("{name}{version}")
    }
}

fn string_field(table: Option<&Table>, key: &str) -> Option<String> {
    table?.get(key)?.as_str().map(str::to_string)
}

fn split_req(item: &str) -> (String, String) {
    let item = item.trim();
    // Strip environment markers: `pkg>=1; python_version>="3.10"`
    let item = item.split(';').next().unwrap_or(item).trim();
    let (name_part, version) = {
        let mut found = None;
        for sep in ["==", ">=", "<=", "~=", "!=", ">", "<"] {
            if let Some((name, ver)) = item.split_once(sep) {
                found = Some((name.trim().to_string(), format!("{sep}{}", ver.trim())));
                break;
            }
        }
        found.unwrap_or_else(|| (item.to_string(), "*".into()))
    };
    // Strip extras: `requests[security]` → `requests`
    let name = match name_part.split_once('[') {
        Some((base, _)) => base.trim().to_string(),
        None => name_part,
    };
    (name, version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn pyproject_round_trip_preserves_product_settings() {
        let root = tempdir().expect("tempdir");
        fs::write(
            root.path().join("pyproject.toml"),
            "[project]\nname = \"demo\"\nversion = \"1.0.0\"\ndependencies = [\"httpx>=0.27\"]\n\n[tool.panda]\nmanager = \"uv\"\n",
        )
        .expect("write");
        let mut manifest = load_package_manifest(root.path()).expect("load");
        manifest.add_dependency("rich", ">=13");
        save_dependencies(root.path(), &manifest).expect("save");
        let saved = fs::read_to_string(root.path().join("pyproject.toml")).expect("read");
        assert!(saved.contains("manager = \"uv\""));
        assert!(saved.contains("rich>=13"));
        assert!(!root.path().join("legion.von").exists());
        assert!(!root.path().join("package.von").exists());
    }

    #[test]
    fn requirements_round_trip() {
        let root = tempdir().expect("tempdir");
        fs::write(root.path().join("requirements.txt"), "httpx>=0.27\n# comment\n").expect("write");
        let mut manifest = load_package_manifest(root.path()).expect("load");
        assert!(manifest.dependencies.contains_key("httpx"));
        manifest.add_dependency("rich", "13.0.0");
        save_dependencies(root.path(), &manifest).expect("save");
        let saved = fs::read_to_string(root.path().join("requirements.txt")).expect("read");
        assert!(saved.contains("httpx>=0.27"));
        assert!(saved.contains("rich==13.0.0"));
        assert!(!root.path().join("package.von").exists());
    }

    #[test]
    fn loads_and_saves_requirements_dev() {
        let root = tempdir().expect("tempdir");
        fs::write(root.path().join("requirements.txt"), "httpx>=0.27\n").expect("write");
        fs::write(root.path().join("requirements-dev.txt"), "mypy>=1.0\n# note\n-r other.txt\n").expect("write");
        let mut manifest = load_package_manifest(root.path()).expect("load");
        assert!(manifest.dependencies.contains_key("httpx"));
        assert_eq!(manifest.dev_dependencies.get("mypy").and_then(|s| s.version_constraint()), Some(">=1.0"));
        assert!(!manifest.dev_dependencies.contains_key("-r"));
        manifest.dev_dependencies.insert("ruff".into(), DependencySpec::Version(">=0.4".into()));
        save_dependencies(root.path(), &manifest).expect("save");
        let saved = fs::read_to_string(root.path().join("requirements-dev.txt")).expect("read");
        assert!(saved.contains("mypy>=1.0"));
        assert!(saved.contains("ruff>=0.4"));
    }

    #[test]
    fn skips_pip_options_and_strips_extras_markers() {
        assert_eq!(split_req("requests[security]>=2.31; python_version>=\"3.10\""), ("requests".into(), ">=2.31".into()));
        assert!(is_pip_option_line("-e ."));
        assert!(is_pip_option_line("-r requirements-dev.txt"));
        assert!(!is_pip_option_line("httpx>=0.27"));
    }

    #[test]
    fn loads_dependency_groups_dev() {
        let root = tempdir().expect("tempdir");
        fs::write(
            root.path().join("pyproject.toml"),
            "[project]\nname = \"demo\"\nversion = \"0.1.0\"\ndependencies = []\n\n[dependency-groups]\ndev = [\"mypy>=1.8\"]\n",
        )
        .expect("write");
        let manifest = load_package_manifest(root.path()).expect("load");
        assert_eq!(manifest.dev_dependencies.get("mypy").and_then(|s| s.version_constraint()), Some(">=1.8"));
    }

    #[test]
    fn loads_optional_dependencies_dev() {
        let root = tempdir().expect("tempdir");
        fs::write(
            root.path().join("pyproject.toml"),
            "[project]\nname = \"demo\"\nversion = \"0.1.0\"\ndependencies = []\n\n[project.optional-dependencies]\ndev = [\"pytest>=8\"]\n",
        )
        .expect("write");
        let manifest = load_package_manifest(root.path()).expect("load");
        assert_eq!(manifest.dev_dependencies.get("pytest").and_then(|s| s.version_constraint()), Some(">=8"));
    }

    #[test]
    fn save_writes_panda_dev_dependencies() {
        let root = tempdir().expect("tempdir");
        fs::write(root.path().join("pyproject.toml"), "[project]\nname = \"demo\"\nversion = \"0.1.0\"\ndependencies = []\n").expect("write");
        let mut manifest = load_package_manifest(root.path()).expect("load");
        manifest.dev_dependencies.insert("mypy".into(), DependencySpec::Version("1.0".into()));
        save_dependencies(root.path(), &manifest).expect("save");
        let saved = fs::read_to_string(root.path().join("pyproject.toml")).expect("read");
        assert!(saved.contains("[tool.panda]"));
        assert!(saved.contains("mypy==1.0") || saved.contains("mypy1.0") || saved.contains("\"mypy==1.0\""));
    }

    #[test]
    fn loads_panda_dev_dependencies() {
        let root = tempdir().expect("tempdir");
        fs::write(
            root.path().join("pyproject.toml"),
            "[project]\nname = \"demo\"\nversion = \"0.1.0\"\ndependencies = [\"httpx\"]\n\n[tool.panda]\ndev-dependencies = [\"mypy>=1.0\"]\n",
        )
        .expect("write");
        let manifest = load_package_manifest(root.path()).expect("load");
        assert!(manifest.dependencies.contains_key("httpx"));
        assert_eq!(manifest.dev_dependencies.get("mypy").and_then(|s| s.version_constraint()), Some(">=1.0"));
    }

    #[test]
    fn loads_poetry_group_dev_when_panda_empty() {
        let root = tempdir().expect("tempdir");
        fs::write(
            root.path().join("pyproject.toml"),
            r#"[tool.poetry]
name = "demo"
version = "0.1.0"

[tool.poetry.dependencies]
python = "^3.10"
httpx = "^0.27"

[tool.poetry.group.dev.dependencies]
mypy = "^1.8"
"#,
        )
        .expect("write");
        let manifest = load_package_manifest(root.path()).expect("load");
        assert!(manifest.dependencies.contains_key("httpx"));
        assert!(!manifest.dependencies.contains_key("python"));
        assert_eq!(manifest.dev_dependencies.get("mypy").and_then(|s| s.version_constraint()), Some("^1.8"));
    }

    #[test]
    fn loads_legacy_poetry_dev_dependencies() {
        let root = tempdir().expect("tempdir");
        fs::write(
            root.path().join("pyproject.toml"),
            r#"[tool.poetry]
name = "demo"
version = "0.1.0"

[tool.poetry.dependencies]
python = "*"

[tool.poetry.dev-dependencies]
mypy = "^1.4"
"#,
        )
        .expect("write");
        let manifest = load_package_manifest(root.path()).expect("load");
        assert_eq!(manifest.dev_dependencies.get("mypy").and_then(|s| s.version_constraint()), Some("^1.4"));
    }

    #[test]
    fn panda_dev_overrides_poetry_group_dev() {
        let root = tempdir().expect("tempdir");
        fs::write(
            root.path().join("pyproject.toml"),
            r#"[tool.poetry]
name = "demo"
version = "0.1.0"

[tool.poetry.dependencies]
python = "*"

[tool.poetry.group.dev.dependencies]
mypy = "^1.0"

[tool.panda]
dev-dependencies = ["ruff>=0.4"]
"#,
        )
        .expect("write");
        let manifest = load_package_manifest(root.path()).expect("load");
        assert!(manifest.dev_dependencies.contains_key("ruff"));
        assert!(!manifest.dev_dependencies.contains_key("mypy"));
    }

    #[test]
    fn poetry_save_rewrites_group_dev_and_panda() {
        let root = tempdir().expect("tempdir");
        fs::write(
            root.path().join("pyproject.toml"),
            r#"[tool.poetry]
name = "demo"
version = "0.1.0"

[tool.poetry.dependencies]
python = "*"
httpx = "^0.27"

[tool.poetry.group.dev.dependencies]
mypy = "^1.0"
"#,
        )
        .expect("write");
        let mut manifest = load_package_manifest(root.path()).expect("load");
        manifest.add_dependency("rich", "13.0.0");
        manifest.dev_dependencies.insert("ruff".into(), DependencySpec::Version(">=0.4".into()));
        manifest.dev_dependencies.remove("mypy");
        save_dependencies(root.path(), &manifest).expect("save");
        let saved = fs::read_to_string(root.path().join("pyproject.toml")).expect("read");
        assert!(saved.contains("[tool.poetry.group.dev.dependencies]"));
        assert!(saved.contains("ruff"));
        assert!(!saved.contains("mypy"));
        assert!(saved.contains("[tool.panda]"));
        assert!(saved.contains("rich"));
        // Round-trip: poetry group and panda stay in sync for the new set.
        let reloaded = load_package_manifest(root.path()).expect("reload");
        assert!(reloaded.dev_dependencies.contains_key("ruff"));
        assert!(!reloaded.dev_dependencies.contains_key("mypy"));
    }

    #[test]
    fn poetry_save_migrates_legacy_dev_to_group() {
        let root = tempdir().expect("tempdir");
        fs::write(
            root.path().join("pyproject.toml"),
            r#"[tool.poetry]
name = "demo"
version = "0.1.0"

[tool.poetry.dependencies]
python = "*"

[tool.poetry.dev-dependencies]
mypy = "^1.4"
"#,
        )
        .expect("write");
        let mut manifest = load_package_manifest(root.path()).expect("load");
        manifest.dev_dependencies.insert("ruff".into(), DependencySpec::Version("^0.4".into()));
        save_dependencies(root.path(), &manifest).expect("save");
        let saved = fs::read_to_string(root.path().join("pyproject.toml")).expect("read");
        assert!(saved.contains("[tool.poetry.group.dev.dependencies]"));
        assert!(saved.contains("mypy"));
        assert!(saved.contains("ruff"));
        assert!(!saved.contains("[tool.poetry.dev-dependencies]"));
    }
}
