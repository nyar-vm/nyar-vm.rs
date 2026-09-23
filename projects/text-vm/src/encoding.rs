/// 文本编码类型，编译期静态已知。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum TextEncoding {
    /// ASCII 编码（单字节）。
    #[default]
    Ascii = 0,
    /// UTF-8 编码。
    Utf8 = 1,
    /// UTF-16 小端编码。
    Utf16Le = 2,
    /// UTF-16 大端编码。
    Utf16Be = 3,
}

impl TextEncoding {
    /// 从原始字节值解析编码类型。
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Ascii),
            1 => Some(Self::Utf8),
            2 => Some(Self::Utf16Le),
            3 => Some(Self::Utf16Be),
            _ => None,
        }
    }
}
