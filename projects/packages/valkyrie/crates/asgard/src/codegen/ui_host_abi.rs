//! 非 browser `UiHost` 共享约定：`mount` / `patch` / `on_event` → AOT export 命名。

/// `on_event("on_tap")` → AOT export `awsl_call_on_tap`。
pub fn resolve_call_export(handler_name: &str) -> String {
    let name = handler_name.trim();
    if name.starts_with("awsl_call_") { name.to_string() } else { format!("awsl_call_{name}") }
}

/// reactive binding → `awsl_sig_{route}_{name}`（读当前值）。
pub fn resolve_sig_export(route: &str, binding_name: &str) -> String {
    format!("awsl_sig_{}_{}", sanitize_ident(route), sanitize_ident(binding_name))
}

fn sanitize_ident(name: &str) -> String {
    name.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() || ch == '_' { ch } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_ascii_lowercase()
}
#[cfg(test)]
#[path = "../../tests/path_wired/codegen_ui_host_abi.rs"]
mod codegen_ui_host_abi_tests;
