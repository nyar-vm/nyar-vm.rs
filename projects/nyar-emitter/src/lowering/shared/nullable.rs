//! Nullable sentinel ABI shared across backends (Phase 2 MVP).

/// i64 nullable sentinel: `0x8000_0000_0000_0000`.
pub const I64_NULL_SENTINEL: i64 = i64::MIN;

/// Returns true when `value` represents a nullable i64 null.
pub fn i64_is_null(value: i64) -> bool {
    value == I64_NULL_SENTINEL
}

/// Unwraps a nullable i64 payload or returns `None` when null.
pub fn i64_unwrap_null(value: i64) -> Option<i64> {
    if i64_is_null(value) { None } else { Some(value) }
}
