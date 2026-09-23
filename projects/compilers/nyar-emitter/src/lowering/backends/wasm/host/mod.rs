//! Host-boundary shells that are not full MIR+GC modules (string interop / CM packaging).
mod js_glue;
mod wasi_cm;

use crate::{
    FragmentSubmission,
    nyar_backend_wasi::{WasiPreview, WasmBinaryModule},
};

pub(super) fn lower_fragment_to_js_glue_module(submission: &FragmentSubmission) -> (WasmBinaryModule, Vec<(String, String)>) {
    js_glue::lower_fragment_to_js_glue_module(submission)
}

pub(super) fn lower_fragment_to_wasi_cm_module(submission: &FragmentSubmission) -> (WasmBinaryModule, Vec<(String, String)>) {
    wasi_cm::lower_fragment_to_wasi_cm_module(submission)
}

pub(super) fn lower_fragment_to_wasi_cm_module_for(
    submission: &FragmentSubmission,
    preview: WasiPreview,
) -> (WasmBinaryModule, Vec<(String, String)>) {
    wasi_cm::lower_fragment_to_wasi_cm_module_for(submission, preview)
}
