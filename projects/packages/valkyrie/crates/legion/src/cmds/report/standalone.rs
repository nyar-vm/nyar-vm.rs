//! Build single-file standalone report HTML (offline `file://` friendly).

use std::{collections::BTreeMap, fs, path::Path};

use base64::{Engine, engine::general_purpose::STANDARD};
use miette::{IntoDiagnostic, Result, WrapErr};
use serde_json::Value;

use super::write::atomic_write_all_text;

/// Glue shim: instantiate from bytes set by bootstrap (no `fetch` for WASM).
const STANDALONE_GLUE: &str = r#"export async function instantiate(_wasmUrl, imports) {
  var bytes = globalThis.__voaStandaloneWasmBytes;
  if (!bytes) throw new Error('standalone wasm bytes missing');
  var result = await WebAssembly.instantiate(bytes, imports || {});
  return result.instance;
}
export async function run(url, imports) { return instantiate(url, imports); }
"#;

/// Build `standalone.html` by inlining boot/css/js/wasm from report dist directory.
pub fn build_standalone_html(report_dir: &Path, output_name: &str) -> Result<()> {
    let index_path = report_dir.join("index.html");
    let mut html = fs::read_to_string(&index_path).into_diagnostic().wrap_err("read index.html")?;

    let boot_path = report_dir.join("boot.js");
    if !boot_path.is_file() {
        atomic_write_all_text(&report_dir.join(output_name), &html)?;
        return Ok(());
    }

    let manifest_path = report_dir.join("manifest.json");
    let manifest_text = fs::read_to_string(&manifest_path).into_diagnostic().wrap_err("read manifest.json")?;
    let manifest: Value = serde_json::from_str(&manifest_text).into_diagnostic().wrap_err("parse manifest.json")?;
    let boot_js = fs::read_to_string(&boot_path).into_diagnostic().wrap_err("read boot.js")?;

    inline_manifest_css(&mut html, report_dir, &manifest)?;

    let component_sources = load_component_sources(report_dir, &manifest)?;
    let wasm_bytes = load_wasm_bytes(report_dir, &manifest)?;

    let manifest_literal = serde_json::to_string(&manifest).into_diagnostic()?;
    let glue_literal = serde_json::to_string(STANDALONE_GLUE).into_diagnostic()?;
    let components_literal = serde_json::to_string(&component_sources).into_diagnostic()?;
    let wasm_b64 = STANDARD.encode(&wasm_bytes);

    let standalone_bootstrap = format!(
        r#"(function() {{
  var manifest = {manifest_literal};
  var glueSource = {glue_literal};
  var componentSources = {components_literal};
  var wasmBytes = Uint8Array.from(atob("{wasm_b64}"), function(c) {{ return c.charCodeAt(0); }});
  globalThis.__voaStandaloneWasmBytes = wasmBytes;

  if (manifest.wasm && manifest.wasm.length > 0) {{
    var glueBlob = new Blob([glueSource], {{ type: 'text/javascript' }});
    manifest.wasm[0].glue = URL.createObjectURL(glueBlob);
    manifest.wasm[0].url = 'standalone-embedded.wasm';
  }}

  if (manifest.components) {{
    for (var i = 0; i < manifest.components.length; i++) {{
      var js = manifest.components[i].js;
      var source = componentSources[js];
      if (typeof source === 'string') {{
        var jsBlob = new Blob([source], {{ type: 'text/javascript' }});
        manifest.components[i].js = URL.createObjectURL(jsBlob);
      }}
    }}
  }}

  var nativeFetch = globalThis.fetch ? globalThis.fetch.bind(globalThis) : null;
  globalThis.fetch = function(input, init) {{
    var url = typeof input === 'string' ? input : (input && input.url) || '';
    if (url === 'manifest.json' || url === '/manifest.json') {{
      return Promise.resolve(new Response(JSON.stringify(manifest), {{ headers: {{ 'Content-Type': 'application/json' }} }}));
    }}
    if (!nativeFetch) {{
      return Promise.reject(new Error('fetch unavailable for ' + url));
    }}
    return nativeFetch(input, init);
  }};
}})();"#,
    );

    let boot_inline = escape_script_body(&boot_js);
    let bootstrap_inline = escape_script_body(&standalone_bootstrap);
    let scripts = format!("<script>{bootstrap_inline}</script><script>{boot_inline}</script>");

    html = html.replace(r#"<script src="boot.js"></script>"#, &scripts).replace(r#"<script src="/boot.js"></script>"#, &scripts);

    atomic_write_all_text(&report_dir.join(output_name), &html)?;
    Ok(())
}

/// When `standalone` is set, build `standalone.html` beside the multi-file report.
pub fn finish_standalone_report(report_dir: &Path, standalone: bool) -> Result<()> {
    if !standalone {
        return Ok(());
    }
    build_standalone_html(report_dir, "standalone.html")?;
    println!("单文件报告已生成：{}", report_dir.join("standalone.html").display());
    Ok(())
}

fn inline_manifest_css(html: &mut String, report_dir: &Path, manifest: &Value) -> Result<()> {
    let mut extra_css = String::new();
    for href in manifest.get("css").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
        let rel = href.trim_start_matches('/');
        let css_path = report_dir.join(rel);
        if css_path.is_file() {
            let css = fs::read_to_string(&css_path).into_diagnostic().wrap_err_with(|| format!("read css {rel}"))?;
            if !css.trim().is_empty() {
                extra_css.push_str("\n/* ");
                extra_css.push_str(rel);
                extra_css.push_str(" */\n");
                extra_css.push_str(&css);
                extra_css.push('\n');
            }
        }
        *html = html.replace(&format!(r#"<link rel="stylesheet" href="{href}">"#), "");
    }
    if !extra_css.trim().is_empty() {
        *html = html.replacen("</head>", &format!("<style>{extra_css}</style></head>"), 1);
    }
    Ok(())
}

fn load_component_sources(report_dir: &Path, manifest: &Value) -> Result<BTreeMap<String, String>> {
    let mut component_sources = BTreeMap::new();
    for component in manifest.get("components").and_then(Value::as_array).into_iter().flatten() {
        if let Some(js) = component.get("js").and_then(Value::as_str) {
            let rel = js.trim_start_matches('/');
            let path = report_dir.join(rel);
            let source = fs::read_to_string(&path).into_diagnostic().wrap_err_with(|| format!("read component glue {rel}"))?;
            component_sources.insert(js.to_string(), source);
        }
    }
    Ok(component_sources)
}

fn load_wasm_bytes(report_dir: &Path, manifest: &Value) -> Result<Vec<u8>> {
    let wasm_rel = manifest
        .get("wasm")
        .and_then(Value::as_array)
        .and_then(|entries| entries.first())
        .and_then(|entry| entry.get("url"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim_start_matches('/')
        .to_string();
    if wasm_rel.is_empty() {
        return Ok(Vec::new());
    }
    fs::read(report_dir.join(&wasm_rel)).into_diagnostic().wrap_err_with(|| format!("read wasm binary {wasm_rel}"))
}

/// Prevent `</script>` in inlined JS from terminating the outer script element.
fn escape_script_body(source: &str) -> String {
    source.replace("</script>", "<\\/script>").replace("</SCRIPT>", "<\\/SCRIPT>")
}
#[cfg(test)]
#[path = "../../../tests/path_wired/cmds_report_standalone.rs"]
mod cmds_report_standalone_tests;
