//! AWSL 诊断与 widget 语义接入

use std_data::text::awsl::{
    extract_component_abi_from_script, AbiSeverity, ComponentAbiIndex,
};

use super::document::{parse_awsl, script_offset_range};
use super::naming::{abi_issue_to_diagnostic, collect_naming_diagnostics, map_synthetic_diagnostic_to_file, shift_diagnostic_spans};
use super::vx::parse_awsl_script_vx;
use super::widget::{component_stem_from_uri, synthetic_widget_source, widget_name_from_root};
use crate::state::DocumentState;
use crate::state::ServerState;
use crate::types::{SourceID, ValkyrieError};
use nyar_language::ValkyrieCompiler;

/// 编译 AWSL 文档：解析模板 + 将 script 作为 **vx** widget 体编译并索引
pub fn compile_awsl_document(
    uri: &str,
    text: &str,
    doc_state: &mut DocumentState,
    server: Option<&ServerState>,
) -> Vec<ValkyrieError> {
    let mut diagnostics = Vec::new();
    let source_id = doc_state.file_id.unwrap_or_default();

    if let Some(state) = server {
        state.remove_awsl_abi_entries_for_uri(uri);
    }

    match parse_awsl(text) {
        Ok(root) => {
            doc_state.awsl_root = Some(root.clone());
            let stem = component_stem_from_uri(uri);
            let widget_name = widget_name_from_root(&root, &stem);

            if let Some(script) = &root.script {
                if let Err(e) = parse_awsl_script_vx(script) {
                    diagnostics.push(map_script_diagnostic_to_file(&root, text, e));
                }

                if let Some(script_range) = script_offset_range(&root, text) {
                    diagnostics.extend(collect_naming_diagnostics(script, true, source_id, script_range.start));
                }

                let abi_result = extract_component_abi_from_script(script, &widget_name);
                doc_state.component_abi = Some(abi_result.abi.clone());
                doc_state.abi_issues = abi_result.issues.clone();

                if let Some(state) = server {
                    if let Some(script_range) = script_offset_range(&root, text) {
                        state.update_awsl_abi_entry(uri, &widget_name, abi_result.abi, script_range.start);
                    }
                }

                if let Some(script_range) = script_offset_range(&root, text) {
                    for issue in &abi_result.issues {
                        if !should_emit_abi_issue(issue) {
                            continue;
                        }
                        if issue.kind == std_data::text::awsl::AbiIssueKind::NotSnakeCase {
                            continue;
                        }
                        diagnostics.push(abi_issue_to_diagnostic(issue, source_id, script_range.start));
                    }
                }
            }
            else {
                doc_state.component_abi = None;
                doc_state.abi_issues.clear();
            }

            if let Some((synthetic, prefix_len)) = synthetic_widget_source(&root, &stem) {
                if let Some(script_range) = script_offset_range(&root, text) {
                    doc_state.awsl_script_base = Some(script_range.start);
                    doc_state.awsl_synthetic_prefix = Some(prefix_len);
                }

                let mut widget_compiler = ValkyrieCompiler::new(synthetic);
                match widget_compiler.parse() {
                    Ok(ast) => {
                        doc_state.ast = Some(ast);
                    }
                    Err(e) => {
                        if let (Some(base), Some(prefix)) =
                            (doc_state.awsl_script_base, doc_state.awsl_synthetic_prefix)
                        {
                            diagnostics.push(map_synthetic_diagnostic_to_file(base, prefix, e));
                        }
                        else {
                            diagnostics.push(e);
                        }
                    }
                }
            }
        }
        Err(message) => {
            diagnostics.push(ValkyrieError::parse_error(message));
        }
    }

    if let Some(state) = server {
        let mut index = ComponentAbiIndex::new();
        for entry in state.awsl_abi_index.iter() {
            index.insert(entry.widget_name.clone(), entry.abi.clone());
        }
        if let Some(root) = &doc_state.awsl_root {
            for issue in index.validate_awsl_root(root) {
                if !should_emit_abi_issue(&issue) {
                    continue;
                }
                let mut diag = abi_issue_to_diagnostic(&issue, source_id, 0);
                if let Some(span) = issue.span {
                    if let Some(label) = diag.labels.first_mut() {
                        label.key = Some(if issue.kind == std_data::text::awsl::AbiIssueKind::NotSnakeCase {
                            "AWSL ABI lint".into()
                        } else {
                            "AWSL ABI cross-file".into()
                        });
                        label.span = crate::types::SourceSpan::new(source_id, span.start as u32, span.end as u32);
                    }
                }
                diagnostics.push(diag);
            }
        }
    }

    doc_state.diagnostics = diagnostics.clone();
    diagnostics
}

fn should_emit_abi_issue(issue: &std_data::text::awsl::AbiIssue) -> bool {
    use std_data::text::awsl::AbiIssueKind;
    match issue.kind {
        AbiIssueKind::NotSnakeCase => true,
        _ => issue.severity == AbiSeverity::Error,
    }
}

fn map_script_diagnostic_to_file(
    root: &std_data::text::awsl::AwslRoot,
    text: &str,
    mut diag: ValkyrieError,
) -> ValkyrieError {
    let Some(script_start) = script_offset_range(root, text).map(|r| r.start) else {
        return diag;
    };
    shift_diagnostic_spans(&mut diag, script_start);
    diag
}
