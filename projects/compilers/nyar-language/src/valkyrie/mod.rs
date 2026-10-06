#![doc = include_str!("readme.md")]
#![warn(missing_docs)]

pub mod assembly;
pub(crate) mod backend_contract;
pub mod formatter;
pub mod optimizer;
pub mod printer;
pub mod text;
/// 单向分析 / 处理编译流。
pub mod compile_pipeline;
pub mod control_flow;
pub(crate) mod cst_format;
pub mod derive;
/// Oak 前端：唯一文本事实源与 AST 类型出口。
pub mod frontend;
pub mod frontend_contract;
pub mod highlight;
pub mod hir;
pub mod meta_reactive;
pub mod mir;
pub mod module;
/// Valkyrie 文本解析入口（委托 `frontend`）。
pub mod parser;
pub(crate) mod source_format;
pub(crate) mod symbols;
pub mod type_checker;
pub mod types;
/// Typing helpers such as linearization and semantic inheritance analysis.
pub mod typing;
/// 跨 HIR 与 Semantic MIR 的编译器一致性校验入口。
pub mod validation;

pub(crate) use assembly::{AssembledFragment, assemble_fragment, build_output_surface_counts, plan_artifacts_from_compiled_program};
pub use frontend_contract::{
    ConcretizeError, concretize_mir_function_types, concretize_mir_function_types_lossy, concretize_type, concretize_type_lossy,
};
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
