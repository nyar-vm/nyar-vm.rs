//! 静态资源写出。

use std::{fs, path::Path};

/// 写出 `legion-document.css`。
pub fn write_document_css(output_dir: &Path, base_css: &str, component_css: &str) -> std::io::Result<()> {
    fs::create_dir_all(output_dir)?;
    let mut css = String::new();
    css.push_str(base_css);
    css.push('\n');
    if !component_css.trim().is_empty() {
        css.push_str("/* component styles */\n");
        css.push_str(component_css);
        css.push('\n');
    }
    fs::write(output_dir.join("legion-document.css"), css)
}

/// 嵌入的 base.css 内容。
pub fn embedded_base_css() -> &'static str {
    include_str!("../../../assets/doc/base.css")
}

/// 嵌入的 layout.awsl。
pub fn embedded_layout_awsl() -> &'static str {
    include_str!("../../../assets/doc/layout.awsl")
}

/// 嵌入的 doc-page.awsl（动态侧栏）。
pub fn embedded_doc_page_awsl() -> &'static str {
    include_str!("../../../assets/doc/doc-page.awsl")
}

/// 嵌入的 hub.awsl。
pub fn embedded_hub_awsl() -> &'static str {
    include_str!("../../../assets/doc/hub.awsl")
}
