//! Atlas route codegen: scan `@get` / `@post` / … on controller methods and emit registration + dispatch V source.

use std::fmt::Write as _;

use crate::types::{
    Identifier, NamePath,
    hir::{HirArgument, HirAttribute, HirExpr, HirFunction, HirModule, HirStruct, ValkyrieType},
};

/// One HTTP route discovered on a controller method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtlasRouteEntry {
    pub controller: String,
    pub method_name: String,
    pub http_method: String,
    pub path: String,
    pub handler_id: String,
}

/// Collected routes + optional manifest JSON fragment.
#[derive(Debug, Clone, Default)]
pub struct RouteInjectionResult {
    pub routes: Vec<AtlasRouteEntry>,
    pub generated_v: String,
    pub manifest_json: String,
}

const ROUTE_ATTRS: &[&str] = &["get", "post", "put", "delete", "patch"];

/// Scan lowered HIR for `*Controller` types with route attributes on methods.
pub fn collect_atlas_routes(module: &HirModule) -> Vec<AtlasRouteEntry> {
    let mut routes = Vec::new();
    collect_routes_module(module, &mut routes);
    routes
}

fn collect_routes_module(module: &HirModule, routes: &mut Vec<AtlasRouteEntry>) {
    for strukt in &module.structs {
        if is_controller(&strukt.name) {
            collect_controller_routes(strukt, routes);
        }
    }
    for submodule in &module.submodules {
        collect_routes_module(submodule, routes);
    }
}

fn is_controller(name: &Identifier) -> bool {
    name.as_str().ends_with("Controller")
}

fn collect_controller_routes(strukt: &HirStruct, routes: &mut Vec<AtlasRouteEntry>) {
    let controller = strukt.name.as_str().to_string();
    let prefix = route_prefix_from_doc(&strukt.doc.lines);
    for method in &strukt.methods {
        if let Some((http_method, path_suffix)) = route_from_method(method) {
            let path = join_path(&prefix, &path_suffix);
            let handler_id = handler_id_for(&controller, method.name.as_str());
            routes.push(AtlasRouteEntry {
                controller: controller.clone(),
                method_name: method.name.as_str().to_string(),
                http_method,
                path,
                handler_id,
            });
        }
    }
}

fn route_prefix_from_doc(lines: &[String]) -> String {
    for line in lines {
        if let Some(prefix) = parse_doc_attr(line, "route_prefix") {
            return prefix;
        }
    }
    String::new()
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

fn route_from_method(method: &HirFunction) -> Option<(String, String)> {
    for attr in &method.annotations {
        let name = attr.name.parts().last().map(|id| id.as_str().to_string())?;
        let http = ROUTE_ATTRS.iter().find(|&&n| n == name.as_str())?;
        let path = attr_string_arg(attr).unwrap_or_else(|| "/".to_string());
        return Some((http.to_ascii_uppercase(), path));
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
        HirExpr { kind: crate::types::hir::HirExprKind::Path(path), .. } => path.parts().last().map(|p| format!("/{}", p.as_str())),
        _ => None,
    }
}

fn join_path(prefix: &str, suffix: &str) -> String {
    if prefix.is_empty() {
        return suffix.to_string();
    }
    if suffix == "/health/live" || suffix == "/health/ready" {
        return suffix.to_string();
    }
    let p = prefix.trim_end_matches('/');
    let s = if suffix.starts_with('/') { suffix.to_string() } else { format!("/{suffix}") };
    format!("{p}{s}")
}

fn handler_id_for(controller: &str, method: &str) -> String {
    let base = controller.strip_suffix("Controller").unwrap_or(controller);
    let snake = camel_to_snake(base);
    if method == "get_health" || method.starts_with("get_") && method.len() > 4 {
        let action = method.strip_prefix("get_").unwrap_or(method);
        if action == snake.as_str() || action == "health" {
            return snake;
        }
        return format!("{snake}_{action}");
    }
    format!("{snake}_{method}")
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

/// Generate `atlas_routes_generated.v` source for the given routes.
pub fn generate_atlas_routes_v(routes: &[AtlasRouteEntry]) -> String {
    let mut out = String::from(
        "# Generated by RouteInjector — do not edit\n\
         namespace atlas.core;\n\n\
         using std.text;\n\
         using atlas.http;\n\
         using atlas.wire;\n\n",
    );

    writeln!(out, "micro atlas_register_routes(mut host: AtlasHost): AtlasHost {{").unwrap();
    for route in routes {
        let reg = match route.http_method.as_str() {
            "GET" => format!("host.get_route(\"{}\", \"{}\")", route.path, route.handler_id),
            "POST" => format!("host.post(\"{}\", \"{}\")", route.path, route.handler_id),
            "PUT" => format!("host.put_route(\"{}\", \"{}\")", route.path, route.handler_id),
            "DELETE" => format!("host.delete_route(\"{}\", \"{}\")", route.path, route.handler_id),
            _ => format!("host.map(\"{}\", \"{}\", \"{}\")", route.http_method, route.path, route.handler_id),
        };
        writeln!(out, "    host = {reg}").unwrap();
    }
    writeln!(out, "    return host").unwrap();
    writeln!(out, "}}\n").unwrap();

    writeln!(
        out,
        "micro atlas_invoke_handler(handler_id: utf8, ctx: AtlasRouteContext, container: AtlasWireContainer): AtlasResult {{"
    )
    .unwrap();
    for route in routes {
        writeln!(
            out,
            "    if handler_id.equals(\"{}\") {{\n        let _ctx: AtlasRouteContext = ctx\n        let controller: {} = {}::wire(container)\n        return controller.{}()\n    }}",
            route.handler_id, route.controller, route.controller, route.method_name
        )
        .unwrap();
    }
    writeln!(out, "    return AtlasResult::not_found()").unwrap();
    writeln!(out, "}}\n").unwrap();

    out
}

/// Generate `atlas.manifest.json` routes section.
pub fn generate_route_manifest_json(routes: &[AtlasRouteEntry]) -> String {
    let mut out = String::from("{\n  \"routes\": [\n");
    for (i, route) in routes.iter().enumerate() {
        let comma = if i + 1 < routes.len() { "," } else { "" };
        writeln!(
            out,
            "    {{\"method\": \"{}\", \"path\": \"{}\", \"handler_id\": \"{}\", \"controller\": \"{}\"}}{comma}",
            route.http_method, route.path, route.handler_id, route.controller
        )
        .unwrap();
    }
    out.push_str("  ]\n");
    out
}

/// Full route injection pass for a module.
pub fn inject_atlas_routes(module: &HirModule) -> RouteInjectionResult {
    let routes = collect_atlas_routes(module);
    RouteInjectionResult {
        generated_v: generate_atlas_routes_v(&routes),
        manifest_json: generate_route_manifest_json(&routes),
        routes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        Identifier, NamePath, SourceID, SourceSpan,
        hir::{
            HirBlock, HirDocumentation, HirExpr, HirExprKind, HirFunction, HirLiteral, HirModule, HirStringLiteral, HirStringSegment,
            HirStruct, HirVisibility, ValkyrieType,
        },
    };

    fn test_span() -> SourceSpan {
        SourceSpan::new(SourceID::default(), 0, 0)
    }

    fn empty_module(structs: Vec<HirStruct>) -> HirModule {
        HirModule {
            name: NamePath::new(vec![Identifier::new("test")]),
            doc: HirDocumentation::default(),
            imports: vec![],
            warnings: Vec::new(),
            submodules: vec![],
            functions: vec![],
            structs,
            enums: vec![],
            flags: vec![],
            traits: vec![],
            impls: vec![],
            type_functions: vec![],
            type_families: vec![],
            widgets: vec![],
            singletons: vec![],
            statements: vec![],
            type_aliases: vec![],
        }
    }

    fn route_attr(method: &str, path: &str) -> HirAttribute {
        HirAttribute::with_arguments(
            NamePath::new(vec![Identifier::new(method)]),
            vec![HirArgument {
                key: None,
                value: Box::new(HirExpr {
                    kind: HirExprKind::Literal(HirLiteral::String(HirStringLiteral {
                        prefix: None,
                        quote_count: 1,
                        segments: vec![HirStringSegment::Text(path.to_string())],
                    })),
                    span: test_span(),
                }),
            }],
        )
    }

    fn health_controller() -> HirStruct {
        HirStruct {
            name: Identifier::new("HealthController"),
            methods: vec![HirFunction {
                name: Identifier::new("get_health"),
                declaring_namespace: NamePath::default(),
                doc: HirDocumentation::default(),
                annotations: vec![route_attr("get", "/api/health")],
                generics: vec![],
                params: vec![],
                return_type: ValkyrieType::Named(Identifier::new("AtlasResult")),
                body: HirBlock {
                    statements: vec![],
                    expr: None,
                    span: test_span(),
                },
                span: test_span(),
                visibility: HirVisibility::public(),
                is_abstract: false,
                is_final: false,
                is_virtual: false,
                is_override: false,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn route_prefix_from_class_doc() {
        let strukt = HirStruct {
            name: Identifier::new("OrdersController"),
            doc: HirDocumentation::from_lines(vec!["@route_prefix(\"/api\")".to_string()]),
            methods: vec![HirFunction {
                name: Identifier::new("get_orders"),
                declaring_namespace: NamePath::default(),
                doc: HirDocumentation::default(),
                annotations: vec![route_attr("get", "/orders")],
                generics: vec![],
                params: vec![],
                return_type: ValkyrieType::Named(Identifier::new("AtlasResult")),
                body: HirBlock { statements: vec![], expr: None, span: test_span() },
                span: test_span(),
                visibility: HirVisibility::public(),
                is_abstract: false,
                is_final: false,
                is_virtual: false,
                is_override: false,
            }],
            ..Default::default()
        };
        let module = empty_module(vec![strukt]);
        let routes = collect_atlas_routes(&module);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].path, "/api/orders");
    }

    #[test]
    fn collect_routes_from_controller() {
        let module = empty_module(vec![health_controller()]);
        let routes = collect_atlas_routes(&module);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].http_method, "GET");
        assert_eq!(routes[0].path, "/api/health");
        assert_eq!(routes[0].handler_id, "health");
    }

    #[test]
    fn generated_v_contains_register_and_invoke() {
        let module = empty_module(vec![health_controller()]);
        let result = inject_atlas_routes(&module);
        assert!(result.generated_v.contains("atlas_register_routes"));
        assert!(result.generated_v.contains("atlas_invoke_handler"));
        assert!(result.generated_v.contains("HealthController::wire"));
        assert!(result.generated_v.contains("get_route(\"/api/health\", \"health\")"));
    }
}
