//! Translate `package.json` ↔ neutral `PackageManifest` (product-layer only).

use std::{collections::BTreeMap, fs, path::Path};

use miette::{IntoDiagnostic, Result, miette};
use nyar_package_manager::{DependencySpec, PackageManifest};
use serde_json::{Map, Value};

/// Load `package.json` as a neutral package manifest.
pub fn load_package_manifest(root: &Path) -> Result<PackageManifest> {
    let path = root.join("package.json");
    let text = fs::read_to_string(&path).into_diagnostic().map_err(|e| e.wrap_err(format!("读取 {} 失败", path.display())))?;
    let json: Value = serde_json::from_str(&text).into_diagnostic().map_err(|e| e.wrap_err("解析 package.json 失败"))?;
    Ok(PackageManifest {
        name: json.get("name").and_then(|v| v.as_str()).unwrap_or("unnamed").to_string(),
        version: json.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0").to_string(),
        description: json.get("description").and_then(|v| v.as_str()).map(str::to_string),
        homepage: json.get("homepage").and_then(|v| v.as_str()).map(str::to_string),
        author: author_to_string(json.get("author")),
        license: json.get("license").and_then(|v| v.as_str()).map(str::to_string),
        dependencies: map_deps(json.get("dependencies")),
        dev_dependencies: map_deps(json.get("devDependencies")),
        peer_dependencies: map_deps(json.get("peerDependencies")),
        scripts: map_scripts(json.get("scripts")),
        hooks: BTreeMap::new(),
        publish_config: Default::default(),
        publish: Vec::new(),
        files: Vec::new(),
    })
}

/// Persist dependency maps back into `package.json` (preserves other fields).
pub fn save_dependencies(root: &Path, manifest: &PackageManifest) -> Result<()> {
    let path = root.join("package.json");
    let text = fs::read_to_string(&path).into_diagnostic()?;
    let mut json: Value = serde_json::from_str(&text).into_diagnostic()?;
    let obj = json.as_object_mut().ok_or_else(|| miette!("package.json 根必须是对象"))?;
    upsert_deps(obj, "dependencies", &manifest.dependencies);
    upsert_deps(obj, "devDependencies", &manifest.dev_dependencies);
    if let Some(name) = obj.get_mut("name") {
        *name = Value::String(manifest.name.clone());
    }
    if let Some(version) = obj.get_mut("version") {
        *version = Value::String(manifest.version.clone());
    }
    let pretty = serde_json::to_string_pretty(&json).into_diagnostic()?;
    fs::write(&path, format!("{pretty}\n")).into_diagnostic()?;
    Ok(())
}

fn upsert_deps(obj: &mut Map<String, Value>, key: &str, deps: &BTreeMap<String, DependencySpec>) {
    if deps.is_empty() {
        obj.remove(key);
    }
    else {
        obj.insert(key.into(), deps_to_json(deps));
    }
}

fn author_to_string(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) => Some(s.clone()),
        Value::Object(map) => {
            let name = map.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let email = map.get("email").and_then(|v| v.as_str());
            match email {
                Some(email) if !name.is_empty() => Some(format!("{name} <{email}>")),
                Some(email) => Some(email.to_string()),
                None if !name.is_empty() => Some(name.to_string()),
                None => None,
            }
        }
        _ => None,
    }
}

fn map_deps(value: Option<&Value>) -> BTreeMap<String, DependencySpec> {
    let mut out = BTreeMap::new();
    let Some(Value::Object(map)) = value
    else {
        return out;
    };
    for (name, spec) in map {
        out.insert(name.clone(), dep_spec_from_json(spec));
    }
    out
}

fn dep_spec_from_json(spec: &Value) -> DependencySpec {
    match spec {
        Value::Bool(true) => DependencySpec::Workspace,
        Value::String(s) if s == "workspace:*" || s.starts_with("workspace:") => DependencySpec::Workspace,
        Value::String(s) => DependencySpec::Version(s.clone()),
        Value::Object(map) => DependencySpec::Detailed {
            version: map.get("version").and_then(|v| v.as_str()).map(str::to_string),
            path: map.get("path").and_then(|v| v.as_str()).map(str::to_string),
            registry: map.get("registry").and_then(|v| v.as_str()).map(str::to_string),
        },
        other => DependencySpec::Version(other.to_string()),
    }
}

fn map_scripts(value: Option<&Value>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(Value::Object(map)) = value
    else {
        return out;
    };
    for (name, cmd) in map {
        if let Some(s) = cmd.as_str() {
            out.insert(name.clone(), s.to_string());
        }
    }
    out
}

fn deps_to_json(deps: &BTreeMap<String, DependencySpec>) -> Value {
    let mut map = Map::new();
    for (name, spec) in deps {
        map.insert(name.clone(), dep_spec_to_json(spec));
    }
    Value::Object(map)
}

fn dep_spec_to_json(spec: &DependencySpec) -> Value {
    match spec {
        DependencySpec::Workspace => Value::String("workspace:*".into()),
        DependencySpec::Version(version) => Value::String(version.clone()),
        DependencySpec::Detailed { version, path, registry } => {
            let mut obj = Map::new();
            if let Some(version) = version {
                obj.insert("version".into(), Value::String(version.clone()));
            }
            if let Some(path) = path {
                obj.insert("path".into(), Value::String(path.clone()));
            }
            if let Some(registry) = registry {
                obj.insert("registry".into(), Value::String(registry.clone()));
            }
            Value::Object(obj)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn map_workspace_and_detailed_deps() {
        let json = json!({
            "name": "demo",
            "version": "1.0.0",
            "dependencies": {
                "local": "workspace:*",
                "path-dep": { "path": "../other" },
                "lodash": "^4.17.21"
            }
        });
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), serde_json::to_string_pretty(&json).unwrap()).unwrap();
        let manifest = load_package_manifest(dir.path()).unwrap();
        assert!(matches!(manifest.dependencies.get("local"), Some(DependencySpec::Workspace)));
        assert!(matches!(
            manifest.dependencies.get("path-dep"),
            Some(DependencySpec::Detailed { path: Some(p), .. }) if p == "../other"
        ));
        assert_eq!(manifest.dependencies.get("lodash").and_then(|s| s.version_constraint()), Some("^4.17.21"));
    }

    #[test]
    fn save_omits_empty_dep_maps() {
        let dir = tempdir().unwrap();
        let initial = json!({
            "name": "demo",
            "version": "0.1.0",
            "dependencies": { "left-pad": "1.0.0" },
            "devDependencies": { "typescript": "5.0.0" }
        });
        std::fs::write(dir.path().join("package.json"), format!("{}\n", serde_json::to_string_pretty(&initial).unwrap())).unwrap();

        let mut manifest = load_package_manifest(dir.path()).unwrap();
        manifest.dependencies.clear();
        manifest.dev_dependencies.clear();
        save_dependencies(dir.path(), &manifest).unwrap();

        let saved: Value = serde_json::from_str(&std::fs::read_to_string(dir.path().join("package.json")).unwrap()).unwrap();
        assert!(saved.get("dependencies").is_none());
        assert!(saved.get("devDependencies").is_none());
        assert_eq!(saved["name"], "demo");
    }
}
