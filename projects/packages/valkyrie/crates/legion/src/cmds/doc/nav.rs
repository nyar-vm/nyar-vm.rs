//! 文档导航树。

use std::path::{Path, PathBuf};

use super::markdown::extract_title;

/// 导航树节点。
#[derive(Debug, Clone)]
pub struct NavNode {
    /// 显示标题。
    pub title: String,
    /// 相对文档根的路径（md）。
    pub relative_path: String,
    /// 链接 href（相对 section 输出目录）。
    pub href: String,
    /// 子节点。
    pub children: Vec<NavNode>,
}

/// 已渲染页面记录。
#[derive(Debug, Clone)]
pub struct DocPage {
    /// 页面标题。
    pub title: String,
    /// 相对 `doc/` 输出目录的路径。
    pub relative_path: String,
    /// 所属分区名。
    pub section_name: String,
}

/// 文档分区。
#[derive(Debug, Clone)]
pub struct DocSection {
    /// 分区显示名。
    pub name: String,
    /// 侧栏标题。
    pub sidebar_title: String,
    /// 源目录。
    pub source_dir: PathBuf,
    /// 已渲染页面。
    pub pages: Vec<DocPage>,
}

/// 从 Markdown 文件列表构建导航树。
pub fn build_nav_tree(md_files: &[PathBuf], doc_root: &Path, _section_name: &str) -> Vec<NavNode> {
    let mut root = Vec::new();

    for md_file in md_files {
        let relative = md_file.strip_prefix(doc_root).unwrap_or(md_file);
        let relative_str = relative.to_string_lossy().replace('\\', "/");
        let parts: Vec<String> = relative_str.split('/').map(str::to_string).collect();
        let title = extract_title(&std::fs::read_to_string(md_file).unwrap_or_default())
            .unwrap_or_else(|| md_file.file_stem().and_then(|s| s.to_str()).unwrap_or("untitled").to_string());

        let mut current = &mut root;
        for (index, part) in parts.iter().enumerate() {
            let is_last = index == parts.len() - 1;
            if is_last {
                let current_path = parts.join("/");
                let href = Path::new(&current_path).with_extension("html").to_string_lossy().replace('\\', "/");
                current.push(NavNode { title: title.clone(), relative_path: current_path, href, children: Vec::new() });
            }
            else {
                let pos = current.iter().position(|n| n.title == *part && n.href.is_empty());
                if let Some(idx) = pos {
                    current = &mut current[idx].children;
                }
                else {
                    current.push(NavNode {
                        title: part.clone(),
                        relative_path: parts[..=index].join("/"),
                        href: String::new(),
                        children: Vec::new(),
                    });
                    let last = current.len() - 1;
                    current = &mut current[last].children;
                }
            }
        }
    }
    root
}

/// 渲染侧栏导航 HTML。
pub fn render_sidebar_html(nodes: &[NavNode], current_relative_path: &str, link_prefix: &str) -> String {
    let mut out = String::from("      <nav class=\"vp-sidebar-nav\">\n");
    render_sidebar_nodes(&mut out, nodes, current_relative_path, link_prefix, 0);
    out.push_str("      </nav>\n");
    out
}

fn render_sidebar_nodes(out: &mut String, nodes: &[NavNode], current_relative_path: &str, link_prefix: &str, depth: usize) {
    let indent = "  ".repeat(depth + 4);
    for node in nodes {
        if !node.children.is_empty() {
            out.push_str(&format!("{indent}<div class=\"vp-nav-section\">\n"));
            out.push_str(&format!("{indent}  <span class=\"vp-nav-section-title\">{}</span>\n", escape_html(&node.title)));
            out.push_str(&format!("{indent}  <ul class=\"vp-nav-list\">\n"));
            render_sidebar_leaf_list(out, &node.children, current_relative_path, link_prefix, depth + 2);
            out.push_str(&format!("{indent}  </ul>\n"));
            out.push_str(&format!("{indent}</div>\n"));
        }
        else {
            let active = if node.relative_path == current_relative_path { " active" } else { "" };
            out.push_str(&format!("{indent}<div class=\"vp-nav-item\">\n"));
            out.push_str(&format!(
                "{indent}  <a href=\"{link_prefix}{}{}\" class=\"vp-nav-link{active}\">{}</a>\n",
                node.href,
                "",
                escape_html(&node.title)
            ));
            out.push_str(&format!("{indent}</div>\n"));
        }
    }
}

fn render_sidebar_leaf_list(out: &mut String, nodes: &[NavNode], current_relative_path: &str, link_prefix: &str, depth: usize) {
    let indent = "  ".repeat(depth + 4);
    for node in nodes {
        if !node.children.is_empty() {
            render_sidebar_leaf_list(out, &node.children, current_relative_path, link_prefix, depth);
        }
        else {
            let active = if node.relative_path == current_relative_path { " active" } else { "" };
            out.push_str(&format!(
                "{indent}<li><a href=\"{link_prefix}{}\" class=\"vp-nav-link{active}\">{}</a></li>\n",
                node.href,
                escape_html(&node.title)
            ));
        }
    }
}

/// 将导航树转为 AWSL 侧栏分区数据（支持嵌套 `<loop>`）。
pub fn nav_sections_for_awsl(nodes: &[NavNode], link_prefix: &str, current_relative_path: &str) -> Vec<serde_json::Value> {
    let mut sections = Vec::new();
    for node in nodes {
        if !node.children.is_empty() {
            sections.push(serde_json::json!({
                "title": node.title,
                "pages": collect_nav_pages(&node.children, link_prefix, current_relative_path),
            }));
        }
        else {
            sections.push(serde_json::json!({
                "title": "",
                "pages": [nav_page_json(node, link_prefix, current_relative_path)],
            }));
        }
    }
    sections
}

fn collect_nav_pages(nodes: &[NavNode], link_prefix: &str, current_relative_path: &str) -> Vec<serde_json::Value> {
    let mut pages = Vec::new();
    collect_nav_pages_inner(nodes, link_prefix, current_relative_path, &mut pages);
    pages
}

fn collect_nav_pages_inner(nodes: &[NavNode], link_prefix: &str, current_relative_path: &str, out: &mut Vec<serde_json::Value>) {
    for node in nodes {
        if !node.children.is_empty() {
            collect_nav_pages_inner(&node.children, link_prefix, current_relative_path, out);
        }
        else {
            out.push(nav_page_json(node, link_prefix, current_relative_path));
        }
    }
}

fn nav_page_json(node: &NavNode, link_prefix: &str, current_relative_path: &str) -> serde_json::Value {
    let link_class = if node.relative_path == current_relative_path { "vp-nav-link active" } else { "vp-nav-link" };
    serde_json::json!({
        "title": node.title,
        "href": format!("{link_prefix}{}", node.href),
        "link_class": link_class,
    })
}

/// 将导航树扁平化为 AWSL 侧栏数据。
pub fn flatten_nav_items(nodes: &[NavNode], link_prefix: &str, current_relative_path: &str) -> Vec<serde_json::Value> {
    let mut items = Vec::new();
    flatten_nav_items_inner(nodes, link_prefix, current_relative_path, &mut items);
    items
}

fn flatten_nav_items_inner(nodes: &[NavNode], link_prefix: &str, current_relative_path: &str, out: &mut Vec<serde_json::Value>) {
    for node in nodes {
        if !node.children.is_empty() {
            flatten_nav_items_inner(&node.children, link_prefix, current_relative_path, out);
        }
        else {
            let link_class = if node.relative_path == current_relative_path { "vp-nav-link active" } else { "vp-nav-link" };
            out.push(serde_json::json!({
                "title": node.title,
                "href": format!("{link_prefix}{}", node.href),
                "link_class": link_class,
            }));
        }
    }
}

/// 目录名安全化。
pub fn sanitize_dir_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            }
            else if c.is_whitespace() || c == '/' || c == '\\' {
                '_'
            }
            else {
                '_'
            }
        })
        .collect()
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
#[cfg(test)]
#[path = "../../../tests/path_wired/cmds_doc_nav.rs"]
mod cmds_doc_nav_tests;
