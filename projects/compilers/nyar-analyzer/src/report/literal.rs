//! 将报告数据序列化为 AWSL script 可注入的字面量。

use super::spec::ColSeriesItem;

/// 将 series 序列化为 Valyrie list 字面量（用于 `let series: list = …`）。
pub fn series_to_init_literal(series: &[ColSeriesItem]) -> String {
    if series.is_empty() {
        return "[]".to_string();
    }
    let items: Vec<String> = series.iter().map(item_to_literal).collect();
    format!("[{}]", items.join(", "))
}

fn item_to_literal(item: &ColSeriesItem) -> String {
    format!(
        "{{ key: {}, label: {}, value_text: {}, fill: {}, height_pct: {} }}",
        utf8_literal(&item.key),
        utf8_literal(&item.label),
        utf8_literal(&item.value_text),
        utf8_literal(&item.fill),
        f64_literal(item.height_pct),
    )
}

fn utf8_literal(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn f64_literal(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 { format!("{value:.1}") } else { format!("{value}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_series_is_empty_list() {
        assert_eq!(series_to_init_literal(&[]), "[]");
    }

    #[test]
    fn serializes_struct_fields() {
        let lit = series_to_init_literal(&[ColSeriesItem {
            key: "pass".into(),
            label: "pass".into(),
            value_text: "2.0 tests".into(),
            fill: "#22c55e".into(),
            height_pct: 100.0,
        }]);
        assert!(lit.contains("key: \"pass\""));
        assert!(lit.contains("height_pct: 100.0"));
    }
}
