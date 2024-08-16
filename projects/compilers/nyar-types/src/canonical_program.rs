//! 规范程序成功类型与编译管线阶段。
//!
//! 失败侧使用**结构化诊断**（共享合同的一族诊断类型），
//! 而不是名叫 `StructuredDiagnostics` 的单一结构体。
//!
//! 模块边界：
//!
//! - `diagnostics`：阶段身份与结构化诊断合同。
//! - `linked_program`：链接完成后的语义程序与稳定身份记录。
//! - `canonical_mir`：Canonical Semantic MIR 结构与其合同错误。
//! - `canonical_validation`：M2 验证器，只拒绝不合法 MIR。
//! - `compiled_program`：CanonicalProgram / CompiledProgram 成功载荷。
//! - `pipeline`：单向编译流的阶段 trait 合同。
//!
//! 根模块只声明子模块并重导出公共类型，不承载实现。

mod canonical_mir;
mod canonical_validation;
mod compiled_program;
mod diagnostics;
mod linked_program;

pub mod pipeline;

pub use canonical_mir::{
    CanonicalArrayInitialization, CanonicalBlock, CanonicalBlockId, CanonicalCallee, CanonicalConstant, CanonicalEffectKind, CanonicalFunction,
    CanonicalInstruction, CanonicalMirError, CanonicalOperand, CanonicalOperation, CanonicalSemanticMir, CanonicalTerminator,
};
pub use compiled_program::{CanonicalProgram, CompiledProgram, CompiledProgramError};
pub use diagnostics::{
    CompileResult, CompileStage, DiagnosticArg, DiagnosticOwner, DiagnosticPhase, DiagnosticRecord, DiagnosticRow, DiagnosticSeverity,
    DiagnosticSource, DiagnosticSpan, StageResult, StructuredDiagnosticSet,
};
pub use linked_program::{
    CanonicalCallEdge, CanonicalExternalCallEdge, CanonicalFragment, CanonicalPrimitiveType, CanonicalTypeKind, EntryRecord, EvidenceRecord,
    ExportRecord, FieldRecord, ImportRecord, ItemInstanceRecord, LinkedSemanticProgram, NominalInstanceRecord, NominalValueSemantics,
    TypeRecord, VariantRecord,
};
