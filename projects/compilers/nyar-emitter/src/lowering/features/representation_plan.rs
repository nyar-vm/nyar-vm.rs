//! ADR 0010: LegacyCall must not drive representation planning.
//!
//! Provisional EvidenceId→CallLayout tables are **deleted**. Real planner awaits
//! `Invoke` + `ItemInstance` and sparse `invoke_lowerings`.

use crate::{FragmentSubmission, RepresentationPlan};

/// Always clear / empty — do not stamp Call-site layouts from LegacyCall fields.
pub(crate) fn provisional_representation_plan(_submission: &FragmentSubmission) -> RepresentationPlan {
    RepresentationPlan::default()
}

/// Ensure `submission.representation_plan` is the empty placeholder (idempotent).
pub(crate) fn ensure_provisional_representation_plan(submission: &mut FragmentSubmission) {
}
