//! `PublishFormat` / 文件扩展名与 `ArtifactFormat` 的登记映射（与 `legion` GPU lane 对齐）。

use nyar::abstractions::ArtifactFormat;
use rusty_valkyrie::PublishFormat;

/// 返回标准 slug（`spirv-module`、`dxil-container` 等）。
pub fn artifact_format_slug(format: ArtifactFormat) -> &'static str {
    match format {
        ArtifactFormat::Pe => "pe",
        ArtifactFormat::Coff => "coff",
        ArtifactFormat::Elf => "elf",
        ArtifactFormat::MsilText => "msil-text",
        ArtifactFormat::RawBinary => "raw-binary",
        ArtifactFormat::SpirvModule => "spirv-module",
        ArtifactFormat::DxilContainer => "dxil-container",
    }
}

/// 由文件扩展名推断产物格式。
pub fn artifact_format_from_extension(extension: &str) -> Option<ArtifactFormat> {
    match extension.trim_start_matches('.').to_ascii_lowercase().as_str() {
        "spv" => Some(ArtifactFormat::SpirvModule),
        "dxil" => Some(ArtifactFormat::DxilContainer),
        _ => None,
    }
}

/// `PublishFormat::ShaderModule` 对应的物理格式列表（SPIR-V + DXIL 同步）。
pub fn artifact_formats_for_publish_format(format: PublishFormat) -> &'static [ArtifactFormat] {
    match format {
        PublishFormat::ShaderModule => &[ArtifactFormat::SpirvModule, ArtifactFormat::DxilContainer],
        _ => &[],
    }
}
