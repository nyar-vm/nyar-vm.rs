//! 微信小程序逻辑 WASM prelude：仅 signal host imports，无 DOM / Compose。

/// 小程序信号宿主导入（与 `asgard-runtime.js` 的 `__voa` 配对）。
pub const MP_SIG_PRELUDE: &str = r#"
[js_builtin("__voa.sigCreateI32")]
micro sig_create_i32(v: i32): i32

[js_builtin("__voa.sigGetI32")]
micro sig_get_i32(id: i32): i32

[js_builtin("__voa.sigSetI32")]
micro sig_set_i32(id: i32, v: i32)

[js_builtin("__voa.sigCreateUtf8")]
micro sig_create_utf8(v: utf8): i32

[js_builtin("__voa.sigGetUtf8")]
micro sig_get_utf8(id: i32): utf8

[js_builtin("__voa.sigSetUtf8")]
micro sig_set_utf8(id: i32, v: utf8)

[js_builtin("__voa.sigCreateBool")]
micro sig_create_bool(v: bool): i32

[js_builtin("__voa.sigGetBool")]
micro sig_get_bool(id: i32): bool

[js_builtin("__voa.sigSetBool")]
micro sig_set_bool(id: i32, v: bool)

[js_builtin("__voa.sigCreateList")]
micro sig_create_list(v: list): i32

[js_builtin("__voa.sigGetList")]
micro sig_get_list(id: i32): list

[js_builtin("__voa.sigSetList")]
micro sig_set_list(id: i32, v: list)

[js_builtin("__voa.sigSubscribe")]
micro sig_subscribe(sig_id: i32, deps_csv: utf8, update_export: utf8)

[js_builtin("__voa.rxBatchBegin")]
micro rx_batch_begin()

[js_builtin("__voa.rxBatchEnd")]
micro rx_batch_end()

[js_builtin("__voa.storeBump")]
micro store_bump()

[js_builtin("__voa.storeSubscribe")]
micro store_subscribe(update_export: utf8)

"#;
