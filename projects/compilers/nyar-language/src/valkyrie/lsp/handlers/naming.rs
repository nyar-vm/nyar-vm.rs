//! Naming diagnostics helpers for LSP.

use std::ops::Range;

use std_data::text::awsl::{AbiIssue, AbiIssueKind, AbiSeverity};
use std_data::text::valkyrie::naming::{NamingViolation, DIAG_ABI_BINDING_NOT_SNAKE_CASE};

use crate::types::{SourceID, SourceSpan, ValkyrieError};

/// Convert a semantic naming violation into a compiler diagnostic.
pub fn violation_to_diagnostic(violation: NamingViolation, source: SourceID, offset: usize) -> ValkyrieError {
    let start = offset + violation.name_span.start;
    let end = offset + violation.name_span.end;
    ValkyrieError::naming_violation(
        violation.code,
        &violation.name,
        SourceSpan::new(source, start as u32, end as u32),
    )
}

/// Convert AWSL ABI issues, mapping naming-related kinds to dedicated codes.
pub fn abi_issue_to_diagnostic(issue: &AbiIssue, source: SourceID, offset: usize) -> ValkyrieError {
    if issue.kind == AbiIssueKind::NotSnakeCase {
        let name = extract_name_from_abi_message(&issue.message).unwrap_or_else(|| issue.message.clone());
        let span = issue.span.as_ref().map(|span| {
            let start = offset + span.start;
            let end = offset + span.end;
            SourceSpan::new(source, start as u32, end as u32)
        }).unwrap_or_else(|| SourceSpan::new(source, 0, 0));
        let mut diag = ValkyrieError::naming_violation(DIAG_ABI_BINDING_NOT_SNAKE_CASE, &name, span);
        if let Some(label) = diag.labels.first_mut() {
            label.key = Some("AWSL ABI lint".into());
        }
        return diag;
    }
    generic_abi_issue_to_diagnostic(issue, source, offset)
}

fn generic_abi_issue_to_diagnostic(issue: &AbiIssue, source: SourceID, offset: usize) -> ValkyrieError {
    let mut diag = ValkyrieError::parse_error(issue.message.clone());
    if let Some(span) = &issue.span {
        let start = offset + span.start;
        let end = offset + span.end;
        diag.labels.push(crate::types::LabeledSpan {
            primary: true,
            span: SourceSpan::new(source, start as u32, end as u32),
            key: Some("AWSL ABI".into()),
            data: Vec::new(),
        });
    }
    else {
        let _ = source;
    }
    if issue.severity == AbiSeverity::Warning {
        diag.level = crate::types::ReportKind::Warning;
    }
    diag
}

fn extract_name_from_abi_message(message: &str) -> Option<String> {
    if let Some(rest) = message.strip_prefix("Name '") {
        if let Some(name) = rest.split('\'').next() {
            return Some(name.to_string());
        }
    }
    for marker in ["`:", "`@", "name `", "parameter `", "ABI name `"] {
        if let Some(start) = message.find(marker) {
            let rest = &message[start + marker.len()..];
            if let Some(name) = rest.split('`').next() {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// Collect naming diagnostics from parsed vx/v source.
pub fn collect_naming_diagnostics(
    source_text: &str,
    is_vx: bool,
    source: SourceID,
    offset: usize,
) -> Vec<ValkyrieError> {
    use std_data::text::valkyrie::naming::validate_snake_case;
    use std_data::text::valkyrie::parser::AstParser;

    let parsed = if is_vx {
        AstParser::parse_vx_root(source_text)
    }
    else {
        AstParser::parse_root(source_text)
    };
    let Ok(root) = parsed else {
        return Vec::new();
    };
    validate_snake_case(&root)
        .into_iter()
        .map(|violation| violation_to_diagnostic(violation, source, offset))
        .collect()
}

/// Shift all label spans in a diagnostic by `offset`.
pub fn shift_diagnostic_spans(diag: &mut ValkyrieError, offset: usize) {
    for label in &mut diag.labels {
        let start = label.span.get_start() as usize + offset;
        let end = label.span.get_end() as usize + offset;
        label.span = SourceSpan::new(label.span.source, start as u32, end as u32);
    }
}

/// Map a diagnostic whose spans refer to synthetic widget source back to the host file.
pub fn map_synthetic_diagnostic_to_file(
    script_start: usize,
    prefix_len: usize,
    mut diag: ValkyrieError,
) -> ValkyrieError {
    for label in &mut diag.labels {
        let start = label.span.get_start() as usize;
        let end = label.span.get_end() as usize;
        let mapped = map_span_to_host(script_start, prefix_len, start..end);
        label.span = SourceSpan::new(label.span.source, mapped.start as u32, mapped.end as u32);
    }
    diag
}

fn map_span_to_host(script_start: usize, prefix_len: usize, span: Range<usize>) -> Range<usize> {
    super::awsl::map_synthetic_span_to_file(script_start, prefix_len, span)
}
