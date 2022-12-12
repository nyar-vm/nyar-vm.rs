//! Emit `.nyar` bytecode modules for the Nyar VM backend.

use std::{fs, path::Path};

use miette::{IntoDiagnostic, Result, WrapErr, miette};
use std_data::binary::nyar_ir::{NyarModuleData, encode_module};

/// Encode [`NyarModuleData`] and write it to `output_path`.
pub fn emit_nyar_module(module: &NyarModuleData, output_path: &Path) -> Result<()> {
    let bytes = encode_module(module);

    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .into_diagnostic()
                .wrap_err_with(|| format!("failed to create output directory: {}", parent.display()))?;
        }
    }

    fs::write(output_path, &bytes).into_diagnostic().wrap_err_with(|| format!("failed to write nyar module: {}", output_path.display()))?;

    Ok(())
}
