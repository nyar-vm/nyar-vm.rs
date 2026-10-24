//! AWSL 响应式运行时：经 `js_builtin` 绑定到 `boot.js` 中的 `__voa` 信号表。

/// 响应式原语前导（与 `js_boot.rs` 的 `__voa` 实现配对）。
pub const REACTIVE_PRELUDE: &str = r#"
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

[js_builtin("__voa.rxBindTextUtf8")]
micro rx_bind_text_utf8(handle: i32, expr_export: utf8, dep1: i32, dep2: i32)

[js_builtin("__voa.rxBindAttrUtf8")]
micro rx_bind_attr_utf8(handle: i32, attr: utf8, expr_export: utf8, dep1: i32, dep2: i32)

[js_builtin("__voa.rxBindClassUtf8")]
micro rx_bind_class_utf8(handle: i32, expr_export: utf8, dep1: i32, dep2: i32)

[js_builtin("__voa.rxBindIf")]
micro rx_bind_if(container: i32, cond_export: utf8, mount_export: utf8, dep1: i32, dep2: i32)

[js_builtin("__voa.rxBindLoop")]
micro rx_bind_loop(container: i32, items_export: utf8, mount_export: utf8, key_export: utf8, dep1: i32, dep2: i32)

[js_builtin("__voa.rxBindPropI32")]
micro rx_bind_prop_i32(sig_id: i32, expr_export: utf8, dep1: i32, dep2: i32)

[js_builtin("__voa.rxBindPropUtf8")]
micro rx_bind_prop_utf8(sig_id: i32, expr_export: utf8, dep1: i32, dep2: i32)

[js_builtin("__voa.rxBindPropBool")]
micro rx_bind_prop_bool(sig_id: i32, expr_export: utf8, dep1: i32, dep2: i32)

[js_builtin("__voa.rxMemoI32")]
micro rx_memo_i32(memo_id: i32, compute_export: utf8, dep1: i32, dep2: i32): i32

[js_builtin("__voa.rxMemoUtf8")]
micro rx_memo_utf8(memo_id: i32, compute_export: utf8, dep1: i32, dep2: i32): utf8

[js_builtin("__voa.componentEmitUtf8")]
micro component_emit(event: utf8, arg: utf8)

[js_builtin("__voa.componentEmitI32")]
micro component_emit_i32(event: utf8, arg: i32)

[js_builtin("__voa.componentEmit")]
micro component_emit(event: utf8)

[js_builtin("__voa.domAddEventExport")]
micro dom_add_event_export(handle: i32, event: utf8, handler_export: utf8)

[js_builtin("__voa.domAddEventExportUtf8")]
micro dom_add_event_export_utf8(handle: i32, event: utf8, handler_export: utf8, arg: utf8)

[js_builtin("__voa.domAddEventExportI32")]
micro dom_add_event_export_i32(handle: i32, event: utf8, handler_export: utf8, arg: i32)

[js_builtin("__voa.rxBatchBegin")]
micro rx_batch_begin()

[js_builtin("__voa.rxBatchEnd")]
micro rx_batch_end()

[js_builtin("__voa.storeBump")]
micro store_bump()

[js_builtin("__voa.storeSubscribe")]
micro store_subscribe(update_export: utf8)

[js_builtin("__voa.styleCollectorPush")]
micro style_collector_push(classes: utf8)

"#;
