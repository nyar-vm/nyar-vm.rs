//! Wasm GC 绑定层：把 `legion` / `asgard` 编译为 `legion.wasm` / `asgard.wasm` 等产物，
//! 由 `node scripts/build.mjs assemble` 写入 `packages/vcc-unknown-wasm32`。

#![deny(missing_docs)]

pub use asgard;
pub use legion;

/// Wasm 宿主占位：实际 lowering 由 `legion build --target node` 管线产出。
pub fn wasm_placeholder() -> &'static str {
    "vcc-wasm"
}
