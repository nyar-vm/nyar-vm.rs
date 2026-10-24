//! VON facade — 直连 [`oak_von`]，不经 `vcc-data::text::von`。

/// VON 解析/序列化错误类型，复用 `oak_core::OakError`。
pub type VonError = oak_core::OakError;
pub use oak_von::{from_str, language::value::VonValue, to_string, to_string_indented};

/// 解析 VON 文本为 [`VonValue`]（兼容旧 `VonParser::parse` 调用点）。
pub struct VonParser;

impl VonParser {
    /// 解析 VON 文本为 [`VonValue`]。
    pub fn parse(source: &str) -> Result<VonValue, VonError> {
        let trimmed = source.trim();
        let ast = oak_von::parse(trimmed).map_err(VonError::custom_error)?;
        Ok(oak_von::language::value::from_ast(&ast))
    }
}

/// 将 [`VonValue`] 反序列化为 Rust 类型。
pub fn from_value<T>(value: VonValue) -> Result<T, VonError>
where
    T: serde::de::DeserializeOwned,
{
    let text = oak_von::printer::print_value(&value, &oak_von::printer::PrintOptions::compact());
    oak_von::from_str(&text)
}
