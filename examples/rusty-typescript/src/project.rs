use crate::RustyTypescriptFrontend;
use nyar_vm::bytecode::format::NyarcModule;
use oak_core::Source;
use oak_vfs::Vfs;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct PackageJson {
    main: Option<String>,
}

pub struct ProjectLoader<V: Vfs> {
    frontend: RustyTypescriptFrontend,
    vfs: V,
}

impl<V: Vfs> ProjectLoader<V> {
    pub fn new(vfs: V) -> Self {
        Self {
            frontend: RustyTypescriptFrontend::new(),
            vfs,
        }
    }

    pub fn frontend(&self) -> &RustyTypescriptFrontend {
        &self.frontend
    }

    /// Loads and compiles a project starting from a directory or a file.
    pub fn load_project(&mut self, start_uri: &str) -> Result<Vec<NyarcModule>, String> {
        let mut modules = Vec::new();
        let mut loaded_files = HashMap::new();

        let entry_point = if self.vfs.is_dir(start_uri) {
            let pkg_json_uri = if start_uri.ends_with('/') {
                format!("{}package.json", start_uri)
            } else {
                format!("{}/package.json", start_uri)
            };

            if self.vfs.exists(&pkg_json_uri) {
                let source = self
                    .vfs
                    .get_source(&pkg_json_uri)
                    .ok_or_else(|| format!("Failed to read {}", pkg_json_uri))?;
                let content = source.get_text_from(0);
                let pkg: PackageJson = serde_json::from_str(&content).map_err(|e| e.to_string())?;
                let main_file = pkg.main.unwrap_or_else(|| "index.ts".to_string());
                if start_uri.ends_with('/') {
                    format!("{}{}", start_uri, main_file)
                } else {
                    format!("{}/{}", start_uri, main_file)
                }
            } else {
                if start_uri.ends_with('/') {
                    format!("{}index.ts", start_uri)
                } else {
                    format!("{}/index.ts", start_uri)
                }
            }
        } else {
            start_uri.to_string()
        };

        self.load_file_recursive(&entry_point, &mut modules, &mut loaded_files)?;

        Ok(modules)
    }

    fn load_file_recursive(
        &mut self,
        uri: &str,
        modules: &mut Vec<NyarcModule>,
        loaded_files: &mut HashMap<String, usize>,
    ) -> Result<usize, String> {
        // In VFS, we assume URIs are already "canonical" or normalized.
        if let Some(&idx) = loaded_files.get(uri) {
            return Ok(idx);
        }

        let source = self
            .vfs
            .get_source(uri)
            .ok_or_else(|| format!("Failed to read {}", uri))?;
        let content = source.get_text_from(0);

        // Compile current file
        let module = self
            .frontend
            .compile_to_nyar(&content)
            .map_err(|e| format!("Compile error in {}: {:?}", uri, e))?;

        let module_idx = modules.len();
        modules.push(module);
        loaded_files.insert(uri.to_string(), module_idx);

        // Process imports to load dependencies
        let imports = modules[module_idx].imports.clone();
        println!("Loaded file {}, found {} imports", uri, imports.len());

        // Simple relative path resolution for URIs
        let parent_uri = if let Some(last_slash) = uri.rfind('/') {
            &uri[..last_slash]
        } else {
            ""
        };

        for import in imports {
            println!("  Import provider: {}", import.provider);
            if import.provider.starts_with(".") {
                // Relative import
                let dep_uri = if parent_uri.is_empty() {
                    import.provider.clone()
                } else {
                    format!("{}/{}", parent_uri, import.provider)
                };

                let mut final_uri = dep_uri.clone();
                if !self.vfs.exists(&final_uri) {
                    let ts_uri = format!("{}.ts", dep_uri);
                    if self.vfs.exists(&ts_uri) {
                        final_uri = ts_uri;
                    }
                }

                self.load_file_recursive(&final_uri, modules, loaded_files)?;
            }
        }

        Ok(module_idx)
    }
}
