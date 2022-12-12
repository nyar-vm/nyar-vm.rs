use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

use nyar_package_registry::{Package, Registry};

use crate::{DependencySpec, PackageManagerError, Result};

/// Resolved dependency node.
#[derive(Debug, Clone)]
pub struct DependencyNode {
    pub name: String,
    pub version: String,
    pub registry: String,
    pub package: Package,
    pub children: Vec<DependencyNode>,
}

/// Recursive dependency resolver (depth-limited, no full SAT).
pub struct DependencyResolver {
    registries: HashMap<String, Arc<dyn Registry>>,
    max_depth: usize,
}

impl DependencyResolver {
    pub fn new(registries: HashMap<String, Arc<dyn Registry>>) -> Self {
        Self { registries, max_depth: 32 }
    }

    pub fn resolve_all(&self, dependencies: &BTreeMap<String, DependencySpec>, default_registry: &str) -> Result<Vec<DependencyNode>> {
        let mut visiting = BTreeSet::new();
        let mut resolved = BTreeMap::new();
        let mut out = Vec::new();
        for (name, spec) in dependencies {
            if spec.is_workspace() {
                continue;
            }
            let version = spec.version_constraint().unwrap_or("latest");
            let registry_name = match spec {
                DependencySpec::Detailed { registry: Some(registry), .. } => registry.as_str(),
                _ => default_registry,
            };
            out.push(self.resolve_one(name, version, registry_name, 0, &mut visiting, &mut resolved)?);
        }
        Ok(out)
    }

    pub fn resolve_one(
        &self,
        name: &str,
        version: &str,
        registry_name: &str,
        depth: usize,
        visiting: &mut BTreeSet<String>,
        resolved: &mut BTreeMap<String, DependencyNode>,
    ) -> Result<DependencyNode> {
        if depth > self.max_depth {
            return Err(PackageManagerError::message(format!("dependency depth exceeded for {name}")));
        }
        if let Some(node) = resolved.get(name) {
            return Ok(node.clone());
        }
        if !visiting.insert(name.to_string()) {
            return Err(PackageManagerError::message(format!("dependency cycle detected at {name}")));
        }
        let registry =
            self.registries.get(registry_name).ok_or_else(|| PackageManagerError::message(format!("注册器 {registry_name} 未找到")))?;
        let package = registry.get_package(name, version)?;
        let mut children = Vec::new();
        for (child_name, child_version) in &package.dependency_versions {
            children.push(self.resolve_one(child_name, child_version, registry_name, depth + 1, visiting, resolved)?);
        }
        visiting.remove(name);
        let node = DependencyNode {
            name: package.name.clone(),
            version: package.version.clone(),
            registry: registry_name.to_string(),
            package,
            children,
        };
        resolved.insert(name.to_string(), node.clone());
        Ok(node)
    }
}
