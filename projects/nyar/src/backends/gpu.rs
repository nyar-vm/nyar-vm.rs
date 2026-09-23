//! GPU lane backend placeholders（SPIR-V / DXIL emit 委托 C# std-data）。

use miette::Result;

use crate::{
    abstractions::{ArtifactFormat, BackendInputKind},
    backends::{BackendDescriptor, CompilationOptions},
    packaging::{ArtifactSet, TargetLane},
};

/// SPIR-V 模块后端占位：接受 `SpirvModule` 输入，产物为 `.spv`。
#[derive(Debug, Clone, Copy, Default)]
pub struct GpuSpirvBackend;

impl GpuSpirvBackend {
    /// 后端名。
    pub const NAME: &'static str = "gpu-spirv";

    /// 解释器标识。
    pub const INTERPRETER: &'static str = "gpu.spirv";

    /// 返回后端描述。
    pub fn descriptor(&self) -> BackendDescriptor {
        BackendDescriptor { name: Self::NAME.to_string(), input_kind: BackendInputKind::SpirvModule, supported_targets: Vec::new() }
    }

    /// 是否接受给定输入种类与 lane。
    pub fn accept(input_kind: BackendInputKind, lane: TargetLane) -> bool {
        lane == TargetLane::Gpu && input_kind == BackendInputKind::SpirvModule
    }

    /// 描述输出产物格式（首期元数据，emit 由 C# 完成）。
    pub fn describe_output() -> ArtifactFormat {
        ArtifactFormat::SpirvModule
    }

    /// 占位 compile：验证输入种类后返回空产物集。
    pub fn compile_placeholder(&self, _options: &CompilationOptions) -> Result<ArtifactSet> {
        Ok(ArtifactSet::default())
    }
}

/// DXIL 容器后端占位：接受 `DxilContainer` 输入，产物为 `.dxil`。
#[derive(Debug, Clone, Copy, Default)]
pub struct GpuDxilBackend;

impl GpuDxilBackend {
    /// 后端名。
    pub const NAME: &'static str = "gpu-dxil";

    /// 解释器标识。
    pub const INTERPRETER: &'static str = "gpu.dxil";

    /// 返回后端描述。
    pub fn descriptor(&self) -> BackendDescriptor {
        BackendDescriptor { name: Self::NAME.to_string(), input_kind: BackendInputKind::DxilContainer, supported_targets: Vec::new() }
    }

    /// 是否接受给定输入种类与 lane。
    pub fn accept(input_kind: BackendInputKind, lane: TargetLane) -> bool {
        lane == TargetLane::Gpu && input_kind == BackendInputKind::DxilContainer
    }

    /// 描述输出产物格式。
    pub fn describe_output() -> ArtifactFormat {
        ArtifactFormat::DxilContainer
    }

    /// 占位 compile。
    pub fn compile_placeholder(&self, _options: &CompilationOptions) -> Result<ArtifactSet> {
        Ok(ArtifactSet::default())
    }
}

/// 注册 GPU 双后端到 registry（graphic / neural 片段共用）。
pub fn register_gpu_backends(
    registry: &mut super::BackendRegistry,
    fragment: crate::Identifier,
    supported_projection_families: Vec<nyar_optimizer::FutamuraProjectionFamily>,
    supported_targets: Vec<crate::abstractions::BinaryTarget>,
    required_capabilities: Vec<crate::CapabilityTag>,
) {
    let spirv = BundledGpuDescriptor::spirv();
    let dxil = BundledGpuDescriptor::dxil();
    registry.register(spirv.registration(
        fragment.clone(),
        supported_projection_families.clone(),
        supported_targets.clone(),
        required_capabilities.clone(),
    ));
    registry.register(dxil.registration(fragment, supported_projection_families, supported_targets, required_capabilities));
}

struct BundledGpuDescriptor {
    backend_name: &'static str,
    interpreter: &'static str,
    input_kind: BackendInputKind,
}

impl BundledGpuDescriptor {
    fn spirv() -> Self {
        Self { backend_name: GpuSpirvBackend::NAME, interpreter: GpuSpirvBackend::INTERPRETER, input_kind: BackendInputKind::SpirvModule }
    }

    fn dxil() -> Self {
        Self { backend_name: GpuDxilBackend::NAME, interpreter: GpuDxilBackend::INTERPRETER, input_kind: BackendInputKind::DxilContainer }
    }

    fn registration(
        &self,
        fragment: crate::Identifier,
        supported_projection_families: Vec<nyar_optimizer::FutamuraProjectionFamily>,
        supported_targets: Vec<crate::abstractions::BinaryTarget>,
        required_capabilities: Vec<crate::CapabilityTag>,
    ) -> super::BackendInterpreterRegistration {
        use crate::{HostProjectionBoundary, ReferenceManagement};
        super::BackendInterpreterRegistration {
            backend_name: self.backend_name.to_string(),
            priority: if self.input_kind == BackendInputKind::SpirvModule { 110 } else { 100 },
            capability: super::BackendCapability {
                interpreter: crate::Identifier::new(self.interpreter),
                fragment,
                lane: TargetLane::Gpu,
                input_kind: Some(self.input_kind),
                supported_projection_families,
                supported_host_boundaries: vec![HostProjectionBoundary::Native],
                supported_targets,
                required_capabilities,
                reference_management: Some(ReferenceManagement::PerceusRc),
            },
        }
    }
}
