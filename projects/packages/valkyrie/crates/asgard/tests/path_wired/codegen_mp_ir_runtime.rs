//! Migrated from `src/codegen/mp_ir_runtime.rs` (`tests`).

use super::*;

#[test]
fn runtime_closed_loop_no_js_handlers() {
    let rt = generate_mp_runtime("mp-test");
    assert!(rt.contains("WASM_MODULE = 'mp-test'"));
    assert!(rt.contains("WXWebAssembly"));
    assert!(rt.contains("sigCreateI32"));
    assert!(rt.contains("syncFromWasm"));
    assert!(rt.contains("sigSubscribe"));
    assert!(rt.contains("storeSubscribe"));
    assert!(rt.contains("parseDeps"));
    assert!(rt.contains("resolveSigRef"));
    assert!(rt.contains("wasmExports"));
    assert!(rt.contains("resolveCallExport"));
    assert!(rt.contains("awsl_call_"));
    assert!(rt.contains("on_event"));
    assert!(rt.contains("findAsgardUiSection"));
    assert!(rt.contains("decodeAsgardUi"));
    assert!(!rt.contains("__ASGARD_PRODUCT__"));
    assert!(!rt.contains("new Uint8Array(["));
    assert!(!rt.contains("on_tap(page)"));
    assert!(!rt.contains("count + 1"));
    let magic_lit = magic_bytes_literal(UI_BIN_MAGIC);
    assert!(rt.contains(&magic_lit));
}
