//! VON 值文本序列化（委托 `oak-von::printer` / serde）。

#[cfg(feature = "serde")]
pub fn to_string<T>(value: &T) -> Result<String, oak_core::OakError>
where
    T: serde::Serialize,
{
    oak_von::to_string(value)
}

#[cfg(feature = "serde")]
pub fn to_string_pretty<T>(value: &T) -> Result<String, oak_core::OakError>
where
    T: serde::Serialize,
{
    oak_von::to_string_indented(value, 4)
}
