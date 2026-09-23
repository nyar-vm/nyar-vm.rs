#![doc = include_str!("readme.md")]
#![warn(missing_docs)]

pub mod abstractions;
pub mod assembled_fragment;
pub mod backends;
pub mod lanes;
pub mod packaging;
pub mod planning;
pub mod selection;
pub mod target_profile;
pub mod testing;

pub use self::{
    abstractions::{
        ArtifactFormat, ArtifactKind, BackendInputKind, BinaryArch, BinaryFlavor, BinaryTarget, ByteOrder, CanonicalAbi, CanonicalArch,
        CanonicalSpecification, CanonicalTarget, CanonicalTargetParseError, CanonicalVendor, ObjectKind, TargetFamily,
    },
    assembled_fragment::AssembledFragment,
    backends::{
        BackendCapability, BackendDescriptor, BackendInterpreterRegistration, BackendInterpreterSelection, BackendRegistry, CompilationOptions,
        TargetCodeGenBackend,
        clr::{ClrImageKind, ClrSuspendStrategy, FrontendBuildContext},
        projection_family_for_backend, projection_policy_for_target_profile,
        vm::VmSuspendStrategy,
    },
    lanes::{LaneLoweringResult, TargetLoweringLane, TargetLoweringLaneDescriptor},
    packaging::{ArtifactDescriptor, ArtifactSet, OutputSpec, TargetLane},
    planning::{
        ArtifactPartition, ArtifactPartitionPlan, ControlFlowPayload, FragmentOptimizationView, PartitionBackendRequirement, PlanningError,
        PlanningInput, SemanticFragment, SuspendConsumptionModel, SuspendContinuationArtifact, SuspendDispatchCase, SuspendFunctionArtifact,
        SuspendRuntimeFunctionArtifact, SuspendRuntimePayload, SuspendStateArtifact, SuspendWitnessBinding, builtin_graphic_manifest,
        builtin_neural_manifest, suspend_consumption_model, suspend_consumption_model_for_lane,
    },
    selection::{BackendCandidate, BackendSelector},
    target_profile::{
        ArtifactPolicy, EntryPolicy, PublishFormat, RunnerFamily, RunnerSelector, TargetBackendFamily, TargetHostKind, TargetMode,
        TargetProfile, WrapStrategy,
    },
    testing::{
        LegendFixtureEntry, LegendFixtureManifest, RuntimeFixtureResult, RuntimeFixtureSpec, assert_or_regenerate_text_sidecar,
        assert_or_regenerate_yaml_sidecar, collect_fixture_cases_with_extensions, load_legend_fixture_manifest, load_optional_text_sidecar,
        load_optional_yaml_sidecar, load_runtime_fixture_spec, regenerate_enabled, resolve_legend_fixture_paths,
        resolve_legend_fixture_targets, resolve_runtime_fixture_targets, text_sidecar_path, verify_legend_fixture_case,
        verify_runtime_fixture_case, verify_runtime_fixture_spec, yaml_sidecar_path,
    },
};
pub use nyar_analyzer::{EntryContract, ExportContract, FunctionAnalysis, ImportContract, ProgramFacts, RuntimeRequirement};
pub use nyar_optimizer::{
    AlgebraicTerm, EGraphSnapshot, FutamuraProjectionFamily, HostProjectionBoundary, ObjectAlgebraicBuilder, ObjectAlgebraicDimension,
    ObjectAlgebraicInterpreter, ObjectAlgebraicProgram, OptimizationRequest, OptimizationResult, OptimizationSession, ProjectionPlan,
    ProjectionPolicy, ReferenceManagement, RewriteEquation, RewritePhase, RewriteRule, RewriteTheory, TermRewrite, TheoryBundle,
};
pub use nyar_types::{
    AggregateLayoutPlan, CapabilityTag, ExecutableFunction, ExternalCallArgument, ExternalCallEdge, ExternalImportLink, FlagsLayout,
    FragmentNullableBoolProfile, FragmentNullableIntrinsicKind, FragmentNullableIntrinsicUse, FragmentNullableTryCall, Identifier,
    InternalCallEdge, NamePath, NyarFunctionType, NyarType, QualifiedName, SingletonInstancePlan, SumTypeLayout, SymbolIdentity,
    WitnessCallEdge, WitnessMethodSlotSubmission, WitnessObject, WitnessSubmission,
};
