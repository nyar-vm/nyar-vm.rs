#![doc = include_str!("readme.md")]
#![warn(missing_docs)]

mod artifacts;
mod assembly;
mod backend;
pub mod contracts;
mod driver;
pub mod executable_provider;
mod lowering;
mod nullable_profiles;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    nyar_backend_clr::ClrBinaryBackendInput, nyar_backend_jvm::JvmBinaryBackendInput, nyar_backend_native::NativeBinaryBackendInput,
    nyar_backend_wasi::WasmBinaryBackendInput,
};
pub use backend::{
    clr as nyar_backend_clr, jvm as nyar_backend_jvm, native as nyar_backend_native, vm as nyar_backend_vm, wasi as nyar_backend_wasi,
    wasm_js_glue as nyar_backend_wasm_js_glue,
};
use miette::{Result, miette};
use nyar::{
    BackendCapability, BackendInputKind, BackendInterpreterRegistration, BackendRegistry, BinaryTarget, CapabilityTag, ClrSuspendStrategy,
    ExternalCallEdge, ExternalImportLink, HostProjectionBoundary, Identifier, InternalCallEdge, PartitionBackendRequirement, ProjectionPolicy,
    QualifiedName, ReferenceManagement, SuspendConsumptionModel, SuspendRuntimePayload, TargetBackendFamily, TargetFamily, TargetLane,
    TargetProfile, TheoryBundle, VmSuspendStrategy, WitnessCallEdge, WitnessSubmission, backends::CompilationOptions, packaging::ArtifactSet,
    suspend_consumption_model_for_lane,
};
use nyar_types::{AggregateLayoutPlan, FlagsLayout, SingletonInstancePlan, SumTypeLayout};
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    driver::partitioning::{backend_family_for_partition, merge_partition_reports, partition_artifact_name},
    executable_provider::ExecutableFunction,
    lowering::{lower_fragment_to_driver_input, write_clr_msil_sidecar, write_wasm_wat_sidecar},
};

pub use assembly::fragment_submission_from_assembled;
pub use executable_provider::{ExecutableProvider, FunctionView, SuspendMetadataView};
pub use lowering::pattern_matching_contract::{PatternMatchingContractError, validate_pattern_matching_invariants};
pub use nullable_profiles::{
    FragmentNullableBoolProfile, FragmentNullableIntrinsicKind, FragmentNullableIntrinsicUse, FragmentNullableTryCall,
};
pub use nyar::ArtifactPartition;

/// Integration-test support wrappers for crate-internal lowering and compile flows.
#[doc(hidden)]
pub mod testing {
    #[cfg(feature = "legacy-lanes")]
    use crate::executable_provider::ExecutableFunction;
    use miette::Result;
    #[cfg(feature = "legacy-lanes")]
    use nyar::QualifiedName;
    use nyar::{PartitionBackendRequirement, backends::CompilationOptions};
    #[cfg(feature = "legacy-lanes")]
    use std_data::binary::{elf::NativeElfImageBuilder, pe::NativeImageBuilder};

    #[cfg(feature = "legacy-lanes")]
    pub use super::lowering::backends::jvm_mir::{JvmLocalKind, jvm_local_slot_conflicts};

    use super::{
        DriverCompileReport, DriverCompileRequest, FragmentSubmission, LoweredBackendInput,
        artifacts::suspend_sidecar::{serialize_control_flow_payload, serialize_suspend_runtime_payload},
        compile_with_bundled_backends,
        nyar_backend_wasi::WasmBinaryModule,
    };
    #[cfg(feature = "legacy-lanes")]
    use super::{
        nyar_backend_clr::{MsilMethodBody, MsilModule, MsilTypeDef},
        nyar_backend_jvm::{JvmClassFile, JvmInstruction, JvmMethodSignature},
    };
    use nyar_types::SingletonInstancePlan;
    #[cfg(feature = "legacy-lanes")]
    use nyar_types::{AggregateLayoutPlan, FlagsLayout, SumTypeLayout};
    #[cfg(feature = "legacy-lanes")]
    use std_data::binary::nyar_ir::NyarModuleData;

    /// Target profile used by normalized physical-contract observations.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum PhysicalContractTarget {
        /// JVM classfile preparation.
        Jvm,
        /// CLR MSIL preparation.
        Clr,
        /// WASM core preparation without a host boundary.
        WasmCore,
        /// WASM core projected through Node/JavaScript glue.
        WasmJsGlue,
        /// WASM core projected through a WASI component boundary.
        WasiComponent,
    }

    /// Return the canonical Semantic MIR observation for a Rust executable
    /// submission.  Paired conformance tests compare this verbatim with the
    /// Valkyrie Semantic MIR observation; neither side exposes backend data.
    pub fn semantic_mir_observation(submission: &FragmentSubmission, case_id: &str) -> String {
        let result = super::lowering::features::semantic_mir_contract::validate_submission(submission);
        super::lowering::features::semantic_mir_contract::observation(case_id, result.as_ref().map(|_| ()).map_err(|error| error))
    }

    /// Return the canonical physical-contract observation for a Rust
    /// Semantic MIR submission and target profile.
    pub fn physical_contract_observation(submission: &FragmentSubmission, case_id: &str, target: PhysicalContractTarget) -> String {
        use super::lowering::features::physical_contract::PhysicalBackend;
        let backend = match target {
            PhysicalContractTarget::Jvm => PhysicalBackend::Jvm,
            PhysicalContractTarget::Clr => PhysicalBackend::Clr,
            PhysicalContractTarget::WasmCore => PhysicalBackend::WasmCore,
            PhysicalContractTarget::WasmJsGlue => PhysicalBackend::WasmJsGlue,
            PhysicalContractTarget::WasiComponent => PhysicalBackend::WasiComponent,
        };
        let result = super::lowering::features::physical_contract::validate_physical_submission(submission, backend);
        super::lowering::features::physical_contract::observation(case_id, result.as_ref().map(|_| ()).map_err(|error| error))
    }

    /// Lower a fragment submission to a CLR MSIL module.
    #[cfg(feature = "legacy-lanes")]
    pub fn lower_fragment_to_clr_msil(submission: &FragmentSubmission) -> Result<super::nyar_backend_clr::MsilModule> {
        super::lowering::testing_lower_fragment_to_clr_msil(submission)
    }

    /// Lower one MIR function to a CLR method body.
    #[cfg(feature = "legacy-lanes")]
    pub fn lower_mir_to_clr_method(
        submission: &FragmentSubmission,
        operation: &QualifiedName,
        mir_function: &ExecutableFunction,
    ) -> Result<MsilMethodBody> {
        super::lowering::testing_lower_mir_to_clr_method(submission, operation, mir_function)
    }

    /// Lower one MIR function to a JVM method body.
    #[cfg(feature = "legacy-lanes")]
    pub fn lower_mir_to_jvm_method(
        submission: &FragmentSubmission,
        operation: &QualifiedName,
        mir_function: &ExecutableFunction,
    ) -> JvmMethodSignature {
        super::lowering::testing_lower_mir_to_jvm_method(submission, operation, mir_function)
    }

    /// Lower a fragment submission to a WASM module via MIR.
    /// GC is mandatory for wasm/wasi targets (language specification).
    pub fn lower_fragment_to_wasm_mir_module(submission: &FragmentSubmission, export_name: &str) -> WasmBinaryModule {
        super::lowering::testing_lower_fragment_to_wasm_mir_module(submission, export_name)
    }

    /// Serialize a control-flow payload using the production sidecar wire format.
    pub fn serialize_control_flow_sidecar(payload: &nyar::ControlFlowPayload) -> String {
        serialize_control_flow_payload(payload)
    }

    /// Serialize a suspend-runtime payload using the production sidecar wire format.
    pub fn serialize_suspend_runtime_sidecar(payload: &nyar::SuspendRuntimePayload) -> String {
        serialize_suspend_runtime_payload(payload)
    }

    /// Build CLR aggregate type definitions from planned layouts.
    #[cfg(feature = "legacy-lanes")]
    pub fn build_clr_type_defs(plan: &AggregateLayoutPlan) -> Vec<MsilTypeDef> {
        super::lowering::testing_build_clr_type_defs(plan)
    }

    /// Build CLR nominal type definitions for sum and flags layouts.
    #[cfg(feature = "legacy-lanes")]
    pub fn build_clr_nominal_type_defs(sum_types: &[SumTypeLayout], flags_types: &[FlagsLayout]) -> Vec<MsilTypeDef> {
        super::lowering::testing_build_clr_nominal_type_defs(sum_types, flags_types)
    }

    /// Lower a fragment submission to a Nyar VM module.
    #[cfg(feature = "legacy-lanes")]
    pub fn lower_fragment_to_nyar_module(submission: &FragmentSubmission) -> NyarModuleData {
        super::lowering::testing_lower_fragment_to_nyar_module(submission)
    }

    /// Build JVM companion singleton classes for a fragment.
    #[cfg(feature = "legacy-lanes")]
    pub fn build_jvm_singleton_classes(submission: &FragmentSubmission) -> Vec<JvmClassFile> {
        super::lowering::testing_build_jvm_singleton_classes(submission)
    }

    /// Lower a fragment submission to a JVM class file.
    #[cfg(feature = "legacy-lanes")]
    pub fn lower_fragment_to_jvm_class(submission: &FragmentSubmission) -> Result<JvmClassFile, miette::Report> {
        super::lowering::testing_lower_fragment_to_jvm_class(submission).map_err(|error| miette::miette!("{error}"))
    }

    /// Append witness methods to an existing JVM class file.
    #[cfg(feature = "legacy-lanes")]
    pub fn append_jvm_witness_methods(class_file: &mut JvmClassFile, submission: &FragmentSubmission) -> Option<Vec<JvmInstruction>> {
        super::lowering::testing_append_jvm_witness_methods(class_file, submission)
    }

    /// Decode one WASM uleb128 integer (test helper for section parsing).
    pub fn decode_wasm_uleb128(bytes: &[u8], pos: &mut usize) -> u32 {
        super::lowering::testing_decode_wasm_uleb128(bytes, pos)
    }

    /// Apply singleton augmentation to an existing MSIL module.
    #[cfg(feature = "legacy-lanes")]
    pub fn augment_msil_with_singletons(submission: &FragmentSubmission, module: &mut MsilModule) -> Result<()> {
        super::lowering::testing_augment_msil_with_singletons(submission, module)
    }

    /// Apply witness augmentation to an existing MSIL module.
    #[cfg(feature = "legacy-lanes")]
    pub fn augment_msil_with_witness(submission: &FragmentSubmission, module: &mut MsilModule) {
        super::lowering::testing_augment_msil_with_witness(submission, module).expect("CLR witness metadata must resolve before emission")
    }

    /// Apply suspend augmentation (state-machine emission) to an existing MSIL module.
    #[cfg(feature = "legacy-lanes")]
    pub fn augment_msil_with_suspend(submission: &FragmentSubmission, module: &mut MsilModule) {
        super::lowering::testing_augment_msil_with_suspend(submission, module)
    }

    /// Shared suspend dispatch-case expansion helper.
    pub fn dispatch_case_keys(artifact: &nyar::SuspendFunctionArtifact) -> Vec<u32> {
        super::lowering::testing_dispatch_case_keys(artifact)
    }

    /// Lower a fragment submission to a host-boundary-specific WASM module.
    pub fn lower_fragment_to_wasm_module(
        submission: &FragmentSubmission,
        host_boundary: nyar::HostProjectionBoundary,
    ) -> Result<(WasmBinaryModule, Vec<(String, String)>), miette::Report> {
        super::lowering::testing_lower_fragment_to_wasm_module(submission, host_boundary).map_err(|error| miette::miette!("{error}"))
    }

    /// Build suspend run-loop bytes with witness dispatch for one WASM state-machine artifact.
    pub fn suspend_run_loop_with_witness_wasm_bytes(
        artifact: &nyar::SuspendFunctionArtifact,
        witness_offset: u32,
        witness_type_index: u32,
        method_index: u32,
        function_index: u32,
        returns_i32: bool,
    ) -> Vec<u8> {
        super::lowering::testing_suspend_run_loop_with_witness_wasm_bytes(
            artifact,
            witness_offset,
            witness_type_index,
            method_index,
            function_index,
            returns_i32,
        )
    }

    /// Lower a fragment submission to a native executable image.
    #[cfg(feature = "legacy-lanes")]
    pub fn lower_fragment_to_native_executable(
        submission: &FragmentSubmission,
        host_flavor: &str,
    ) -> Result<(Vec<u8>, String), miette::Report> {
        super::lowering::testing_lower_fragment_to_native_executable(submission, host_flavor).map_err(|error| miette::miette!("{error}"))
    }

    /// Emit native witness tables into PE/ELF image builders.
    #[cfg(feature = "legacy-lanes")]
    pub fn emit_native_witness_tables(
        pe: Option<&mut NativeImageBuilder>,
        elf: Option<&mut NativeElfImageBuilder>,
        submission: &FragmentSubmission,
    ) -> Result<(), miette::Report> {
        super::lowering::testing_emit_native_witness_tables(pe, elf, submission).map_err(|error| miette::miette!("{error}"))
    }

    #[cfg(feature = "legacy-lanes")]
    pub fn lower_mir_functions_to_native_msvc(submission: &FragmentSubmission, function: &mut std_data::binary::x86_64::MsvcFunctionBuilder) {
        super::lowering::testing_lower_mir_functions_to_native_msvc(submission, function)
    }

    #[cfg(feature = "legacy-lanes")]
    pub fn lower_mir_functions_to_native_sysv(submission: &FragmentSubmission, function: &mut std_data::binary::x86_64::SysvFunctionBuilder) {
        super::lowering::testing_lower_mir_functions_to_native_sysv(submission, function)
    }

    #[cfg(feature = "legacy-lanes")]
    pub fn lower_suspend_witness_calls_windows(
        submission: &FragmentSubmission,
        function: &mut std_data::binary::x86_64::MsvcFunctionBuilder,
        builder: &mut NativeImageBuilder,
    ) {
        super::lowering::testing_lower_suspend_witness_calls_windows(submission, function, builder)
    }

    #[cfg(feature = "legacy-lanes")]
    pub fn lower_suspend_witness_calls_linux(
        submission: &FragmentSubmission,
        function: &mut std_data::binary::x86_64::SysvFunctionBuilder,
        builder: &mut NativeElfImageBuilder,
    ) {
        super::lowering::testing_lower_suspend_witness_calls_linux(submission, function, builder)
    }

    /// JVM/CLR shared helper: compute aggregate field local slot offset.
    pub fn field_slot_index(submission: &FragmentSubmission, layout_id: Option<u32>, type_name: &str, field: &str) -> u16 {
        super::lowering::testing_field_slot_index(submission, layout_id, type_name, field)
    }

    #[cfg(feature = "legacy-lanes")]
    pub const NATIVE_VALUE_AREA_BASE: i32 = super::lowering::TESTING_NATIVE_VALUE_AREA_BASE;
    #[cfg(feature = "legacy-lanes")]
    pub const SUSPEND_SPILL_RSP_OFFSET: i32 = super::lowering::TESTING_SUSPEND_SPILL_RSP_OFFSET;
    #[cfg(feature = "legacy-lanes")]
    pub fn native_value_area_size(submission: &FragmentSubmission) -> u32 {
        super::lowering::testing_native_value_area_size(submission)
    }

    /// Append singleton metadata custom sections to an existing WASM module.
    pub fn append_singleton_metadata_sections(module: &mut WasmBinaryModule, submission: &FragmentSubmission) {
        super::lowering::testing_append_singleton_metadata_sections(module, submission)
    }

    /// Inject singleton accessor globals and exports into an existing WASM module.
    pub fn augment_wasm_with_singleton_accessors(module: &mut WasmBinaryModule, submission: &FragmentSubmission) {
        super::lowering::testing_augment_wasm_with_singleton_accessors(module, submission)
    }

    /// Render one singleton metadata line using the production schema.
    pub fn singleton_metadata_line(plan: &SingletonInstancePlan) -> String {
        super::lowering::testing_singleton_metadata_line(plan)
    }

    /// WASM-GC anyref marker byte for MIR contract tests.
    pub const WASM_GC_ANYREF: u8 = super::lowering::TESTING_WASM_GC_ANYREF;

    /// Compile a previously lowered backend input with bundled backends.
    pub fn compile_lowered_backend_input(
        artifact_name: &str,
        requirement: PartitionBackendRequirement,
        input: LoweredBackendInput,
        generate_runtime_config: bool,
        options: &CompilationOptions,
    ) -> Result<DriverCompileReport> {
        compile_with_bundled_backends(DriverCompileRequest {
            artifact_name,
            requirement,
            input: input.into_driver_backend_input(),
            generate_runtime_config,
            options,
        })
    }
}

/// bundled backend 解释器的显式 capability 描述。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundledBackendCapabilityDescriptor {
    /// backend family。
    pub backend_family: TargetBackendFamily,
    /// backend 注册名。
    pub backend_name: &'static str,
    /// interpreter 标识。
    pub interpreter: &'static str,
    /// 对应 lane。
    pub lane: TargetLane,
    /// 产出的 backend 输入类型。
    pub input_kind: Option<BackendInputKind>,
    /// 对应目标家族。
    pub target_family: TargetFamily,
    /// 支持的宿主边界。
    pub supported_host_boundaries: &'static [HostProjectionBoundary],
    /// 默认引用管理策略。
    pub reference_management: ReferenceManagement,
    /// 可接受的动态分发路由。
    pub backend_route: BackendRoute,
}

impl BundledBackendCapabilityDescriptor {
    /// 判断当前 descriptor 是否能消费指定的规划需求。
    pub fn supports_requirement(&self, requirement: &PartitionBackendRequirement) -> bool {
        requirement.lane == self.lane
            && self.input_kind.is_none_or(|kind| kind == requirement.input_kind)
            && requirement.target.family == self.target_family
            && self.supported_host_boundaries.contains(&requirement.host_boundary)
            && requirement.reference_management == self.reference_management
    }

    /// 将 descriptor 转成 backend 注册项。
    pub fn interpreter_registration(
        &self,
        fragment: Identifier,
        supported_projection_families: Vec<nyar::FutamuraProjectionFamily>,
        supported_targets: Vec<BinaryTarget>,
        required_capabilities: Vec<CapabilityTag>,
    ) -> BackendInterpreterRegistration {
        BackendInterpreterRegistration {
            backend_name: self.backend_name.to_string(),
            priority: 100,
            capability: BackendCapability {
                interpreter: Identifier::new(self.interpreter),
                fragment,
                lane: self.lane,
                input_kind: self.input_kind,
                supported_projection_families,
                supported_host_boundaries: self.supported_host_boundaries.to_vec(),
                supported_targets,
                required_capabilities,
                reference_management: Some(self.reference_management),
            },
        }
    }
}

const CLR_HOST_BOUNDARIES: &[HostProjectionBoundary] = &[HostProjectionBoundary::Clr];
const JVM_HOST_BOUNDARIES: &[HostProjectionBoundary] = &[HostProjectionBoundary::Jvm];
const WASM_HOST_BOUNDARIES: &[HostProjectionBoundary] = &[HostProjectionBoundary::WasmJsGlue, HostProjectionBoundary::WasiComponent];
const NATIVE_HOST_BOUNDARIES: &[HostProjectionBoundary] = &[HostProjectionBoundary::Native];
const VM_HOST_BOUNDARIES: &[HostProjectionBoundary] = &[HostProjectionBoundary::Vm];

/// 返回 bundled backend family 对应的显式 capability 描述。
pub fn bundled_backend_capability_descriptor(backend_family: TargetBackendFamily) -> Option<BundledBackendCapabilityDescriptor> {
    match backend_family {
        TargetBackendFamily::Clr => Some(BundledBackendCapabilityDescriptor {
            backend_family,
            backend_name: "clr-binary",
            interpreter: "clr.msil",
            lane: TargetLane::Clr,
            input_kind: Some(BackendInputKind::MsilText),
            target_family: TargetFamily::Clr,
            supported_host_boundaries: CLR_HOST_BOUNDARIES,
            reference_management: ReferenceManagement::HostGc,
            backend_route: BackendRoute::WitnessCapable,
        }),
        TargetBackendFamily::Jvm => Some(BundledBackendCapabilityDescriptor {
            backend_family,
            backend_name: "jvm-binary",
            interpreter: "jvm.classfile",
            lane: TargetLane::Jvm,
            input_kind: Some(BackendInputKind::JvmClassFile),
            target_family: TargetFamily::Jvm,
            supported_host_boundaries: JVM_HOST_BOUNDARIES,
            reference_management: ReferenceManagement::HostGc,
            backend_route: BackendRoute::WitnessCapable,
        }),
        TargetBackendFamily::Wasm => Some(BundledBackendCapabilityDescriptor {
            backend_family,
            backend_name: "wasm-binary",
            interpreter: "wasm.module",
            lane: TargetLane::Wasm,
            input_kind: Some(BackendInputKind::WasmModule),
            target_family: TargetFamily::Wasm,
            supported_host_boundaries: WASM_HOST_BOUNDARIES,
            reference_management: ReferenceManagement::HostGc,
            backend_route: BackendRoute::WitnessCapable,
        }),
        TargetBackendFamily::Native => Some(BundledBackendCapabilityDescriptor {
            backend_family,
            backend_name: "native-binary",
            interpreter: "native.object",
            lane: TargetLane::Native,
            input_kind: Some(BackendInputKind::CoffObject),
            target_family: TargetFamily::Native,
            supported_host_boundaries: NATIVE_HOST_BOUNDARIES,
            reference_management: ReferenceManagement::PerceusRc,
            backend_route: BackendRoute::WitnessCapable,
        }),
        TargetBackendFamily::Gpu => Some(BundledBackendCapabilityDescriptor {
            backend_family,
            backend_name: "gpu-spirv",
            interpreter: "gpu.spirv",
            lane: TargetLane::Gpu,
            input_kind: Some(BackendInputKind::SpirvModule),
            target_family: TargetFamily::Gpu,
            supported_host_boundaries: NATIVE_HOST_BOUNDARIES,
            reference_management: ReferenceManagement::PerceusRc,
            backend_route: BackendRoute::StaticOnly,
        }),
        TargetBackendFamily::NyarVm => Some(BundledBackendCapabilityDescriptor {
            backend_family,
            backend_name: "nyar-vm",
            interpreter: "nyar.vm",
            lane: TargetLane::Vm,
            input_kind: None,
            target_family: TargetFamily::NyarVm,
            supported_host_boundaries: VM_HOST_BOUNDARIES,
            reference_management: ReferenceManagement::HostGc,
            backend_route: BackendRoute::Full,
        }),
        TargetBackendFamily::Unknown => None,
    }
}

/// 基于 target profile 与语义片段生成 bundled backend registry。
pub fn bundled_backend_registry(
    fragments: &[nyar::SemanticFragment],
    target_profile: &TargetProfile,
    projection_policy: &ProjectionPolicy,
) -> BackendRegistry {
    let mut registry = BackendRegistry::default();
    let binary_target: BinaryTarget = target_profile.canonical_target.into();
    if target_profile.backend_family == TargetBackendFamily::Gpu {
        for fragment in fragments {
            nyar::backends::gpu::register_gpu_backends(
                &mut registry,
                fragment.id.clone(),
                vec![projection_policy.family],
                vec![binary_target.clone()],
                fragment.required_capabilities.clone(),
            );
        }
        return registry;
    }
    let Some(descriptor) = bundled_backend_capability_descriptor(target_profile.backend_family)
    else {
        return registry;
    };
    for fragment in fragments {
        registry.register(descriptor.interpreter_registration(
            fragment.id.clone(),
            vec![projection_policy.family],
            vec![binary_target.clone()],
            fragment.required_capabilities.clone(),
        ));
    }
    registry
}

/// 后端路由可接受的动态分发能力。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendRoute {
    /// 仅接受静态分发。
    StaticOnly,
    /// 接受 witness 分发，不接受 effect handler。
    WitnessCapable,
    /// 接受全部已建模分发。
    Full,
}

/// backend boundary 可接受的分发形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendDispatchKind {
    /// 静态分发。
    Static,
    /// witness 分发。
    Witness,
    /// effect handler 分发。
    EffectHandler,
}

/// 驱动层在 backend boundary 前校验的输入形状。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BackendInputShape {
    /// 是否仍然包含未消解的开放 row 证据。
    pub contains_open_row_evidence: bool,
    /// 是否仍然包含未消解的名义检查。
    pub contains_unresolved_nominal_checks: bool,
}

/// backend boundary 校验失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendBoundaryError {
    /// backend 输入仍包含开放 row 证据。
    OpenRowEvidence,
    /// backend 输入仍包含未消解名义检查。
    UnresolvedNominalCheck,
    /// 当前路由不支持 trait witness 分发。
    UnsupportedTraitDispatch {
        /// 不支持的分发路由。
        route: BackendRoute,
    },
    /// 当前路由不支持 effect handler 分发。
    UnsupportedEffectDispatch {
        /// 不支持的分发路由。
        route: BackendRoute,
    },
}

/// 校验 backend 输入形状是否已经满足后端边界。
pub fn validate_backend_input(shape: BackendInputShape) -> Result<(), BackendBoundaryError> {
    if shape.contains_open_row_evidence {
        return Err(BackendBoundaryError::OpenRowEvidence);
    }
    if shape.contains_unresolved_nominal_checks {
        return Err(BackendBoundaryError::UnresolvedNominalCheck);
    }
    Ok(())
}

/// 校验指定路由是否支持当前分发形态。
pub fn validate_dispatch_for_route(route: BackendRoute, dispatch: BackendDispatchKind) -> Result<(), BackendBoundaryError> {
    match (route, dispatch) {
        (_, BackendDispatchKind::Static) => Ok(()),
        (BackendRoute::WitnessCapable | BackendRoute::Full, BackendDispatchKind::Witness) => Ok(()),
        (BackendRoute::Full, BackendDispatchKind::EffectHandler) => Ok(()),
        (_, BackendDispatchKind::Witness) => Err(BackendBoundaryError::UnsupportedTraitDispatch { route }),
        (_, BackendDispatchKind::EffectHandler) => Err(BackendBoundaryError::UnsupportedEffectDispatch { route }),
    }
}

/// 从片段能力标签推断 backend 分发形态。
pub fn infer_dispatch_kind(submission: &FragmentSubmission) -> BackendDispatchKind {
    if submission.required_capabilities.iter().any(|cap| cap.as_str().contains("effect-handler")) {
        return BackendDispatchKind::EffectHandler;
    }
    if !submission.witness_calls.is_empty()
        || submission.required_capabilities.iter().any(|cap| matches!(cap.as_str(), "trait-witness" | "open-witness" | "witness-dispatch"))
    {
        return BackendDispatchKind::Witness;
    }
    BackendDispatchKind::Static
}

fn submission_has_resolved_witness(submission: &FragmentSubmission) -> bool {
    submission
        .control_flow
        .as_ref()
        .is_some_and(|payload| payload.functions.iter().flat_map(|function| &function.states).any(|state| !state.witness_bindings.is_empty()))
        || !submission.witness_calls.is_empty()
        || !submission.witness_tables.is_empty()
}

/// Validate suspend fragment shape matches the target lane consumption model.
fn validate_suspend_submission(
    submission: &FragmentSubmission,
    lane: TargetLane,
    clr_strategy: ClrSuspendStrategy,
    vm_strategy: VmSuspendStrategy,
) -> Result<()> {
    for capability in &submission.required_capabilities {
        let tag = capability.as_str();
        if tag.contains("open-witness") || tag.contains("effect-handler") {
            return Err(miette!("CLR lane 拒绝未静态化的开放 witness/effect 能力 `{tag}`；请在 MIR 阶段完成静态化"));
        }
        if tag.contains("trait-witness")
            && submission.witness_tables.is_empty()
            && submission.witness_calls.is_empty()
            && !submission_has_resolved_witness(submission)
        {
            return Err(miette!("CLR lane 拒绝未静态化的开放 witness/effect 能力 `{tag}`；请在 MIR 阶段完成静态化"));
        }
    }

    let model = suspend_consumption_model_for_lane(lane, clr_strategy, vm_strategy);
    match model {
        SuspendConsumptionModel::FirstClass => {
            if submission.control_flow.is_some() {
                return Err(miette!(
                    "first-class suspend lane `{lane:?}` 拒绝 state-machine `control_flow` 载荷（片段 `{fragment}`）；请提交 `suspend_runtime`",
                    fragment = submission.fragment_id,
                    lane = lane
                ));
            }
        }
        SuspendConsumptionModel::StateMachine => {
            if submission.suspend_runtime.is_some() {
                return Err(miette!(
                    "state-machine lane `{lane:?}` 拒绝 first-class `suspend_runtime` 载荷（片段 `{fragment}`）；请提交 `control_flow`",
                    fragment = submission.fragment_id,
                    lane = lane
                ));
            }
        }
    }
    Ok(())
}

/// 驱动层可消费的已规划分区视图。
pub trait PlannedArtifactPartitionsView {
    /// 返回主分区名。
    fn primary_partition_name(&self) -> Option<String>;

    /// 返回分区数量。
    fn partition_count(&self) -> usize;

    /// 返回指定分区。
    fn partition(&self, partition_index: usize) -> Option<&ArtifactPartition>;

    /// 返回指定分区已经完成规划的后端需求。
    fn backend_requirement(&self, partition_index: usize) -> Option<PartitionBackendRequirement>;
}

/// 前端提交给驱动层的最小构建 bundle 协议。
pub trait FrontendBuildBundle {
    /// 返回驱动层可消费的分区计划视图。
    fn planned_partitions(&self) -> &dyn PlannedArtifactPartitionsView;

    /// 为指定分区提交驱动层可消费的目标输入。
    fn submit_backend_input_for_partition(
        &self,
        partition_index: usize,
        backend_family: TargetBackendFamily,
        host_boundary: HostProjectionBoundary,
        output_dir: &Path,
        lane: TargetLane,
    ) -> Result<LoweredBackendInput>;
}

/// 前端提交给驱动层的语义片段。
#[derive(Clone)]
pub struct FragmentSubmission {
    /// 逻辑模块名。
    pub module_name: String,
    /// 当前语义片段标识。
    pub fragment_id: Identifier,
    /// 当前片段导出的稳定操作。
    pub exported_operations: Vec<QualifiedName>,
    /// 当前片段要求的能力约束。
    pub required_capabilities: Vec<CapabilityTag>,
    /// 当前片段携带的理论 bundle。
    pub theory_bundle: TheoryBundle,
    /// 当前片段的可解释入口。
    pub entry_operation: Option<QualifiedName>,
    /// 显式 `[export]` 的稳定操作 → wasm 公开导出名。
    pub wasm_export_names: std::collections::BTreeMap<QualifiedName, String>,
    /// 当前片段内稳定操作到外部导入链接的映射。
    pub external_import_links: BTreeMap<QualifiedName, ExternalImportLink>,
    /// 当前片段内已经解析好的外部调用边。
    pub external_call_edges: Vec<ExternalCallEdge>,
    /// 当前片段内已经解析好的内部调用边。
    pub internal_call_edges: Vec<InternalCallEdge>,
    /// 仅返回字符串字面量的稳定操作。
    pub operation_literal_returns: std::collections::BTreeMap<QualifiedName, String>,
    /// 返回 `unit` 的稳定操作。
    pub operation_void_returns: std::collections::BTreeSet<QualifiedName>,
    /// 具名 trait 见证表载荷。
    pub witness_tables: Vec<WitnessSubmission>,
    /// 入口 witness 动态调用边。
    pub witness_calls: Vec<WitnessCallEdge>,
    /// suspend 分区携带的控制流 rewrite 载荷（state-machine 后端）。
    pub control_flow: Option<nyar::ControlFlowPayload>,
    /// suspend 分区携带的 first-class runtime 载荷（nyar-vm / 原生 continuation 后端）。
    pub suspend_runtime: Option<SuspendRuntimePayload>,
    /// 值/引用聚合体的内存布局计划。
    ///
    /// `singleton` 也在这里以普通引用聚合体的形态出现，负责提供字段偏移、字段类型、
    /// 方法签名和接收者布局等结构事实；后端不得为 `singleton` 单独拼接这些规则。
    pub aggregate_layouts: AggregateLayoutPlan,
    /// Sum type discriminant layouts。
    pub sum_types: Vec<SumTypeLayout>,
    /// Flags bitmask layouts。
    pub flags_types: Vec<FlagsLayout>,
    /// Executable query provider for backends.
    ///
    /// Transitional: currently backed by `mir_functions` until full query payload cutover.
    pub executable: Option<Arc<dyn ExecutableProvider>>,
    /// Singleton 全局实例初始化计划。
    ///
    /// 这里只回答唯一实例的固定符号名、访问器名与 eager/lazy 初始化模式；
    /// 具体字段/方法形状仍必须回到 `aggregate_layouts` 查询。
    pub singleton_instances: Vec<SingletonInstancePlan>,
}

impl std::fmt::Debug for FragmentSubmission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FragmentSubmission")
            .field("module_name", &self.module_name)
            .field("fragment_id", &self.fragment_id)
            .field("exported_operations", &self.exported_operations)
            .field("required_capabilities", &self.required_capabilities)
            .field("entry_operation", &self.entry_operation)
            .finish()
    }
}

impl Default for FragmentSubmission {
    fn default() -> Self {
        Self {
            module_name: String::new(),
            fragment_id: Identifier::new("functions"),
            exported_operations: Vec::new(),
            required_capabilities: Vec::new(),
            theory_bundle: TheoryBundle::default(),
            entry_operation: None,
            wasm_export_names: BTreeMap::new(),
            external_import_links: BTreeMap::new(),
            external_call_edges: Vec::new(),
            internal_call_edges: Vec::new(),
            operation_literal_returns: BTreeMap::new(),
            operation_void_returns: BTreeSet::new(),
            witness_tables: Vec::new(),
            witness_calls: Vec::new(),
            control_flow: None,
            suspend_runtime: None,
            aggregate_layouts: AggregateLayoutPlan::default(),
            sum_types: Vec::new(),
            flags_types: Vec::new(),
            executable: None,
            singleton_instances: Vec::new(),
        }
    }
}

/// First-class suspend backend input (nyar-vm / native continuation runtime).
#[derive(Debug, Clone)]
pub struct NyarVmBackendInput {
    /// Suspend runtime continuation artifacts (semantic MIR boundary).
    pub suspend_runtime: Option<SuspendRuntimePayload>,
    /// State-machine suspend artifacts when `VmSuspendStrategy::StateMachine`.
    pub control_flow: Option<nyar::ControlFlowPayload>,
    /// Optional Nyar module payload to emit as `.nyar`.
    pub nyar_module: Option<std_data::binary::nyar_ir::NyarModuleData>,
    /// Output directory.
    pub output_dir: PathBuf,
}

/// 驱动层接收的目标专用输入。
#[derive(Debug, Clone)]
pub(crate) enum DriverBackendInput {
    /// `CLR` 二进制输入。
    Clr(ClrBinaryBackendInput),
    /// `JVM` 二进制输入。
    Jvm(JvmBinaryBackendInput),
    /// `WASM/WASI` 二进制输入。
    Wasm(WasmBinaryBackendInput),
    /// `native` 二进制输入。
    Native(NativeBinaryBackendInput),
    /// `nyar-vm` first-class suspend 输入。
    NyarVm(NyarVmBackendInput),
}

/// 前端已经完成的分区级 backend 输入。
#[derive(Debug, Clone)]
pub struct LoweredBackendInput {
    input: DriverBackendInput,
    entry_artifact_name: Option<String>,
}

impl LoweredBackendInput {
    pub(crate) fn new(input: DriverBackendInput) -> Self {
        Self { input, entry_artifact_name: None }
    }

    /// 直接提交 `CLR` backend 输入。
    pub fn clr(input: ClrBinaryBackendInput) -> Self {
        Self::new(DriverBackendInput::Clr(input))
    }

    /// 直接提交 `JVM` backend 输入。
    pub fn jvm(input: JvmBinaryBackendInput) -> Self {
        Self::new(DriverBackendInput::Jvm(input))
    }

    /// 直接提交 `WASM/WASI` backend 输入。
    pub fn wasm(input: WasmBinaryBackendInput) -> Self {
        Self::new(DriverBackendInput::Wasm(input))
    }

    /// 直接提交 `native` backend 输入。
    pub fn native(input: NativeBinaryBackendInput) -> Self {
        Self::new(DriverBackendInput::Native(input))
    }

    /// 直接提交 `nyar-vm` backend 输入。
    pub fn nyar_vm(input: NyarVmBackendInput) -> Self {
        Self::new(DriverBackendInput::NyarVm(input))
    }

    /// 基于前端片段提交生成驱动层 backend 输入。
    pub fn from_fragment_submission(
        submission: &FragmentSubmission,
        backend_family: TargetBackendFamily,
        host_boundary: HostProjectionBoundary,
        output_dir: &Path,
        lane: TargetLane,
        clr_suspend_strategy: ClrSuspendStrategy,
        vm_suspend_strategy: VmSuspendStrategy,
        host_flavor: &str,
    ) -> Result<Self> {
        validate_backend_input(BackendInputShape::default()).map_err(|error| miette!("backend boundary 输入形状不合法: {error:?}"))?;
        let backend_route = bundled_backend_capability_descriptor(backend_family)
            .map(|descriptor| descriptor.backend_route)
            .unwrap_or(BackendRoute::StaticOnly);
        let dispatch = infer_dispatch_kind(submission);
        validate_dispatch_for_route(backend_route, dispatch).map_err(|error| miette!("backend boundary 分发形态不合法: {error:?}"))?;
        if backend_family == TargetBackendFamily::Clr || submission.control_flow.is_some() || submission.suspend_runtime.is_some() {
            validate_suspend_submission(submission, lane, clr_suspend_strategy, vm_suspend_strategy)?;
        }
        Ok(Self {
            input: lower_fragment_to_driver_input(submission, backend_family, host_boundary, output_dir.to_path_buf(), host_flavor)?,
            entry_artifact_name: submission.entry_operation.as_ref().and_then(entry_artifact_name),
        })
    }

    pub(crate) fn into_driver_backend_input(self) -> DriverBackendInput {
        self.input
    }

    pub(crate) fn entry_artifact_name(&self) -> Option<&str> {
        self.entry_artifact_name.as_deref()
    }
}

/// 驱动层编译请求。
#[derive(Debug)]
pub(crate) struct DriverCompileRequest<'a> {
    /// 逻辑产物名。
    pub artifact_name: &'a str,
    /// 已经完成规划的后端需求。
    pub requirement: PartitionBackendRequirement,
    /// 目标专用输入。
    pub input: DriverBackendInput,
    /// 是否生成 runtime config。
    pub generate_runtime_config: bool,
    /// 通用编译选项。
    pub options: &'a CompilationOptions,
}

/// 驱动层运行契约。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverRunContract {
    /// 逻辑入口名。
    pub logical_entry: String,
    /// 物理入口文件。
    pub physical_entry: String,
    /// 调用命令。
    pub invocation: String,
    /// 校验命令。
    pub validate: String,
}

/// 驱动层编译结果。
#[derive(Debug, Default)]
pub struct DriverCompileReport {
    /// 产物集合。
    pub artifacts: ArtifactSet,
    /// 入口符号。
    pub entry_symbol: Option<String>,
    /// 运行契约列表。
    pub run_contracts: Vec<DriverRunContract>,
}

struct DriverPartitionCompileRequest<'a> {
    bundle: &'a dyn FrontendBuildBundle,
    planned_partitions: &'a dyn PlannedArtifactPartitionsView,
    output_dir: &'a Path,
    project_name: &'a str,
    emit_msil_sidecar: bool,
    emit_wat_sidecar: bool,
    generate_runtime_config: bool,
}

impl<'a> DriverPartitionCompileRequest<'a> {
    pub fn from_frontend_bundle(
        bundle: &'a dyn FrontendBuildBundle,
        output_dir: &'a Path,
        project_name: &'a str,
        emit_msil_sidecar: bool,
        emit_wat_sidecar: bool,
        generate_runtime_config: bool,
    ) -> Self {
        Self {
            bundle,
            planned_partitions: bundle.planned_partitions(),
            output_dir,
            project_name,
            emit_msil_sidecar,
            emit_wat_sidecar,
            generate_runtime_config,
        }
    }
}

/// 使用 bundled backend 执行目标编译。
pub(crate) fn compile_with_bundled_backends(request: DriverCompileRequest<'_>) -> Result<DriverCompileReport> {
    driver::compile(request)
}

/// 使用 bundled backend 直接编译前端构建 bundle。
pub fn compile_frontend_bundle_with_bundled_backends(
    bundle: &dyn FrontendBuildBundle,
    output_dir: &Path,
    project_name: &str,
    emit_msil_sidecar: bool,
    emit_wat_sidecar: bool,
    generate_runtime_config: bool,
) -> Result<DriverCompileReport> {
    compile_partitions_with_bundled_backends(DriverPartitionCompileRequest::from_frontend_bundle(
        bundle,
        output_dir,
        project_name,
        emit_msil_sidecar,
        emit_wat_sidecar,
        generate_runtime_config,
    ))
}

fn compile_partitions_with_bundled_backends(request: DriverPartitionCompileRequest<'_>) -> Result<DriverCompileReport> {
    let primary_partition_name = request.planned_partitions.primary_partition_name();
    let partition_count = request.planned_partitions.partition_count();
    let mut reports = Vec::new();

    for partition_index in 0..partition_count {
        let partition_started_at = std::time::Instant::now();
        let partition =
            request.planned_partitions.partition(partition_index).ok_or_else(|| miette!("分区索引 `{partition_index}` 超出范围"))?;
        eprintln!(
            "[seed-debug] compiling partition {}/{} name={} backend={:?}",
            partition_index + 1,
            partition_count,
            partition.name,
            backend_family_for_partition(partition),
        );
        let lowering_started_at = std::time::Instant::now();
        let lowered_input = request.bundle.submit_backend_input_for_partition(
            partition_index,
            backend_family_for_partition(partition),
            partition.host_boundary,
            request.output_dir,
            partition.lane,
        )?;
        eprintln!(
            "[seed-debug] backend input lowered {}/{} elapsed_ms={}",
            partition_index + 1,
            partition_count,
            lowering_started_at.elapsed().as_millis(),
        );
        let artifact_base_name = lowered_input.entry_artifact_name().unwrap_or(request.project_name);
        let artifact_name = partition_artifact_name(artifact_base_name, partition, partition_count);
        let options = CompilationOptions {
            target: partition.binary_target.clone(),
            artifact_name: artifact_name.clone(),
            emit_debug_symbols: false,
            optimize: false,
        };
        let driver_input = lowered_input.into_driver_backend_input();
        eprintln!("[seed-debug] backend input ready {}/{} artifact={}", partition_index + 1, partition_count, artifact_name);

        if request.emit_msil_sidecar {
            let _ = write_clr_msil_sidecar(request.output_dir, &artifact_name, &driver_input);
        }
        if request.emit_wat_sidecar {
            let _ = write_wasm_wat_sidecar(request.output_dir, &artifact_name, &driver_input);
        }

        let requirement =
            request.planned_partitions.backend_requirement(partition_index).ok_or_else(|| miette!("分区 `{}` 缺少后端需求", partition.name))?;
        let backend_started_at = std::time::Instant::now();
        let report = compile_with_bundled_backends(DriverCompileRequest {
            artifact_name: &artifact_name,
            requirement,
            input: driver_input,
            generate_runtime_config: request.generate_runtime_config,
            options: &options,
        })?;
        eprintln!(
            "[seed-debug] backend emitted {}/{} elapsed_ms={} total_ms={}",
            partition_index + 1,
            partition_count,
            backend_started_at.elapsed().as_millis(),
            partition_started_at.elapsed().as_millis(),
        );
        eprintln!("[seed-debug] completed partition {}/{} name={}", partition_index + 1, partition_count, partition.name,);
        reports.push((partition.name.clone(), report));
    }

    Ok(merge_partition_reports(reports, primary_partition_name.as_deref()))
}

fn entry_artifact_name(entry_operation: &QualifiedName) -> Option<String> {
    let name = entry_operation.parts().last()?.as_str();
    let sanitized = name.chars().map(|ch| if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' { ch } else { '_' }).collect::<String>();
    (!sanitized.is_empty()).then_some(sanitized)
}
