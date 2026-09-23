#![doc = include_str!("readme.md")]

pub mod clr;
pub mod gpu;
pub mod vm;

use miette::{Result, miette};
use serde::{Deserialize, Serialize};

use crate::{
    CapabilityTag, FutamuraProjectionFamily, HostProjectionBoundary, Identifier, ProjectionPolicy, ReferenceManagement,
    abstractions::{BackendInputKind, BinaryTarget},
    packaging::{ArtifactSet, TargetLane},
    target_profile::{TargetBackendFamily, TargetProfile},
};

/// 编译选项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompilationOptions {
    /// 输出目标。
    pub target: BinaryTarget,
    /// 逻辑产物名。
    pub artifact_name: String,
    /// 是否生成调试信息。
    pub emit_debug_symbols: bool,
    /// 是否启用优化。
    pub optimize: bool,
}

/// 后端描述。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendDescriptor {
    /// 后端名。
    pub name: String,
    /// 期待的 lane 输入种类。
    pub input_kind: BackendInputKind,
    /// 支持的目标。
    pub supported_targets: Vec<BinaryTarget>,
}

/// 后端声明的语义片段能力。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendCapability {
    /// 解释器名。
    pub interpreter: Identifier,
    /// 片段标识。
    pub fragment: Identifier,
    /// 后端真正承诺的 lane。
    pub lane: TargetLane,
    /// 后端真正产出的输入种类。
    pub input_kind: Option<BackendInputKind>,
    /// 后端能够消费的投影家族。
    pub supported_projection_families: Vec<FutamuraProjectionFamily>,
    /// 后端能够消费的宿主边界。
    pub supported_host_boundaries: Vec<HostProjectionBoundary>,
    /// 后端支持的目标。
    pub supported_targets: Vec<BinaryTarget>,
    /// 激活该片段所需的能力标签。
    pub required_capabilities: Vec<CapabilityTag>,
    /// 片段若落到此解释器时的引用管理默认值。
    pub reference_management: Option<ReferenceManagement>,
}

/// 单个后端解释器注册项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendInterpreterRegistration {
    /// 后端名。
    pub backend_name: String,
    /// 注册优先级。
    pub priority: u16,
    /// 该后端声明的能力。
    pub capability: BackendCapability,
}

/// 规划层选择出的后端解释器。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendInterpreterSelection {
    /// 选中的后端名。
    pub backend_name: String,
    /// 选中的解释器名。
    pub interpreter: Identifier,
    /// 被解释的片段。
    pub fragment: Identifier,
    /// 解释器承诺的 lane。
    pub lane: TargetLane,
    /// 解释器产出的输入种类。
    pub input_kind: Option<BackendInputKind>,
    /// 对应宿主边界。
    pub host_boundary: HostProjectionBoundary,
    /// 选中的引用管理策略。
    pub reference_management: ReferenceManagement,
}

/// 面向规划层的后端解释器注册表。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BackendRegistry {
    /// 已注册解释器。
    pub registrations: Vec<BackendInterpreterRegistration>,
}

impl BackendRegistry {
    /// 注册一个后端解释器。
    pub fn register(&mut self, registration: BackendInterpreterRegistration) {
        self.registrations.push(registration);
    }

    /// 为指定片段解析最合适的后端解释器。
    pub fn resolve(
        &self,
        fragment: &Identifier,
        capabilities: &[CapabilityTag],
        target: &BinaryTarget,
        projection_family: FutamuraProjectionFamily,
        host_boundary: HostProjectionBoundary,
        fallback_reference_management: ReferenceManagement,
    ) -> Option<BackendInterpreterSelection> {
        self.registrations
            .iter()
            .filter(|registration| registration.capability.fragment == *fragment)
            .filter(|registration| {
                registration.capability.supported_projection_families.is_empty()
                    || registration.capability.supported_projection_families.contains(&projection_family)
            })
            .filter(|registration| {
                registration.capability.supported_host_boundaries.is_empty()
                    || registration.capability.supported_host_boundaries.contains(&host_boundary)
            })
            .filter(|registration| {
                registration.capability.supported_targets.is_empty()
                    || registration.capability.supported_targets.iter().any(|candidate| candidate == target)
            })
            .filter(|registration| {
                registration.capability.required_capabilities.iter().all(|required| capabilities.iter().any(|provided| provided == required))
            })
            .max_by_key(|registration| registration.priority)
            .map(|registration| BackendInterpreterSelection {
                backend_name: registration.backend_name.clone(),
                interpreter: registration.capability.interpreter.clone(),
                fragment: registration.capability.fragment.clone(),
                lane: registration.capability.lane,
                input_kind: registration.capability.input_kind,
                host_boundary,
                reference_management: registration.capability.reference_management.unwrap_or(fallback_reference_management),
            })
    }
}

/// 基于目标 profile 生成 bundled projection policy。
pub fn projection_policy_for_target_profile(profile: &TargetProfile) -> Result<ProjectionPolicy> {
    Ok(ProjectionPolicy {
        family: projection_family_for_backend(profile.backend_family)?,
        host_boundary: profile.host_boundary,
        reference_management: profile.reference_management,
        prefer_small_artifacts: matches!(profile.backend_family, TargetBackendFamily::Wasm),
        preserve_effect_boundaries: true,
    })
}

/// 将 backend family 解释为对应的投影家族。
pub fn projection_family_for_backend(backend_family: TargetBackendFamily) -> Result<FutamuraProjectionFamily> {
    match backend_family {
        TargetBackendFamily::Clr => Ok(FutamuraProjectionFamily::Clr),
        TargetBackendFamily::Jvm => Ok(FutamuraProjectionFamily::Jvm),
        TargetBackendFamily::Wasm => Ok(FutamuraProjectionFamily::Wasm),
        TargetBackendFamily::Native => Ok(FutamuraProjectionFamily::Native),
        TargetBackendFamily::Gpu => Ok(FutamuraProjectionFamily::Gpu),
        TargetBackendFamily::NyarVm => Ok(FutamuraProjectionFamily::NyarVm),
        TargetBackendFamily::Unknown => Err(miette!("未解析的后端家族不能进入 projection 装配")),
    }
}

/// 目标代码生成后端。
///
/// 这里不统一所有目标的物理表示，只统一“每个后端都必须诚实声明自己吃什么”。
pub trait TargetCodeGenBackend {
    /// 后端真实消费的输入类型。
    type Input;

    /// 返回后端描述。
    fn descriptor(&self) -> &BackendDescriptor;

    /// 验证输入是否满足本后端路线约束。
    fn validate(&self, input: &Self::Input) -> miette::Result<()>;

    /// 只对通过验证的输入执行目标相关编码与产物生成。
    fn compile(&self, input: Self::Input, options: &CompilationOptions) -> miette::Result<ArtifactSet>;
}
