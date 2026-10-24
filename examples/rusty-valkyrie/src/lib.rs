#![doc = include_str!("../readme.md")]
#![deny(missing_docs)]

pub mod assembly;
pub(crate) mod backend_contract;
/// 单向分析 / 处理编译流。
pub mod compile_pipeline;
pub mod control_flow;
pub mod derive;
pub mod frontend;
pub mod frontend_contract;
pub mod highlight;
pub mod hir;
/// 诊断专用入口（HIR 级），与 `compile_pipeline` 产物链分离。
pub mod lint;
pub mod meta_reactive;
pub mod mir;
pub mod module;
/// 运算符成员名的唯一所有者（运算符是普通成员调用，不是语义类别）。
pub(crate) mod operator_members;
pub mod optimizer;
/// Valkyrie 文本解析入口（委托 `frontend`）。
pub mod parser;
/// 入口驱动的语义闭包规划。
pub mod semantic_closure;
/// 可格式化源码面的扩展名与种类（Valkyrie / VON / AWSL）。
pub mod source_surface;
pub mod source_format;
pub(crate) mod symbols;
pub mod text;
pub mod type_checker;
pub mod types;
/// Typing helpers such as linearization and semantic inheritance analysis.
pub mod typing;
/// 跨 HIR 与 Semantic MIR 的编译器一致性校验入口。
pub mod validation;

/// 供外部集成测试引用的内部符号工具（不稳定，勿在正式代码中使用）。
#[doc(hidden)]
pub use backend_contract::interop::function_interop_contract;
/// 供外部集成测试引用的内部符号工具（不稳定，勿在正式代码中使用）。
#[doc(hidden)]
pub use symbols::{stable_hir_function_name_path, stable_hir_function_symbol};

pub(crate) use assembly::{AssembledFragment, assemble_fragment, build_output_surface_counts, plan_artifacts_from_compiled_program};
pub use frontend_contract::{ConcretizeError, concretize_mir_function_types, concretize_type};
pub use hir::{CaptureAnalyzer, function_body_contains_yield, *};
pub use mir::{
    ArrayInitialization, MirBlock, MirBlockRef, MirConstant, MirEffectKind, MirFunction, MirInstruction, MirLowerer, MirModule, MirOperand,
    MirOperation, MirTerminator, MirValue, MirValueOrigin, MirValueRef,
};
pub use nyar::{
    self, ArtifactKind, ArtifactPartitionPlan, ArtifactPolicy, ArtifactSet, CanonicalAbi, CanonicalArch, CanonicalSpecification,
    CanonicalTarget, CanonicalTargetParseError, CanonicalVendor, CompilationOptions, EntryPolicy, HostProjectionBoundary, ProgramFacts,
    PublishFormat, ReferenceManagement, RunnerFamily, RunnerSelector, TargetHostKind, TargetMode, TargetProfile, WrapStrategy,
};
pub use types::{Identifier, NamePath, QualifiedName, SourceID, SourceSpan};
pub use compile_pipeline::{
    CompilerArtifactReport, CompilerBuildContext, CompilerCompileEvidence, CompilerHostProviderBinding, compile_source_groups_to_artifacts,
    generated_source_hash, host_provider_binding_hash_for_bindings, semantic_closure_hash_for_groups,
};
pub use lint::{lint_valkyrie_source, lint_valkyrie_vx_source};
pub use mir::{
    AggregateLayout, AggregateLayoutPlan, FieldLayout, FlagsLayout, LayoutId, LayoutKey, MirDiagnostic, MirStorageKind, SingletonInstancePlan,
    SumTypeLayout, SumVariantLayout, collect_singleton_instance_plans, compute_aggregate_layout_plan, layout_id_for_type,
    storage_kind_for_type,
};
pub use semantic_closure::{
    ClosureGroupInput, DeclarationIndexEntry, DeclarationKey, DeclarationKind, EntryPointKind, GroupSemanticClosure, SemanticClosure,
    SemanticClosureError, plan_semantic_closure,
};
pub use validation::ControlFlowScheduler;
