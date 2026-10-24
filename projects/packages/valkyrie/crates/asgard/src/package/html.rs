//! `index.html` 生成（仅 markup：stylesheet + boot.js src，无内联 JS）。

use crate::{
    awsl::LoweredComponent,
    codegen::{asgard_boot_script_tag, asgard_boot_stylesheet_tag},
    config::VoaConfig,
};

/// HTML 输出。
#[derive(Debug, Clone)]
pub struct HtmlOutput {
    /// HTML 内容。
    pub content: String,
    /// 相对路径。
    pub relative_path: String,
}

/// 组件路由元数据。
#[derive(Debug, Clone)]
pub struct ComponentRoute {
    /// 组件文件名（路由名）。
    pub name: String,
    /// island 类型。
    pub island_type: String,
    /// hydrate 策略。
    pub strategy: String,
}

/// 生成入口 HTML。
///
/// `boot.js` auto-starts via generated glue；此处只挂 `<script src>`，禁止内联业务脚本。
pub fn generate_index_html(config: &VoaConfig, components: &[LoweredComponent], css_files: &[String]) -> HtmlOutput {
    let project_name = config.name.clone().unwrap_or_else(|| "asgard-app".into());
    let css_href = css_files.first().cloned().unwrap_or_else(|| format!("/{project_name}.css"));
    let stylesheet = asgard_boot_stylesheet_tag(&css_href);
    let boot_script = asgard_boot_script_tag(false);

    let routes: Vec<ComponentRoute> = components
        .iter()
        .map(|c| ComponentRoute { name: c.route_name.clone(), island_type: c.island_type.clone(), strategy: c.hydrate_strategy.clone() })
        .collect();

    let mut islands = String::new();
    for route in &routes {
        let hydrate = if route.island_type == "hydrated" { format!(" data-hydrate=\"{}\"", route.strategy) } else { String::new() };
        islands.push_str(&format!(
            "    <div data-island=\"{}\" data-component=\"{}\" data-asgard-slot=\"{}\"{hydrate}></div>\n",
            route.island_type, route.name, route.name
        ));
    }

    let content = format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>{project_name}</title>
  {stylesheet}
  {boot_script}
</head>
<body>
  <div id="app">
{islands}  </div>
</body>
</html>
"#
    );

    HtmlOutput { content, relative_path: "index.html".into() }
}
