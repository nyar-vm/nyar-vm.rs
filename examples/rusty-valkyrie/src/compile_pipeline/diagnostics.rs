//! Structured diagnostic helpers for pipeline stages.

use nyar_types::{CompileStage, DiagnosticRecord, StageResult, StructuredDiagnosticSet};

/// Build one diagnostic record with a deterministic sort key.
pub fn diagnostic(stage: CompileStage, code: &str, module: &str, message: impl Into<String>) -> DiagnosticRecord {
    let message = message.into();
    DiagnosticRecord {
        code: code.to_string(),
        severity: "error".into(),
        stage,
        module: module.to_string(),
        stable_sort_key: format!("{code}|{module}|{:?}", stage),
        message,
    }
}

/// Fail a stage with a non-empty structured diagnostic set.
pub fn fail_stage<T>(stage: CompileStage, code: &str, module: &str, message: impl Into<String>) -> StageResult<T> {
    let record = diagnostic(stage, code, module, message);
    Err(StructuredDiagnosticSet::from_records(vec![record]).expect("non-empty diagnostics"))
}
