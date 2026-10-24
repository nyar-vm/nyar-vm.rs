//! 文档页面渲染（Asgard static renderer）。

use std::{fs, path::Path};

use asgard::ssg::{generate_static_page, render_awsl_static};
use miette::{Context, IntoDiagnostic, Result};
use serde_json::json;

use super::{
    assets::{embedded_base_css, embedded_doc_page_awsl, embedded_hub_awsl, embedded_layout_awsl},
    discover::collect_md_files,
    markdown::{DocRenderContext, extract_title, render_markdown_to_html},
    nav::{self, DocPage, DocSection, build_nav_tree, nav_sections_for_awsl, sanitize_dir_name},
};

/// 渲染所有分区并返回合并的组件 CSS。
pub fn render_sections(sections: &mut [DocSection], doc_output_dir: &Path, verbose: bool) -> Result<String> {
    fs::create_dir_all(doc_output_dir).into_diagnostic()?;
    let mut merged_component_css = String::new();

    for section in sections.iter_mut() {
        let md_files = collect_md_files(&section.source_dir);
        if md_files.is_empty() {
            continue;
        }
        if verbose {
            println!("  渲染分区 {}：{} 个 Markdown 文件", section.name, md_files.len());
        }

        let nav_tree = build_nav_tree(&md_files, &section.source_dir, &section.name);
        let section_output = doc_output_dir.join(sanitize_dir_name(&section.name));
        fs::create_dir_all(&section_output).into_diagnostic()?;

        for md_file in &md_files {
            let source = fs::read_to_string(md_file).into_diagnostic()?;
            let relative = md_file.strip_prefix(&section.source_dir).into_diagnostic()?.to_string_lossy().replace('\\', "/");
            let title =
                extract_title(&source).unwrap_or_else(|| md_file.file_stem().and_then(|s| s.to_str()).unwrap_or("untitled").to_string());
            let render_ctx = super::markdown::DocRenderContext::from_relative_md(&relative);
            let rendered = render_markdown_to_html(&source, &render_ctx);
            let body_html = rendered.html;

            let depth = relative.matches('/').count();
            let link_prefix = "../".repeat(depth);
            let asset_prefix = "../".repeat(depth + 1);

            let sidebar = nav_sections_for_awsl(&nav_tree, &link_prefix, &relative);
            let page_data = json!({
                "sidebar_title": section.sidebar_title,
                "section_name": section.name,
                "home_href": format!("{link_prefix}index.html"),
                "sections": sidebar,
                "__current_path": relative,
            });
            let page_shell = render_awsl_static(embedded_doc_page_awsl(), "doc-page", "doc-page.awsl", &page_data);
            let page_body = page_shell.html.replace("<p>LEGION_DOC_SLOT</p>", &format!("<div class=\"vp-article-inner\">{body_html}</div>"));

            let layout_data = json!({
                "site_title": section.sidebar_title,
                "home_href": format!("{link_prefix}index.html"),
            });
            let rendered = render_awsl_static(embedded_layout_awsl(), "layout", "layout.awsl", &layout_data);
            let fragment_html = rendered.html.replace("<p>LEGION_DOC_SLOT</p>", &page_body);
            merged_component_css.push_str(&rendered.css);
            merged_component_css.push_str(&page_shell.css);
            merged_component_css.push('\n');

            let page_result =
                asgard::codegen::StaticRenderResult { html: fragment_html, css: rendered.css.clone(), scope: rendered.scope.clone() };
            let page_title = format!("{title} - {}", section.name);
            let mut html = generate_static_page(&page_title, &[page_result], embedded_base_css());
            inject_stylesheet_link(&mut html, &format!("{asset_prefix}legion-document.css"));

            let html_rel = Path::new(&relative).with_extension("html");
            let html_path = section_output.join(&html_rel);
            if let Some(parent) = html_path.parent() {
                fs::create_dir_all(parent).into_diagnostic()?;
            }
            fs::write(&html_path, html).into_diagnostic()?;

            let doc_rel = format!("doc/{}/{}", sanitize_dir_name(&section.name), html_rel.to_string_lossy().replace('\\', "/"));
            section.pages.push(DocPage { title, relative_path: doc_rel, section_name: section.name.clone() });
        }
    }

    Ok(merged_component_css)
}

/// 渲染 `doc/index.html` 文档目录页。
pub fn render_doc_index(sections: &[DocSection], doc_output_dir: &Path) -> Result<()> {
    let mut body = String::from("<div class=\"vp-page\"><main class=\"vp-content\"><article class=\"vp-article\">");
    body.push_str("<h1 id=\"用户文档\">用户文档</h1>");
    body.push_str("<p>欢迎查阅用户文档，以下为各分区的文档目录。</p>");

    for section in sections {
        if section.pages.is_empty() {
            continue;
        }
        body.push_str(&format!("<h2>{}</h2><ul class=\"doc-index-list\">", escape_html(&section.name)));
        for page in &section.pages {
            let href = page.relative_path.strip_prefix("doc/").map(str::to_string).unwrap_or_else(|| page.relative_path.clone());
            body.push_str(&format!("<li><a href=\"{href}\">{}</a></li>", escape_html(&page.title)));
        }
        body.push_str("</ul>");
    }
    body.push_str("</article></main></div>");

    let layout_data = json!({
        "site_title": "用户文档",
        "home_href": "index.html",
    });
    let rendered = render_awsl_static(embedded_layout_awsl(), "layout", "layout.awsl", &layout_data);
    let fragment_html = rendered.html.replace("<p>LEGION_DOC_SLOT</p>", &body);
    let page_result = asgard::codegen::StaticRenderResult { html: fragment_html, css: rendered.css, scope: rendered.scope };
    let mut html = generate_static_page("用户文档", &[page_result], embedded_base_css());
    inject_stylesheet_link(&mut html, "../legion-document.css");
    fs::write(doc_output_dir.join("index.html"), html).into_diagnostic()?;
    Ok(())
}

/// 渲染根 hub `index.html`。
pub fn render_root_hub(output_dir: &Path, has_doc: bool) -> Result<()> {
    let data = json!({
        "title": "文档",
        "doc_href": if has_doc { "doc/index.html" } else { "#" },
        "doc_desc": "语言参考、用户指南、工具链与维护者文档",
    });
    let rendered = render_awsl_static(embedded_hub_awsl(), "hub", "hub.awsl", &data);
    let mut html = generate_static_page("文档", &[rendered], "");
    inject_stylesheet_link(&mut html, "legion-document.css");
    fs::write(output_dir.join("index.html"), html).into_diagnostic()?;
    Ok(())
}

fn inject_stylesheet_link(html: &mut String, href: &str) {
    let link = format!("<link rel=\"stylesheet\" href=\"{href}\">");
    if let Some(pos) = html.find("</head>") {
        html.insert_str(pos, &format!("  {link}\n"));
    }
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
