//! Notedown HTML 渲染选项。

/// Notedown → HTML 渲染配置。
#[derive(Debug, Clone, Default)]
pub struct NotedownHtmlOptions {
    /// 是否为标题生成 `id` 锚点。
    pub generate_heading_ids: bool,
    /// 是否对 fenced code block 做语法高亮。
    pub highlight_code: bool,
    /// 是否将相对 `.md` 链接改写为 `.html`。
    pub rewrite_md_links: bool,
    /// 当前页面所在目录（用于相对链接解析，POSIX `/` 分隔）。
    pub current_dir: String,
}

impl NotedownHtmlOptions {
    /// legion doc 默认选项。
    pub fn legion_doc(current_dir: impl Into<String>) -> Self {
        Self {
            generate_heading_ids: true,
            highlight_code: true,
            rewrite_md_links: true,
            current_dir: current_dir.into(),
        }
    }
}
