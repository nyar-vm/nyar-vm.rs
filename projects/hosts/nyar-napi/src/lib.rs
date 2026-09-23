//! Node-API（N-API）原生绑定层：把 Nyar 运行时能力导出给 Node 宿主。
//!
//! 用户面对的 CLI 在 `packages/nyar`，由各 `packages/nyar-*` 平台 collect 组装而成。

#![warn(missing_docs)]

pub use nyar_runner;

/// Node 宿主占位：后续在此挂 `#[napi]` 导出（run / compile / inspect 等）。
pub fn napi_placeholder() -> &'static str {
    "nyar-napi"
}
