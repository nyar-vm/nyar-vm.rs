//! 合并 AWSL 降级结果为单一 WASM 编译单元。

use std::fmt::Write as _;

use crate::{
    awsl::{LoweredComponent, ScriptBinding, SignalValueType},
    codegen::{
        mobile_prelude::MOBILE_HOST_PRELUDE, mp_prelude::MP_SIG_PRELUDE, reactive_prelude::REACTIVE_PRELUDE,
        terminal_prelude::terminal_prelude, terminal_v_render::render_terminal_component_v, ui_host_abi::resolve_sig_export,
        v_prelude::DOM_PRELUDE, v_render::render_component_v,
    },
};

/// 从所有 AWSL 组件生成可编译的 V 源码（浏览器：DOM + reactive）。
pub fn build_awsl_wasm_source(components: &[LoweredComponent]) -> String {
    build_awsl_source_with_prelude(components, DOM_PRELUDE, REACTIVE_PRELUDE)
}

/// 从所有 AWSL 组件生成移动端/桌面宿主 V 源码（原生 patch/event FFI）。
pub fn build_awsl_host_source(components: &[LoweredComponent]) -> String {
    build_awsl_source_with_prelude(components, MOBILE_HOST_PRELUDE, REACTIVE_PRELUDE)
}

/// 从所有 AWSL 组件生成终端 TUI 宿主 V 源码（纯 Valkyrie widget_* 调用）。
///
/// 终端路径是纯 V 实现：host_contract 原语 + `tui_runtime`（项目依赖闭包中的
/// `std.terminal.tui.v`）+ AWSL 降低的 widget 构建代码合并为单一 `app.v`。
/// 不依赖 DOM / 信号系统，状态由 TuiRuntime 绑定存储管理。
pub fn build_awsl_terminal_source(components: &[LoweredComponent], tui_runtime: &str) -> String {
    let mut out = terminal_prelude(tui_runtime);
    out.push_str("namespace std.terminal;\n\n");
    let mut generated = String::new();
    for component in components {
        generated.push_str(&render_terminal_component_v(component));
        generated.push('\n');
    }
    push_host_callable_micros(&mut out, &generated);
    out
}

/// 小程序逻辑 WASM：signal host + script / `awsl_call_*` / `awsl_sig_*`，无 DOM hydrate。
pub fn build_awsl_mp_source(components: &[LoweredComponent]) -> String {
    let mut out = String::from(MP_SIG_PRELUDE);
    out.push_str("micro awsl_key_none(): utf8 { return \"\" }\n\n");
    let mut generated = String::new();
    for component in components {
        emit_module_level_signal_slots(&mut generated, component);
        emit_mp_component_init(&mut generated, component);
        if !component.synthetic_v.trim().is_empty() {
            generated.push_str(&component.synthetic_v);
            generated.push('\n');
        }
        emit_component_call_wrappers(&mut generated, component);
        emit_mp_sig_value_exports(&mut generated, component);
    }
    push_host_callable_micros(&mut out, &generated);
    out
}

/// 生成源码的顶层 `micro` 是宿主 ABI 的一部分：JS / 原生宿主按名字直接调用，
/// 因此每条都必须显式 `[export]`。缺注解时编译虽然成功，但调用会静默落空。
///
/// 只处理生成段，不处理静态 prelude：prelude 里的辅助函数不是宿主调用表面。
fn push_host_callable_micros(out: &mut String, generated: &str) {
    for line in generated.lines() {
        if line.starts_with("micro ") {
            out.push_str("[export]\n");
        }
        out.push_str(line);
        out.push('\n');
    }
}

fn emit_module_level_signal_slots(out: &mut String, component: &LoweredComponent) {
    let mut wrote = false;
    for binding in &component.script_bindings {
        if !binding.reactive {
            continue;
        }
        writeln!(out, "{}", binding.as_v_let_line()).unwrap();
        wrote = true;
    }
    if wrote {
        writeln!(out).unwrap();
    }
}

fn emit_mp_component_init(out: &mut String, component: &LoweredComponent) {
    let route = sanitize_route(&component.route_name);
    writeln!(out, "micro awsl_mp_init_{route}() {{").unwrap();
    for binding in &component.script_bindings {
        if !binding.reactive {
            continue;
        }
        let reset = match binding.value_type {
            SignalValueType::I32 => format!("sig_set_i32({}, {});", binding.sig_var, binding.init_expr),
            SignalValueType::Utf8 => format!("sig_set_utf8({}, {});", binding.sig_var, binding.init_expr),
            SignalValueType::Bool => format!("sig_set_bool({}, {});", binding.sig_var, binding.init_expr),
        };
        writeln!(out, "    {reset}").unwrap();
    }
    writeln!(out, "}}\n").unwrap();
}

fn build_awsl_source_with_prelude(components: &[LoweredComponent], dom_or_mobile: &str, reactive: &str) -> String {
    let mut out = String::from(dom_or_mobile);
    out.push_str(reactive);
    out.push_str("micro awsl_key_none(): utf8 { return \"\" }\n\n");
    let mut generated = String::new();
    for component in components {
        emit_module_level_signal_slots(&mut generated, component);
        if !component.synthetic_v.trim().is_empty() {
            generated.push_str(&component.synthetic_v);
            generated.push('\n');
        }
        emit_component_call_wrappers(&mut generated, component);
        emit_component_sig_exports(&mut generated, component);
        let script_body = script_lets_prelude(component);
        generated.push_str(&render_component_v(&component.route_name, &component.script_bindings, &script_body, &component.render_ir));
        generated.push('\n');
    }
    push_host_callable_micros(&mut out, &generated);
    out
}

fn emit_component_call_wrappers(out: &mut String, component: &LoweredComponent) {
    for line in component.synthetic_v.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("micro ") {
            if let Some(micro_name) = rest.split(|c: char| c.is_whitespace() || c == '(').next() {
                if !micro_name.is_empty() {
                    writeln!(out, "micro awsl_call_{micro_name}() {{\n    {micro_name}()\n}}\n").unwrap();
                }
            }
        }
    }
    let _ = component.route_name.as_str();
}

fn emit_component_sig_exports(out: &mut String, component: &LoweredComponent) {
    for binding in &component.script_bindings {
        if !binding.reactive {
            continue;
        }
        let export = resolve_sig_export(&component.route_name, &binding.name);
        writeln!(out, "micro {export}(): i32 {{ return {} }}", binding.sig_var).unwrap();
    }
}

/// 小程序：`awsl_sig_*` 返回当前值（供 setData 同步），不是 signal id。
fn emit_mp_sig_value_exports(out: &mut String, component: &LoweredComponent) {
    for binding in &component.script_bindings {
        if !binding.reactive {
            continue;
        }
        let export = resolve_sig_export(&component.route_name, &binding.name);
        match binding.value_type {
            SignalValueType::I32 => {
                writeln!(out, "micro {export}(): i32 {{ return sig_get_i32({}) }}", binding.sig_var).unwrap();
            }
            SignalValueType::Utf8 => {
                writeln!(out, "micro {export}(): utf8 {{ return sig_get_utf8({}) }}", binding.sig_var).unwrap();
            }
            SignalValueType::Bool => {
                writeln!(out, "micro {export}(): bool {{ return sig_get_bool({}) }}", binding.sig_var).unwrap();
            }
        }
    }
}

fn sanitize_route(name: &str) -> String {
    name.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() || ch == '_' { ch } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_ascii_lowercase()
}

fn script_lets_prelude(component: &LoweredComponent) -> String {
    component.script_bindings.iter().map(ScriptBinding::as_v_let_line).collect::<Vec<_>>().join("\n    ")
}
