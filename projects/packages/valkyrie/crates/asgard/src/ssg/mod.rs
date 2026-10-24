//! 静态站点生成（SSG）：AWSL → 纯 HTML 页面，无 hydration。

pub mod project;

use std::fmt::Write as _;

use serde_json::Value;

use crate::{
    awsl::{LoweringOptions, compile_awsl_source},
    codegen::html_render::{StaticRenderResult, render_static_ir},
};

/// 将页面 HTML 注入 layout 的 `<slot />` 占位符。
pub fn wrap_with_layout(layout_source: &str, body_html: &str) -> String {
    layout_source.replace("<slot />", body_html).replace("<slot/>", body_html).replace("<slot></slot>", body_html)
}

/// 解析 AWSL 源码并静态渲染。
pub fn render_awsl_static(source: &str, component_name: &str, source_path: &str, data: &Value) -> StaticRenderResult {
    let component = compile_awsl_source(source, component_name, source_path, &LoweringOptions::default()).expect("awsl compile failed");
    render_static_ir(&component, data)
}

/// 生成完整静态 HTML 页面（无 `asgard-runtime.js`）。
pub fn generate_static_page(title: &str, results: &[StaticRenderResult], extra_css: &str) -> String {
    let mut css = String::new();
    if !extra_css.trim().is_empty() {
        css.push_str(extra_css.trim());
        css.push('\n');
    }
    for result in results {
        if !result.css.trim().is_empty() {
            css.push_str(&format!("/* {} */\n", result.scope));
            css.push_str(result.css.trim());
            css.push('\n');
        }
    }

    let mut body = String::new();
    for result in results {
        body.push_str(&result.html);
        body.push('\n');
    }

    let mut page = String::new();
    writeln!(page, "<!DOCTYPE html>").unwrap();
    writeln!(page, "<html lang=\"zh-CN\">").unwrap();
    writeln!(page, "<head>").unwrap();
    writeln!(page, "  <meta charset=\"utf-8\">").unwrap();
    writeln!(page, "  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">").unwrap();
    writeln!(page, "  <title>{}</title>", escape_html(title)).unwrap();
    if !css.trim().is_empty() {
        writeln!(page, "  <style>").unwrap();
        for line in css.lines() {
            writeln!(page, "    {line}").unwrap();
        }
        writeln!(page, "  </style>").unwrap();
    }
    writeln!(page, "</head>").unwrap();
    writeln!(page, "<body>").unwrap();
    writeln!(page, "{}", body.trim()).unwrap();
    writeln!(page, "</body>").unwrap();
    writeln!(page, "</html>").unwrap();
    page
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
#[cfg(test)]
#[path = "../../tests/path_wired/ssg_mod.rs"]
mod ssg_mod_tests;
