//! Atlas compile-time injectors: middleware dispatch, WebSocket handlers, config schema, manifest emission.

use std::{env, fmt::Write as _, fs, path::Path};

use crate::types::{
    Identifier,
    hir::{HirArgument, HirAttribute, HirExpr, HirFunction, HirModule, HirStruct},
};

use super::route_injector::{AtlasRouteEntry, RouteInjectionResult, collect_atlas_routes, inject_atlas_routes, generate_route_manifest_json};

/// Collected middleware binding from `@middleware("name")` on controller methods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtlasMiddlewareEntry {
    pub name: String,
    pub controller: String,
    pub method_name: String,
}

/// Collected WebSocket route from `@ws("/path")` on handler types/methods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtlasWsEntry {
    pub path: String,
    pub handler_id: String,
    pub controller: String,
    pub method_name: String,
}

/// Collected health check from `@health_check("db")` on controller methods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtlasHealthCheckEntry {
    pub name: String,
    pub controller: String,
    pub method_name: String,
}

/// Config section discovered via `@config_section("atlas")` on a struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtlasConfigSection {
    pub section: String,
    pub struct_name: String,
    pub fields: Vec<String>,
}

/// Full Atlas injection artifacts for one HIR module.
#[derive(Debug, Clone, Default)]
pub struct AtlasInjectionArtifacts {
    pub routes: RouteInjectionResult,
    pub middleware_v: String,
    pub ws_v: String,
    pub config_v: String,
    pub health_checks: Vec<AtlasHealthCheckEntry>,
    pub manifest_json: String,
}

const MIDDLEWARE_ATTR: &str = "middleware";
const WS_ATTR: &str = "ws";
const HEALTH_ATTR: &str = "health_check";
const CONFIG_SECTION_ATTR: &str = "config_section";

/// Run all Atlas injectors against a lowered module.
pub fn inject_atlas_artifacts(module: &HirModule) -> AtlasInjectionArtifacts {
    let routes = super::route_injector::inject_atlas_routes(module);
    let middleware = collect_middleware(module);
    let ws = collect_ws_routes(module);
    let health_checks = collect_health_checks(module);
    let config_sections = collect_config_sections(module);
    let middleware_v = generate_middleware_dispatch_v(&middleware);
    let ws_v = generate_ws_dispatch_v(&ws);
    let config_v = generate_config_loader_v(&config_sections);
    let manifest_json = merge_manifest(&routes.routes, &ws, &health_checks, &config_sections);
    AtlasInjectionArtifacts {
        routes,
        middleware_v,
        ws_v,
        config_v,
        health_checks,
        manifest_json,
    }
}

/// Write generated Atlas artifacts when `VALKYRIE_ATLAS_OUT_DIR` is set.
pub fn try_emit_atlas_artifacts(module: &HirModule) {
    let Ok(out_dir) = env::var("VALKYRIE_ATLAS_OUT_DIR") else {
        return;
    };
    let artifacts = inject_atlas_artifacts(module);
    let _ = emit_atlas_artifacts(&out_dir, &artifacts);
}

/// Write routes, middleware, ws, config V sources and `atlas.manifest.json`.
pub fn emit_atlas_artifacts(out_dir: &str, artifacts: &AtlasInjectionArtifacts) -> std::io::Result<()> {
    fs::create_dir_all(out_dir)?;
    fs::write(Path::new(out_dir).join("routes_generated.v"), &artifacts.routes.generated_v)?;
    if !artifacts.middleware_v.is_empty() {
        fs::write(Path::new(out_dir).join("middleware_generated.v"), &artifacts.middleware_v)?;
    }
    if !artifacts.ws_v.is_empty() {
        fs::write(Path::new(out_dir).join("ws_generated.v"), &artifacts.ws_v)?;
    }
    if !artifacts.config_v.is_empty() {
        fs::write(Path::new(out_dir).join("config_generated.v"), &artifacts.config_v)?;
    }
    fs::write(Path::new(out_dir).join("atlas.manifest.json"), &artifacts.manifest_json)?;
    Ok(())
}

fn collect_middleware(module: &HirModule) -> Vec<AtlasMiddlewareEntry> {
    let mut out = Vec::new();
    let mut visitor = |strukt: &HirStruct| {
        for method in &strukt.methods {
            if let Some(name) = attr_string_by_name(&method.annotations, MIDDLEWARE_ATTR) {
                out.push(AtlasMiddlewareEntry {
                    name,
                    controller: strukt.name.as_str().to_string(),
                    method_name: method.name.as_str().to_string(),
                });
            }
        }
    };
    visit_structs(module, &mut visitor);
    out
}

fn collect_ws_routes(module: &HirModule) -> Vec<AtlasWsEntry> {
    let mut out = Vec::new();
    let mut visitor = |strukt: &HirStruct| {
        for method in &strukt.methods {
            if let Some(path) = attr_string_by_name(&method.annotations, WS_ATTR) {
                let handler_id = ws_handler_id(strukt.name.as_str(), method.name.as_str());
                out.push(AtlasWsEntry {
                    path,
                    handler_id,
                    controller: strukt.name.as_str().to_string(),
                    method_name: method.name.as_str().to_string(),
                });
            }
        }
    };
    visit_structs(module, &mut visitor);
    out
}

fn collect_health_checks(module: &HirModule) -> Vec<AtlasHealthCheckEntry> {
    let mut out = Vec::new();
    let mut visitor = |strukt: &HirStruct| {
        if !is_controller(&strukt.name) {
            return;
        }
        for method in &strukt.methods {
            let name = attr_string_by_name(&method.annotations, HEALTH_ATTR)
                .unwrap_or_else(|| method.name.as_str().to_string());
            if method.annotations.iter().any(|a| attr_last_name(a) == HEALTH_ATTR) {
                out.push(AtlasHealthCheckEntry {
                    name,
                    controller: strukt.name.as_str().to_string(),
                    method_name: method.name.as_str().to_string(),
                });
            }
        }
    };
    visit_structs(module, &mut visitor);
    out
}

fn collect_config_sections(module: &HirModule) -> Vec<AtlasConfigSection> {
    let mut out = Vec::new();
    let mut visitor = |strukt: &HirStruct| {
        if let Some(section) = struct_attr_string(strukt, CONFIG_SECTION_ATTR) {
            let fields = strukt.fields.iter().map(|f| f.name.as_str().to_string()).collect();
            out.push(AtlasConfigSection {
                section,
                struct_name: strukt.name.as_str().to_string(),
                fields,
            });
        }
    };
    visit_structs(module, &mut visitor);
    out
}

fn struct_attr_string(strukt: &HirStruct, attr_name: &str) -> Option<String> {
    for line in &strukt.doc.lines {
        if let Some(value) = parse_doc_attr(line, attr_name) {
            return Some(value);
        }
    }
    None
}

fn parse_doc_attr(line: &str, attr_name: &str) -> Option<String> {
    let trimmed = line.trim();
    let needle = format!("@{attr_name}");
    if !trimmed.contains(&needle) {
        return None;
    }
    let start = trimmed.find('(')? + 1;
    let end = trimmed.rfind(')')?;
    if end <= start {
        return None;
    }
    let inner = trimmed[start..end].trim();
    Some(inner.trim_matches('"').to_string())
}

fn visit_structs(module: &HirModule, f: &mut dyn FnMut(&HirStruct)) {
    for strukt in &module.structs {
        f(strukt);
    }
    for submodule in &module.submodules {
        visit_structs(submodule, f);
    }
}

fn is_controller(name: &Identifier) -> bool {
    name.as_str().ends_with("Controller")
}

fn ws_handler_id(controller: &str, method: &str) -> String {
    let base = controller.strip_suffix("Controller").unwrap_or(controller);
    format!("{}_{}", camel_to_snake(base), method)
}

fn camel_to_snake(name: &str) -> String {
    let mut out = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn attr_last_name(attr: &HirAttribute) -> String {
    attr.name.parts().last().map(|id| id.as_str().to_string()).unwrap_or_default()
}

fn attr_string_by_name(attrs: &[HirAttribute], name: &str) -> Option<String> {
    for attr in attrs {
        if attr_last_name(attr) == name {
            return attr_string_arg(attr);
        }
    }
    None
}

fn attr_string_arg(attr: &HirAttribute) -> Option<String> {
    let first = attr.arguments.first()?;
    match &*first.value {
        HirExpr { kind: crate::types::hir::HirExprKind::Literal(crate::types::hir::HirLiteral::String(lit)), .. } => {
            let mut out = String::new();
            for seg in &lit.segments {
                if let crate::types::hir::HirStringSegment::Text(t) = seg {
                    out.push_str(t);
                }
            }
            Some(out)
        }
        HirExpr { kind: crate::types::hir::HirExprKind::Path(path), .. } => path.parts().last().map(|p| p.as_str().to_string()),
        _ => None,
    }
}

pub fn generate_middleware_dispatch_v(entries: &[AtlasMiddlewareEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "# Generated by MiddlewareInjector — do not edit\n\
         namespace atlas.core;\n\n\
         using std.text;\n\
         using atlas.http;\n\n\
         micro atlas_invoke_middleware(name: utf8, middleware: AtlasMiddleware, mut ctx: AtlasRouteContext, next_index: i32, mut pipeline: MiddlewarePipeline, mut host: AtlasHost): unit {\n",
    );
    for entry in entries {
        writeln!(
            out,
            "    if name.equals(\"{}\") {{\n        let controller: {} = {}::wire(host.container)\n        controller.{}(middleware, ctx, next_index, pipeline, host)\n        return\n    }}",
            entry.name, entry.controller, entry.controller, entry.method_name
        )
        .unwrap();
    }
    writeln!(out, "    pipeline.execute_at(next_index, ctx, host)").unwrap();
    writeln!(out, "}}\n").unwrap();
    out
}

pub fn generate_ws_dispatch_v(entries: &[AtlasWsEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "# Generated by WsHandlerInjector — do not edit\n\
         namespace atlas.ws;\n\n\
         using std.text;\n\n\
         micro atlas_invoke_ws_handler(handler_id: utf8, event: utf8, mut ctx: AtlasWebSocketContext, message: utf8): unit {\n",
    );
    for entry in entries {
        writeln!(
            out,
            "    if handler_id.equals(\"{}\") {{\n        let controller: {} = {}::wire_ws(ctx)\n        controller.{}(event, ctx, message)\n        return\n    }}",
            entry.handler_id, entry.controller, entry.controller, entry.method_name
        )
        .unwrap();
    }
    writeln!(out, "}}\n").unwrap();

    let mut table = String::from(
        "# Generated WebSocket route table\n\
         namespace atlas.ws;\n\n\
         micro atlas_register_ws_routes(mut table: AtlasWebSocketRouteTable): AtlasWebSocketRouteTable {\n",
    );
    for entry in entries {
        writeln!(table, "    table = table.map(\"{}\", \"{}\")", entry.path, entry.handler_id).unwrap();
    }
    writeln!(table, "    return table\n}}\n").unwrap();
    out.push_str(&table);
    out
}

pub fn generate_config_loader_v(sections: &[AtlasConfigSection]) -> String {
    if sections.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "# Generated by ConfigInjector — do not edit\n\
         namespace atlas.config;\n\n\
         using std.text;\n\n",
    );
    for section in sections {
        writeln!(
            out,
            "class {} {{\n    section: utf8\n}}\n",
            section.struct_name
        )
        .unwrap();
        for field in &section.fields {
            writeln!(out, "# field: {}", field).unwrap();
        }
        writeln!(
            out,
            "imply {} {{\n    micro load_from_von(mut self, source: utf8): bool {{\n        let _src: utf8 = source\n        return self.section.equals(\"{}\")\n    }}\n}}\n",
            section.struct_name, section.section
        )
        .unwrap();
    }
    writeln!(
        out,
        "class AtlasListenConfig {{\n    host: utf8\n    port: i32\n}}\n\n\
         class AtlasTlsConfig {{\n    enabled: bool\n    cert_path: utf8\n    key_path: utf8\n    min_version: utf8\n}}\n\n\
         class AtlasRuntimeConfig {{\n    listen: AtlasListenConfig\n    tls: AtlasTlsConfig\n}}\n\n\
         imply AtlasRuntimeConfig {{\n    micro defaults(): Self {{\n        return Self {{\n            listen: AtlasListenConfig {{ host: \"0.0.0.0\", port: 8080 }},\n            tls: AtlasTlsConfig {{ enabled: false, cert_path: \"\", key_path: \"\", min_version: \"1.2\" }}\n        }}\n    }}\n\n\
         micro validate(self): bool {{\n        if self.listen.port <= 0 {{ return false }}\n        if self.tls.enabled && self.tls.cert_path.is_empty() {{ return false }}\n        if self.tls.enabled && self.tls.key_path.is_empty() {{ return false }}\n        return true\n    }}\n}}\n"
    )
    .unwrap();
    out
}

fn merge_manifest(
    routes: &[AtlasRouteEntry],
    ws: &[AtlasWsEntry],
    health: &[AtlasHealthCheckEntry],
    config: &[AtlasConfigSection],
) -> String {
    let mut out = generate_route_manifest_json(routes);
    out.push_str(",\n  \"ws_routes\": [\n");
    for (i, entry) in ws.iter().enumerate() {
        let comma = if i + 1 < ws.len() { "," } else { "" };
        writeln!(
            out,
            "    {{\"path\": \"{}\", \"handler_id\": \"{}\", \"controller\": \"{}\"}}{comma}",
            entry.path, entry.handler_id, entry.controller
        )
        .unwrap();
    }
    out.push_str("  ],\n  \"health_checks\": [\n");
    for (i, entry) in health.iter().enumerate() {
        let comma = if i + 1 < health.len() { "," } else { "" };
        writeln!(
            out,
            "    {{\"name\": \"{}\", \"controller\": \"{}\", \"method\": \"{}\"}}{comma}",
            entry.name, entry.controller, entry.method_name
        )
        .unwrap();
    }
    out.push_str("  ],\n  \"listen\": { \"host\": \"0.0.0.0\", \"port\": 8080 },\n");
    out.push_str("  \"tls\": { \"enabled\": false, \"feature\": \"tls\" },\n");
    out.push_str("  \"config_sections\": [\n");
    for (i, section) in config.iter().enumerate() {
        let comma = if i + 1 < config.len() { "," } else { "" };
        writeln!(
            out,
            "    {{\"section\": \"{}\", \"struct\": \"{}\"}}{comma}",
            section.section, section.struct_name
        )
        .unwrap();
    }
    out.push_str("  ]\n}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        Identifier,
        hir::{HirDocumentation, HirModule, HirStruct},
    };

    #[test]
    fn parse_doc_config_section() {
        let strukt = HirStruct {
            name: Identifier::new("AtlasAppConfig"),
            doc: HirDocumentation::from_lines(vec!["@config_section(\"atlas\")".to_string()]),
            fields: vec![],
            ..Default::default()
        };
        let section = struct_attr_string(&strukt, CONFIG_SECTION_ATTR);
        assert_eq!(section.as_deref(), Some("atlas"));
    }

    #[test]
    fn manifest_includes_ws_and_health() {
        let routes = vec![AtlasRouteEntry {
            controller: "HealthController".into(),
            method_name: "get_live".into(),
            http_method: "GET".into(),
            path: "/health/live".into(),
            handler_id: "health_live".into(),
        }];
        let ws = vec![AtlasWsEntry {
            path: "/ws/echo".into(),
            handler_id: "echo".into(),
            controller: "EchoWsHandler".into(),
            method_name: "on_event".into(),
        }];
        let health = vec![AtlasHealthCheckEntry {
            name: "db".into(),
            controller: "HealthController".into(),
            method_name: "check_db".into(),
        }];
        let manifest = merge_manifest(&routes, &ws, &health, &[]);
        assert!(manifest.contains("ws_routes"));
        assert!(manifest.contains("health_checks"));
        assert!(manifest.contains("/health/live"));
    }
}
