//! Browser hydrate island packaging: auto glue + boot + CSS + manifest（无手写业务 JS）。

use std::{fs, path::Path};

use miette::{IntoDiagnostic, Result, WrapErr};

use crate::{
    awsl::LoweredComponent,
    codegen::{extract_and_merge_styles, generate_boot_script, generate_component_glue, manifest_url_for_mode},
    package::{ManifestUrlMode, generate_manifest_with_urls},
};

/// Options for packaging browser hydrate islands.
#[derive(Debug, Clone)]
pub struct IslandPackageOptions {
    /// Module stem used for CSS / manifest (`legion-test`, `asgard-app`, …).
    pub module_stem: String,
    /// WASM artifact stem (filename without extension).
    pub wasm_stem: String,
    /// Whether WASM artifacts were successfully built.
    pub wasm_built: bool,
    /// Use relative URLs in manifest (`c/foo.js`) instead of root-absolute (`/c/foo.js`).
    pub relative_urls: bool,
    /// CSS merge mode (`merge` / `scoped`, forwarded to extract_and_merge_styles).
    pub css_mode: String,
    /// Build mode string written into manifest.
    pub mode: String,
    /// Optional Tailwind CLI output appended to merged CSS.
    pub tailwind_css: Option<String>,
}

impl Default for IslandPackageOptions {
    fn default() -> Self {
        Self {
            module_stem: "asgard-app".into(),
            wasm_stem: "asgard-app".into(),
            wasm_built: false,
            relative_urls: false,
            css_mode: "merge".into(),
            mode: "production".into(),
            tailwind_css: None,
        }
    }
}

/// Report from [`package_browser_islands`].
#[derive(Debug, Clone)]
pub struct IslandPackageReport {
    /// Number of component glue JS files written.
    pub js_file_count: usize,
    /// CSS filenames written to `output_dir`.
    pub css_filenames: Vec<String>,
    /// Component route names packaged.
    pub routes: Vec<String>,
}

/// Write auto glue, boot.js, CSS, and manifest for hydrate islands.
///
/// Does **not** compile WASM — callers run `compile_wasm_bundle` first and pass `wasm_built`.
pub fn package_browser_islands(components: &[LoweredComponent], output_dir: &Path, opts: &IslandPackageOptions) -> Result<IslandPackageReport> {
    fs::create_dir_all(output_dir.join("c")).into_diagnostic().wrap_err("创建 dist/c 失败")?;

    let mut js_outputs = Vec::new();
    let mut routes = Vec::new();
    for component in components {
        let js = generate_component_glue(component, &opts.wasm_stem);
        let js_path = output_dir.join(&js.relative_path);
        if let Some(parent) = js_path.parent() {
            fs::create_dir_all(parent).into_diagnostic()?;
        }
        fs::write(&js_path, &js.content).into_diagnostic().wrap_err("写入 JS 胶水失败")?;
        routes.push(component.route_name.clone());
        js_outputs.push(js);
    }

    let css_outputs = extract_and_merge_styles(components, &opts.module_stem, &opts.css_mode, opts.tailwind_css.as_deref());
    let css_filenames: Vec<String> = css_outputs.iter().map(|css| css.filename.clone()).collect();
    for css in &css_outputs {
        let css_path = output_dir.join(&css.filename);
        fs::write(&css_path, &css.content).into_diagnostic().wrap_err("写入 CSS 失败")?;
    }

    let url_mode = if opts.relative_urls { ManifestUrlMode::Relative } else { ManifestUrlMode::Absolute };
    let manifest =
        generate_manifest_with_urls(&opts.module_stem, &opts.mode, components, &js_outputs, &opts.wasm_stem, opts.wasm_built, url_mode);
    fs::write(output_dir.join(&manifest.relative_path), &manifest.content).into_diagnostic()?;

    let boot_js = generate_boot_script(&opts.wasm_stem, manifest_url_for_mode(opts.relative_urls));
    fs::write(output_dir.join("boot.js"), &boot_js).into_diagnostic().wrap_err("写入 boot.js 失败")?;
    let _ = fs::remove_file(output_dir.join("asgard-runtime.js"));

    Ok(IslandPackageReport { js_file_count: js_outputs.len(), css_filenames, routes })
}
