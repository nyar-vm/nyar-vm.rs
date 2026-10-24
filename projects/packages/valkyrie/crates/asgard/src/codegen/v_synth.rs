//! synthetic V 辅助（降级主逻辑在 awsl::lower）。

use crate::awsl::LoweredComponent;

/// 收集所有 synthetic V 模块文本。
pub fn collect_synthetic_v(components: &[LoweredComponent]) -> String {
    let mut out = String::new();
    for component in components {
        out.push_str(&component.synthetic_v);
        out.push('\n');
    }
    out
}
