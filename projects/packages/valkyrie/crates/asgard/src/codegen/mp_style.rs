//! `<style>` → WXSS（早期固定 `1px = 2rpx` 换算）。

use crate::awsl::LoweredComponent;

/// WXSS 页面输出。
#[derive(Debug, Clone)]
pub struct MpWxssOutput {
    /// 相对路径。
    pub relative_path: String,
    /// WXSS 内容。
    pub content: String,
}

/// 从组件样式块生成 WXSS。
pub fn generate_page_wxss(component: &LoweredComponent) -> Option<MpWxssOutput> {
    let style = component.style.as_ref()?;
    let route = &component.route_name;
    Some(MpWxssOutput { relative_path: format!("pages/{route}/{route}.wxss"), content: px_to_rpx(style) })
}

fn px_to_rpx(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let chars: Vec<char> = source.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if chars[index].is_ascii_digit() {
            let start = index;
            while index < chars.len() && (chars[index].is_ascii_digit() || chars[index] == '.') {
                index += 1;
            }
            if index < chars.len() && chars[index] == 'p' && index + 1 < chars.len() && chars[index + 1] == 'x' {
                let number: f64 = chars[start..index].iter().collect::<String>().parse().unwrap_or(0.0);
                let rpx = number * 2.0;
                if (rpx - rpx.round()).abs() < f64::EPSILON {
                    out.push_str(&format!("{}rpx", rpx as i64));
                }
                else {
                    out.push_str(&format!("{rpx}rpx"));
                }
                index += 2;
                continue;
            }
            out.extend(&chars[start..index]);
            continue;
        }
        out.push(chars[index]);
        index += 1;
    }
    out
}
#[cfg(test)]
#[path = "../../tests/path_wired/codegen_mp_style.rs"]
mod codegen_mp_style_tests;
