//! Wrap x64 machine code into a Windows PE executable.
//!
//! This is the **native product** packaging step after Futamura specialization
//! lowers a [`super::NativeResidualModule`] to x64 (subset today). It is unrelated
//! to NyarVM / `.nyar`.

use std::{fs, path::Path};

use miette::{IntoDiagnostic, Result, WrapErr, miette};
use std_data::binary::{
    pe::{NativeImageBuilder, NativePeImage, NativePeWriter, extract_pe_section},
    x86_64::{Reg64, X64Instruction},
};

/// PE executable compiler for legacy VM native output.
#[derive(Debug, Default, Clone, Copy)]
pub struct PeCompiler;

impl PeCompiler {
    /// Compile x64 machine code to a PE file at `output_path`.
    pub fn compile(&self, _entry_function_name: &str, x64_code: &[u8], output_path: &Path) -> Result<Vec<u8>> {
        let bytes = self.compile_to_bytes(x64_code)?;
        if let Some(parent) = output_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .into_diagnostic()
                    .wrap_err_with(|| format!("failed to create output directory: {}", parent.display()))?;
            }
        }
        fs::write(output_path, &bytes).into_diagnostic().wrap_err_with(|| format!("failed to write PE file: {}", output_path.display()))?;
        Ok(bytes)
    }

    /// Compile x64 machine code to PE bytes.
    pub fn compile_to_bytes(&self, x64_code: &[u8]) -> Result<Vec<u8>> {
        if x64_code.is_empty() {
            return self.compile_empty_executable();
        }

        let mut epilog_builder = NativeImageBuilder::new();
        let exit_slot = epilog_builder.import("kernel32", "ExitProcess");
        epilog_builder.push(X64Instruction::Label("_epilog".into()));
        epilog_builder.push(X64Instruction::MovRegReg { dst: Reg64::Rcx, src: Reg64::Rax });
        epilog_builder.push(X64Instruction::CallImport { slot: exit_slot });
        let epilog_pe = epilog_builder.build_executable("_epilog")?;

        let epilog_text = extract_pe_section(&epilog_pe, b".text\0\0\0").ok_or_else(|| miette!("missing .text section in epilog PE"))?;
        let idata = extract_pe_section(&epilog_pe, b".idata\0\0").unwrap_or_default();

        let mut text = Vec::with_capacity(x64_code.len() + epilog_text.len());
        text.extend_from_slice(x64_code);
        text.extend_from_slice(&epilog_text);

        let image = NativePeImage {
            text,
            rdata: Vec::new(),
            idata,
            imports: vec![std_data::binary::pe::NativeDllImport {
                dll: "kernel32.dll".to_string(),
                functions: vec!["ExitProcess".to_string()],
            }],
            entry_point: 0,
        };

        NativePeWriter::write_executable(&image)
    }

    fn compile_empty_executable(&self) -> Result<Vec<u8>> {
        let mut builder = NativeImageBuilder::new();
        let exit_slot = builder.import("kernel32", "ExitProcess");
        builder.push(X64Instruction::Label("entry".into()));
        builder.push(X64Instruction::XorReg { dst: Reg64::Rax, src: Reg64::Rax });
        builder.push(X64Instruction::MovRegReg { dst: Reg64::Rcx, src: Reg64::Rax });
        builder.push(X64Instruction::CallImport { slot: exit_slot });
        builder.build_executable("entry")
    }
}
