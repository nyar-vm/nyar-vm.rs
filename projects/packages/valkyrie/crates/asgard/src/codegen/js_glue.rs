//! 组件 JS 胶水：仅负责调用 WASM 导出、交还 DOM 句柄。

use std::fmt::Write as _;

use crate::{
    awsl::{LoweredComponent, is_fragment_root},
    codegen::v_render::{export_mount_name_for_route, export_name_for_route},
};

/// JS 胶水输出。
#[derive(Debug, Clone)]
pub struct JsGlueOutput {
    /// 组件名。
    pub component_name: String,
    /// 胶水 JS 源码。
    pub content: String,
    /// 相对路径（如 `c/index.js`）。
    pub relative_path: String,
}

/// 生成组件 JS 胶水（无模板逻辑，逻辑在 WASM）。
pub fn generate_component_glue(component: &LoweredComponent, _wasm_module: &str) -> JsGlueOutput {
    let route = &component.route_name;
    let fragment = is_fragment_root(&component.render_ir);
    let export_name = if fragment { export_mount_name_for_route(route) } else { export_name_for_route(route) };

    let mut out = String::new();
    writeln!(out, "(function() {{").unwrap();
    writeln!(out, "  'use strict';").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "// AWSL 组件胶水：{route}（逻辑在 WASM）").unwrap();
    writeln!(out, "var RENDER_EXPORT = '{export_name}';").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "function factory(host) {{").unwrap();
    writeln!(out, "  var asgard = globalThis.__voa;").unwrap();
    writeln!(out, "  if (!asgard || !asgard.isLoaded()) {{").unwrap();
    writeln!(out, "    return document.createComment('asgard:wasm-pending');").unwrap();
    writeln!(out, "  }}").unwrap();
    if fragment {
        writeln!(out, "  if (host && host.nodeType) {{").unwrap();
        writeln!(out, "    var hostHandle = asgard.storeDomHandle(host);").unwrap();
        writeln!(out, "    asgard.callExport(RENDER_EXPORT, hostHandle);").unwrap();
        writeln!(out, "    return host;").unwrap();
        writeln!(out, "  }}").unwrap();
        writeln!(out, "  return document.createComment('asgard:fragment');").unwrap();
    }
    else {
        writeln!(out, "  var handle = asgard.callExport(RENDER_EXPORT);").unwrap();
        writeln!(out, "  var node = asgard.getDomHandle(handle);").unwrap();
        writeln!(out, "  if (!node) {{").unwrap();
        writeln!(out, "    var el = document.createElement('div');").unwrap();
        writeln!(out, "    el.setAttribute('data-asgard-wasm-handle', String(handle));").unwrap();
        writeln!(out, "    return el;").unwrap();
        writeln!(out, "  }}").unwrap();
        writeln!(out, "  return node;").unwrap();
    }
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "globalThis.__voa.registerComponent('{route}', factory);").unwrap();
    writeln!(out, "}})();").unwrap();

    JsGlueOutput { component_name: component.name.clone(), content: out, relative_path: format!("c/{route}.js") }
}
