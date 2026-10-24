//! 移动端/桌面宿主 V 预置：patch 与 event 走原生 FFI，不用 DOM。

/// 移动端与桌面原生宿主 prelude（`js_builtin` → Compose / SwiftUI / WinUI / Linux native provider）。
pub const MOBILE_HOST_PRELUDE: &str = r#"
[js_builtin("asgardComposePatch")]
micro asgard_compose_patch(key: utf8, value: utf8)

[js_builtin("asgardComposeDispatch")]
micro asgard_compose_dispatch(name: utf8)

[js_builtin("asgardComposeMount")]
micro asgard_compose_mount(component_json: utf8)

micro awsl_key_none(): utf8 { return "" }
"#;
