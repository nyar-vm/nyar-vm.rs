//! `CLR` 二进制后端。
//!
//! 直接写入 `PE/COFF` 二进制，不依赖 `ilasm`，保证跨平台可用。
//! `MSIL / PE / COFF` 格式模型与编解码统一由 `vcc-data` 提供。

#![warn(missing_docs)]

pub mod hosting;
mod msil_text_writer;

pub use hosting::{DotNetFramework, DotNetRuntimeConfig, DotNetRuntimeOptions, write_dotnet_deps_json, write_dotnet_runtime_config};
pub use msil_text_writer::MsilTextWriter;
pub use vcc_data::{
    binary::{
        coff::{CoffHeader, CoffMachine, CoffObject, CoffRelocation, CoffRelocationKind, CoffSection, CoffSymbol},
        pe::{ClrMetadataBuilder, ClrMetadataError, PeWriter, PeWriterError, PeWriterOptions},
    },
    text::msil::{
        MethodBodyEncoder, MethodBodyError, MsilAssembly, MsilInstruction, MsilInstructionOperand, MsilMethodBody, MsilMethodRef,
        MsilMethodSignature, MsilModule, MsilOpcode, MsilParser, MsilTextMethod, MsilType, MsilTypeDef,
    },
};

use std::path::PathBuf;

use miette::{Result, miette};
use nyar::{
    abstractions::BackendInputKind,
    backends::{BackendDescriptor, CompilationOptions, TargetCodeGenBackend, clr::ClrImageKind},
    packaging::ArtifactSet,
};
/// `CLR` 二进制后端输入。
#[derive(Debug, Clone)]
pub struct ClrBinaryBackendInput {
    /// `MSIL` 模块。
    pub module: MsilModule,
    /// 输出目录。
    pub output_dir: PathBuf,
    /// 期望生成的镜像口味；为空时自动推断。
    pub image_kind: Option<ClrImageKind>,
}

/// `CLR` 二进制后端。
///
/// 将 `MsilModule` 直接编码为 `PE/COFF` 二进制 `.exe`，
/// 不调用 `ilasm`，保证 `Linux` 上也可运行（生成产物供 `Windows` 运行）。
pub struct ClrBinaryBackend {
    /// 后端描述。
    descriptor: BackendDescriptor,
}

impl ClrBinaryBackend {
    /// 创建一个新的 `CLR` 二进制后端。
    pub fn new() -> Self {
        Self {
            descriptor: BackendDescriptor {
                name: "clr-binary".to_string(),
                input_kind: BackendInputKind::MsilText,
                supported_targets: Vec::new(),
            },
        }
    }
}

impl Default for ClrBinaryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl TargetCodeGenBackend for ClrBinaryBackend {
    type Input = ClrBinaryBackendInput;

    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn validate(&self, _input: &Self::Input) -> Result<()> {
        Err(miette!(code = "nyar::clr::backend::unsupported", "CLR 尚无正式 BackendPrivatePlan 编码器，拒绝编译"))
    }

    fn compile(&self, _input: Self::Input, _options: &CompilationOptions) -> Result<ArtifactSet> {
        Err(miette!(code = "nyar::clr::backend::unsupported", "CLR 尚无正式 BackendPrivatePlan 编码器，拒绝编译"))
    }
}
