//! `manifest.json` 生成。

use serde::Serialize;

use crate::{
    awsl::LoweredComponent,
    codegen::{JsGlueOutput, encode_mobile_ui_package},
    config::VoaConfig,
};

/// manifest 输出。
#[derive(Debug, Clone)]
pub struct ManifestOutput {
    /// JSON 文本。
    pub content: String,
    /// 相对路径。
    pub relative_path: String,
}

/// Asset URL style in manifest entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestUrlMode {
    /// Root-absolute paths (`/c/foo.js`) for served apps.
    Absolute,
    /// Relative paths (`c/foo.js`) for `file://` report previews.
    Relative,
}

/// 生成 manifest.json（默认 absolute URLs，对齐全站 hosted browser 产物）。
pub fn generate_manifest(
    config: &VoaConfig,
    components: &[LoweredComponent],
    js_outputs: &[JsGlueOutput],
    wasm_name: &str,
    wasm_built: bool,
) -> ManifestOutput {
    let module = config.name.clone().unwrap_or_else(|| "asgard-app".into());
    generate_manifest_with_urls(&module, &config.build.mode, components, js_outputs, wasm_name, wasm_built, ManifestUrlMode::Absolute)
}

/// 生成 manifest.json，可切换 relative / absolute asset URLs。
pub fn generate_manifest_with_urls(
    module_stem: &str,
    mode: &str,
    components: &[LoweredComponent],
    js_outputs: &[JsGlueOutput],
    wasm_name: &str,
    wasm_built: bool,
    url_mode: ManifestUrlMode,
) -> ManifestOutput {
    let module = module_stem.to_string();
    let css = vec![url_path(&format!("{}.css", module.replace('.', "-")), url_mode)];

    let wasm = if wasm_built {
        vec![WasmEntry {
            name: module.clone(),
            url: url_path(&format!("{wasm_name}.wasm"), url_mode),
            glue: url_path(&format!("{wasm_name}.mjs"), url_mode),
        }]
    }
    else {
        vec![]
    };

    let manifest = VoaManifest {
        module: module.clone(),
        mode: mode.to_string(),
        css: css.clone(),
        wasm,
        render_ir: Some(RenderIrManifest { encoding: "asgard-ui-v1".into(), size: encode_mobile_ui_package(components).len() }),
        components: components
            .iter()
            .zip(js_outputs.iter())
            .map(|(component, js)| ManifestComponent {
                name: js_relative_name(&js.relative_path),
                js: url_path(&js.relative_path.replace('\\', "/"), url_mode),
                strategy: component.hydrate_strategy.clone(),
                island: component.island_type.clone(),
            })
            .collect(),
    };

    let content = serde_json::to_string_pretty(&manifest).unwrap_or_else(|_| "{}".into());
    ManifestOutput { content, relative_path: "manifest.json".into() }
}

fn url_path(path: &str, mode: ManifestUrlMode) -> String {
    let trimmed = path.trim_start_matches('/');
    match mode {
        ManifestUrlMode::Absolute => format!("/{trimmed}"),
        ManifestUrlMode::Relative => trimmed.to_string(),
    }
}

fn js_relative_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).trim_end_matches(".js").to_string()
}

#[derive(Serialize)]
struct VoaManifest {
    module: String,
    mode: String,
    css: Vec<String>,
    wasm: Vec<WasmEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    render_ir: Option<RenderIrManifest>,
    components: Vec<ManifestComponent>,
}

#[derive(Serialize)]
struct RenderIrManifest {
    encoding: String,
    size: usize,
}

#[derive(Serialize)]
struct WasmEntry {
    name: String,
    url: String,
    glue: String,
}

#[derive(Serialize)]
struct ManifestComponent {
    name: String,
    js: String,
    strategy: String,
    island: String,
}
