use legion::{artifact_format_from_extension, artifact_format_slug, artifact_formats_for_publish_format};
use nyar::abstractions::ArtifactFormat;
use rusty_valkyrie::PublishFormat;

#[test]
fn shader_module_publish_format_maps_to_spirv_and_dxil() {
    let formats = artifact_formats_for_publish_format(PublishFormat::ShaderModule);
    assert_eq!(formats, &[ArtifactFormat::SpirvModule, ArtifactFormat::DxilContainer]);
    assert_eq!(artifact_format_slug(ArtifactFormat::SpirvModule), "spirv-module");
    assert_eq!(artifact_format_slug(ArtifactFormat::DxilContainer), "dxil-container");
    assert_eq!(artifact_format_from_extension("spv"), Some(ArtifactFormat::SpirvModule));
    assert_eq!(artifact_format_from_extension(".dxil"), Some(ArtifactFormat::DxilContainer));
}
