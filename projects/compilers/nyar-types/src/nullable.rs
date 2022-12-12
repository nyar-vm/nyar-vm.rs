//! Nullable intrinsic profiles shared by language assembly and driver lowering.

use crate::QualifiedName;

/// Nullable intrinsic kind referenced by backend lowering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FragmentNullableIntrinsicKind {
    /// `is_null` probe.
    IsNull,
    /// `unwrap_null` extraction.
    UnwrapNull,
}

/// Nullable intrinsic use site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FragmentNullableIntrinsicUse {
    /// Calling operation.
    pub caller: QualifiedName,
    /// Intrinsic kind.
    pub kind: FragmentNullableIntrinsicKind,
}

/// Known `try?` call with a literal bool argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FragmentNullableTryCall {
    /// Calling operation.
    pub caller: QualifiedName,
    /// Callee operation.
    pub callee: QualifiedName,
    /// Whether the bool argument is `true`.
    pub callee_arg_is_true: bool,
}

/// Bool-gated nullable helper profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FragmentNullableBoolProfile {
    /// Helper function.
    pub function: QualifiedName,
    /// Value produced on the `true` path.
    pub true_value: i64,
}
