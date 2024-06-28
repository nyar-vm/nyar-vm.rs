//! 编译阶段定义与结构化诊断合同。
//!
//! 失败侧使用一族结构化诊断类型，而不是名叫 `StructuredDiagnostics` 的单一结构体。

/// One structured diagnostic record (minimum contract fields).
///
/// Concrete compile stages may wrap or extend this; the category is plural.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticRecord {
    /// Stable machine code (e.g. `SMIR006`).
    pub code: String,
    /// Severity label (`error` / `warning` / …).
    pub severity: String,
    /// Pipeline stage that produced the diagnostic.
    pub stage: CompileStage,
    /// Owning module / package symbol when known.
    pub module: String,
    /// Human message (not used for semantic decisions).
    pub message: String,
    /// Deterministic sort key.
    pub stable_sort_key: String,
}

/// A non-empty structured diagnostics payload (category, not a singleton type name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuredDiagnosticSet {
    /// Ordered diagnostic records.
    pub records: Vec<DiagnosticRecord>,
}

impl StructuredDiagnosticSet {
    /// Construct from one or more records. Empty sets are not allowed for `Err`.
    pub fn from_records(records: Vec<DiagnosticRecord>) -> Option<Self> {
        if records.is_empty() { None } else { Some(Self { records }) }
    }
}

/// Stages of the one-way compile / analysis / processing stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CompileStage {
    /// Package AST.
    Ast,
    /// HIR elaboration / overload / evidence solving (may use work types).
    Hir,
    /// Package semantic MIR + SPI (M1).
    SemanticMir,
    /// Cross-package link with selected std adaptors.
    LinkTime,
    /// Validated Semantic MIR (M2).
    ValidateMir,
    /// Sparse representation / layout planning.
    RepresentationPlan,
    /// Target-private plan.
    TargetExecutionPlan,
    /// Artifact emit.
    Emit,
}

/// Result alias for pipeline stages: success value or structured diagnostics category.
pub type StageResult<T> = Result<T, StructuredDiagnosticSet>;
