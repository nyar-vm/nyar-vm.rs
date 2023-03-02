//! LegacyCall 不得驱动 representation planning。
//!
//! 临时 EvidenceId→CallLayout 表已**删除**。真正规划器等待
//! `Invoke` + `ItemInstance` 与稀疏 `invoke_lowerings`。

use crate::{FragmentSubmission, RepresentationPlan};

/// 恒为空 — 不得从 LegacyCall 字段盖章 Call 点布局。
pub(crate) fn provisional_representation_plan(_submission: &FragmentSubmission) -> RepresentationPlan {
    RepresentationPlan::default()
}

/// Ensure `submission.representation_plan` is the empty placeholder (idempotent).
pub(crate) fn ensure_provisional_representation_plan(submission: &mut FragmentSubmission) {
}
