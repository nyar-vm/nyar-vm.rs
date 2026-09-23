//! Wasm GC 绑定层：把 Nyar 运行时编译为 `nyar.wasm` 等产物，
//! 由构建管线写入 `packages/nyar-wasm32-wasi`。

#![warn(missing_docs)]

pub use nyar_runner;

/// Wasm 宿主占位：实际 lowering 由 Nyar 编译管线产出。
pub fn wasm_placeholder() -> &'static str {
    "nyar-wasm"
}
