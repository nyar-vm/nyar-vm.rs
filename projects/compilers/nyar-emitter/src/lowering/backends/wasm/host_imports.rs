//! Collect declared host imports / string-output edges for wasm shells.
use crate::FragmentSubmission;
use nyar::{ExternalCallArgument, ExternalCallEdge, QualifiedName};
use std_data::binary::wasm::{WasmOpcode, encode_i32_const};

use super::super::interop::{wasi_host_import_target, wasm_host_import_target};

pub(super) fn wasm_impl_return_offset_body(text_offset: u32) -> Vec<u8> {
    let mut body = vec![0];
    encode_i32_const(i32::try_from(text_offset).unwrap(), &mut body);
    WasmOpcode::End.encode(&mut body);
    body
}

pub(super) fn collect_string_output_for_wasm_host(submission: &FragmentSubmission) -> Vec<u8> {
    collect_string_output(submission, |link| wasm_host_import_target(link).is_some())
}

fn collect_string_output(submission: &FragmentSubmission, matches_host_import: impl Fn(&nyar::ExternalImportLink) -> bool) -> Vec<u8> {
    let mut output = Vec::new();
    for operation in submission.entry_operation.iter().chain(submission.exported_operations.iter()) {
        for edge in outgoing_external_call_edges(operation, &submission.external_call_edges) {
            let Some(link) = submission.external_import_links.get(&edge.callee_symbol)
            else {
                continue;
            };
            if !matches_host_import(link) {
                continue;
            }
            let Some(value) = edge.arguments.iter().find_map(string_literal_argument)
            else {
                continue;
            };
            output.extend_from_slice(value.as_bytes());
        }
        if !output.is_empty() {
            break;
        }
    }
    output
}

pub(super) fn first_wasm_host_import_target(submission: &FragmentSubmission) -> Option<(String, String)> {
    first_host_link_for_output(submission, wasm_host_import_target).and_then(|link| {
        let target = wasm_host_import_target(link)?;
        Some((target.module.to_string(), target.field.to_string()))
    })
}

pub(super) fn declared_wasi_host_import_targets(submission: &FragmentSubmission) -> Vec<(String, String)> {
    declared_wasi_host_import_targets_for(submission, crate::nyar_backend_wasi::WasiPreview::Preview2)
}

pub(super) fn declared_wasi_host_import_targets_for(
    submission: &FragmentSubmission,
    preview: crate::nyar_backend_wasi::WasiPreview,
) -> Vec<(String, String)> {
    let mut imports = Vec::new();
    for link in submission.external_import_links.values() {
        let Some(target) = wasi_host_import_target(link)
        else {
            continue;
        };
        let Some((module, field)) = crate::nyar_backend_wasi::wasi_adapt_import_for_preview(target.module, target.function, preview)
        else {
            continue;
        };
        let import_target = (crate::nyar_backend_wasi::wasi_versioned_import_module_for(&module, preview), field);
        if !imports.contains(&import_target) {
            imports.push(import_target);
        }
    }
    imports
}

fn first_host_link_for_output<'a, T>(
    submission: &'a FragmentSubmission,
    matches_host_import: impl Fn(&'a nyar::ExternalImportLink) -> Option<T>,
) -> Option<&'a nyar::ExternalImportLink> {
    for operation in submission.entry_operation.iter().chain(submission.exported_operations.iter()) {
        for edge in outgoing_external_call_edges(operation, &submission.external_call_edges) {
            let Some(link) = submission.external_import_links.get(&edge.callee_symbol)
            else {
                continue;
            };
            if matches_host_import(link).is_some() {
                return Some(link);
            }
        }
    }
    submission.external_import_links.values().find(|link| matches_host_import(link).is_some())
}

fn string_literal_argument(argument: &ExternalCallArgument) -> Option<String> {
    match argument {
        ExternalCallArgument::StringLiteral(value) => Some(value.clone()),
    }
}

fn outgoing_external_call_edges<'a>(operation: &QualifiedName, edges: &'a [ExternalCallEdge]) -> Vec<&'a ExternalCallEdge> {
    edges.iter().filter(|edge| &edge.caller == operation).collect()
}
