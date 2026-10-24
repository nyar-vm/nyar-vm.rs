//! Debug 侧车：`build.mode: dev` 或 `sourcemap` 时额外写出 RenderIR。

use std::{fs, path::Path};

use miette::{IntoDiagnostic, Result, WrapErr};

use crate::{awsl::LoweredComponent, codegen::encode_mobile_ui_package, config::VoaConfig};

/// 是否应写出 `debug/render-ir.bin`。
pub fn should_emit_render_ir_sidecar(config: &VoaConfig) -> bool {
    config.build.mode == "dev" || config.build.sourcemap
}

/// 写出 `debug/render-ir.bin`（非 Release 交付物）。
pub fn write_render_ir_sidecar(output_dir: &Path, components: &[LoweredComponent]) -> Result<()> {
    let debug_dir = output_dir.join("debug");
    fs::create_dir_all(&debug_dir).into_diagnostic().wrap_err("创建 debug 目录失败")?;
    let bytes = encode_mobile_ui_package(components);
    fs::write(debug_dir.join("render-ir.bin"), bytes).into_diagnostic().wrap_err("写入 render-ir.bin 失败")?;
    Ok(())
}
