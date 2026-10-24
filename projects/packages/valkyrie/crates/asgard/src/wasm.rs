//! WASM 编译：复用 `compile_v_bundle`（仅 browser）。

use std::path::Path;

use miette::Result;
use nyar_types::compiler_input::CompilerSourceGroup;
use rusty_valkyrie::CanonicalTarget;

use crate::{
    compile::{GeneratedSource, HostArtifactKind, compile_v_bundle},
    host_backend::HostBackend,
};

/// WASM 编译报告。
#[derive(Debug, Clone)]
pub struct WasmCompileReport {
    /// 主 wasm 文件名（不含路径）。
    pub wasm_filename: String,
    /// glue 文件名。
    pub glue_filename: String,
}

/// 将已解析的源码组与独立生成源码编译为 WASM + glue。
pub fn compile_wasm_bundle(
    source_groups: &[CompilerSourceGroup],
    generated: GeneratedSource<'_>,
    output_dir: &Path,
    module_name: &str,
    target: &CanonicalTarget,
) -> Result<WasmCompileReport> {
    let report = compile_v_bundle(source_groups, generated, output_dir, module_name, target, HostBackend::BrowserDom)?;
    if report.kind != HostArtifactKind::Wasm {
        return Err(miette::miette!("预期 WASM 制品"));
    }
    report.wasm.ok_or_else(|| miette::miette!("缺少 WASM 报告"))
}

/// 将 WASM 产物复制/提升到 dist 根目录（供 manifest 引用）。
pub fn copy_wasm_artifacts_to_dist(output_dir: &Path, report: &WasmCompileReport) -> Result<()> {
    use miette::{IntoDiagnostic, WrapErr};
    use std::fs;

    let wasm_name = &report.wasm_filename;
    let glue_name = &report.glue_filename;
    let wasm_path = output_dir.join(wasm_name);
    if !wasm_path.is_file() {
        return Err(miette::miette!("WASM 编译报告指定的产物不存在: {}", wasm_name));
    }
    write_browser_wasm_glue(output_dir, glue_name)?;
    Ok(())
}

/// 写入浏览器 `boot.js` 可 `import()` 的 WASM glue（`instantiate` / `run`）。
pub fn write_browser_wasm_glue(output_dir: &Path, glue_filename: &str) -> Result<()> {
    use miette::IntoDiagnostic;
    use std::fs;

    let content = r#"export async function instantiate(wasmUrl, imports) {
  const response = await fetch(wasmUrl);
  if (!response.ok) {
    throw new Error('wasm fetch failed: ' + response.status);
  }
  const bytes = await response.arrayBuffer();
  const result = await WebAssembly.instantiate(bytes, imports || {});
  return result.instance;
}

export async function run(wasmUrl, imports) {
  return instantiate(wasmUrl, imports);
}
"#;
    fs::write(output_dir.join(glue_filename), content).into_diagnostic()?;
    Ok(())
}
