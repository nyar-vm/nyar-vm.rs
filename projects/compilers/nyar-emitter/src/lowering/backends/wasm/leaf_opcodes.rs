//! Wasm 叶子操作码（`acorn-wasm` `WasmOpcode` 尚未收录的 MVP 变体）。

/// `i64.extend_i32_s`
pub(crate) const I64_EXTEND_I32_S: u8 = 0xAC;
/// `i64.shr_u`
pub(crate) const I64_SHR_U: u8 = 0x88;
