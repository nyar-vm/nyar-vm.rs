//! `app.json` 路由表生成。

use std::fmt::Write as _;

use crate::awsl::LoweredComponent;

/// `app.json` 输出。
#[derive(Debug, Clone)]
pub struct MpAppJsonOutput {
    /// 相对路径。
    pub relative_path: String,
    /// JSON 内容。
    pub content: String,
}

/// 从页面组件列表生成 `app.json`。
pub fn generate_app_json(components: &[LoweredComponent], project_name: &str) -> MpAppJsonOutput {
    let pages: Vec<String> = components.iter().map(|c| format!("pages/{}/{}", c.route_name, c.route_name)).collect();
    let entry = pages.first().cloned().unwrap_or_else(|| "pages/index/index".into());
    let mut content = String::new();
    writeln!(content, "{{").unwrap();
    writeln!(content, "  \"pages\": [").unwrap();
    for (index, page) in pages.iter().enumerate() {
        let comma = if index + 1 == pages.len() { "" } else { "," };
        writeln!(content, "    \"{page}\"{comma}").unwrap();
    }
    writeln!(content, "  ],").unwrap();
    writeln!(content, "  \"window\": {{").unwrap();
    writeln!(content, "    \"navigationBarTitleText\": \"{project_name}\"").unwrap();
    writeln!(content, "  }},").unwrap();
    writeln!(content, "  \"entryPagePath\": \"{entry}\"").unwrap();
    writeln!(content, "}}").unwrap();
    MpAppJsonOutput { relative_path: "app.json".into(), content }
}
