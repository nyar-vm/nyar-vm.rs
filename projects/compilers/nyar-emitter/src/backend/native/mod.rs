#![doc = include_str!("readme.md")]
#![warn(missing_docs)]

mod executable;

use std::path::PathBuf;

use miette::{IntoDiagnostic, Result, WrapErr, miette};
use nyar::{
    abstractions::{ArtifactFormat, BackendInputKind, BinaryArch, BinaryFlavor, BinaryTarget, TargetFamily},
    backends::{BackendDescriptor, CompilationOptions, TargetCodeGenBackend},
    packaging::{ArtifactDescriptor, ArtifactSet, TargetLane},
};
use std_data::binary::{
    coff::{CoffMachine, CoffObjectWriter, CoffSection, CoffSymbol, coff_object_from_sections},
    pe::extract_pe_section,
};

pub use executable::{NativeExecutableError, NativeExecutableKind, classify_native_executable};

/// `Native` 后端输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeBinaryBackendInput {
    /// 已编码的可执行文件（`PE` 或 `ELF`）。
    pub executable: Vec<u8>,
    /// 输出目录。
    pub output_dir: PathBuf,
    /// 入口符号名。
    pub entry_symbol: String,
}

/// `Native` 二进制后端（Windows PE / Linux ELF）。
pub struct NativeBinaryBackend {
    descriptor: BackendDescriptor,
}

impl NativeBinaryBackend {
    /// 创建一个新的原生二进制后端。
    pub fn new() -> Self {
        Self {
            descriptor: BackendDescriptor {
                name: "native".to_string(),
                input_kind: BackendInputKind::CoffObject,
                supported_targets: vec![
                    BinaryTarget::new(TargetFamily::Native, BinaryArch::X64, BinaryFlavor::Native),
                    BinaryTarget::new(TargetFamily::Native, BinaryArch::Arm64, BinaryFlavor::Native),
                    BinaryTarget::new(TargetFamily::Native, BinaryArch::X86, BinaryFlavor::Native),
                ],
            },
        }
    }
}

impl Default for NativeBinaryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl TargetCodeGenBackend for NativeBinaryBackend {
    type Input = NativeBinaryBackendInput;

    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn validate(&self, input: &Self::Input) -> Result<()> {
        classify_native_executable(&input.executable).map_err(|error| miette!("Native 后端 executable 格式无效: {error}"))?;
        Ok(())
    }

    fn compile(&self, input: Self::Input, options: &CompilationOptions) -> Result<ArtifactSet> {
        std::fs::create_dir_all(&input.output_dir)
            .into_diagnostic()
            .wrap_err_with(|| format!("创建输出目录失败：{}", input.output_dir.display()))?;

        match classify_native_executable(&input.executable).map_err(|error| miette!("Native 后端 executable 格式无效: {error}"))? {
            NativeExecutableKind::Elf | NativeExecutableKind::ElfShared => compile_elf(input, options),
            NativeExecutableKind::Pe => compile_pe(input, options),
        }
    }
}

fn compile_pe(input: NativeBinaryBackendInput, options: &CompilationOptions) -> Result<ArtifactSet> {
    let executable_path = input.output_dir.join(format!("{}.exe", options.artifact_name));
    std::fs::write(&executable_path, &input.executable)
        .into_diagnostic()
        .wrap_err_with(|| format!("写入 PE 可执行文件失败：{}", executable_path.display()))?;

    let object_path = input.output_dir.join(format!("{}.obj", options.artifact_name));
    let object = coff_object_from_sections(
        CoffMachine::Amd64,
        vec![CoffSection {
            name: ".text".to_string(),
            data: extract_pe_section(&input.executable, b".text").unwrap_or_default(),
            relocations: Vec::new(),
            characteristics: 0x6000_0020,
        }],
        vec![CoffSymbol { name: input.entry_symbol.clone(), section_index: 1, value: 0, storage_class: 2 }],
    );
    let object_bytes = CoffObjectWriter::write(&object)?;
    std::fs::write(&object_path, object_bytes)
        .into_diagnostic()
        .wrap_err_with(|| format!("写入 `COFF` sidecar 失败：{}", object_path.display()))?;

    let mut artifacts = ArtifactSet::default();
    artifacts.push(ArtifactDescriptor {
        name: options.artifact_name.clone(),
        kind: nyar::ArtifactKind::Executable,
        format: ArtifactFormat::Pe,
        target: options.target.clone(),
        lane: TargetLane::Native,
    });
    artifacts.push(ArtifactDescriptor {
        name: options.artifact_name.clone(),
        kind: nyar::ArtifactKind::Object,
        format: ArtifactFormat::Coff,
        target: options.target.clone(),
        lane: TargetLane::Native,
    });
    Ok(artifacts)
}

fn compile_elf(input: NativeBinaryBackendInput, options: &CompilationOptions) -> Result<ArtifactSet> {
    let kind = classify_native_executable(&input.executable).map_err(|error| miette!("Native 后端 executable 格式无效: {error}"))?;
    let extension = if matches!(kind, NativeExecutableKind::ElfShared) { "so" } else { "" };
    let file_name = if extension.is_empty() { options.artifact_name.clone() } else { format!("{}.{}", options.artifact_name, extension) };
    let executable_path = input.output_dir.join(&file_name);
    std::fs::write(&executable_path, &input.executable)
        .into_diagnostic()
        .wrap_err_with(|| format!("写入 ELF 可执行文件失败：{}", executable_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&executable_path).into_diagnostic()?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&executable_path, perms).into_diagnostic()?;
    }

    let mut artifacts = ArtifactSet::default();
    artifacts.push(ArtifactDescriptor {
        name: options.artifact_name.clone(),
        kind: nyar::ArtifactKind::Executable,
        format: if matches!(kind, NativeExecutableKind::ElfShared) { ArtifactFormat::Elf } else { ArtifactFormat::Elf },
        target: options.target.clone(),
        lane: TargetLane::Native,
    });
    Ok(artifacts)
}
