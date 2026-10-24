//! CSS 提取与合并。

use crate::awsl::LoweredComponent;

/// CSS 输出。
#[derive(Debug, Clone)]
pub struct CssOutput {
    /// 合并后的 CSS 文本。
    pub content: String,
    /// 输出文件名（不含目录）。
    pub filename: String,
}

/// 从组件列表提取并合并样式。
pub fn extract_and_merge_styles(
    components: &[LoweredComponent],
    project_name: &str,
    css_mode: &str,
    tailwind_css: Option<&str>,
) -> Vec<CssOutput> {
    match css_mode {
        "per-component" => {
            let mut outputs = components
                .iter()
                .filter_map(|component| {
                    component
                        .style
                        .as_ref()
                        .map(|style| CssOutput { content: style.clone(), filename: format!("{}.css", component.name.to_ascii_lowercase()) })
                })
                .collect::<Vec<_>>();
            if let Some(extra) = tailwind_css {
                if let Some(first) = outputs.first_mut() {
                    crate::codegen::tailwind::append_tailwind_css(&mut first.content, extra);
                }
                else {
                    outputs.push(CssOutput { content: format!("/* tailwind */\n{extra}"), filename: format!("{project_name}.css") });
                }
            }
            outputs
        }
        _ => {
            let mut merged = String::new();
            for component in components {
                if let Some(style) = &component.style {
                    merged.push_str(&format!("/* {} */\n", component.name));
                    merged.push_str(style);
                    merged.push('\n');
                }
            }
            if let Some(extra) = tailwind_css {
                crate::codegen::tailwind::append_tailwind_css(&mut merged, extra);
            }
            vec![CssOutput { content: merged, filename: format!("{project_name}.css") }]
        }
    }
}
