//! AWSL island kind heuristic (static SSG vs hydrated WASM vs server partial).

use std::path::Path;

/// AWSL 岛渲染方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IslandKind {
    /// 构建期 SSG 静态 HTML。
    Static,
    /// 客户端 hydrate（WASM + glue）。
    Hydrated,
    /// 请求时由 Atlas 返回 HTML 片段（非整站 SSR / LiveView）。
    ///
    /// 枚举与路径启发式已留位；渲染管线 **planned**。
    Server,
}

impl IslandKind {
    /// 是否为静态 SSG 岛。
    pub fn is_static(self) -> bool {
        matches!(self, Self::Static)
    }

    /// 是否为请求时 server 片段岛。
    pub fn is_server(self) -> bool {
        matches!(self, Self::Server)
    }
}

/// 根据 AWSL 相对路径推断岛类型。
pub fn awsl_island_kind(relative_path: &str) -> IslandKind {
    let normalized = relative_path.replace('\\', "/");
    if normalized.starts_with("islands/server/") || normalized.contains("/islands/server/") {
        return IslandKind::Server;
    }
    if normalized.starts_with("charts/") || normalized.contains("/charts/") {
        return IslandKind::Hydrated;
    }
    if normalized.starts_with("pages/") || normalized.contains("/pages/") {
        let name = Path::new(&normalized).file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name == "layout.awsl" || name.ends_with("-report.awsl") {
            return IslandKind::Static;
        }
    }
    IslandKind::Hydrated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chart_routes_are_hydrated() {
        assert_eq!(awsl_island_kind("charts/chart-status.awsl"), IslandKind::Hydrated);
    }

    #[test]
    fn report_pages_are_static() {
        assert_eq!(awsl_island_kind("pages/test-report.awsl"), IslandKind::Static);
        assert_eq!(awsl_island_kind("pages/layout.awsl"), IslandKind::Static);
    }

    #[test]
    fn server_island_paths_are_server() {
        assert_eq!(awsl_island_kind("islands/server/user-card.awsl"), IslandKind::Server);
        assert_eq!(awsl_island_kind("source/islands/server/x.awsl"), IslandKind::Server);
    }
}
