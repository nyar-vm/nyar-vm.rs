use std::{cmp::Ordering, range::Range};
use url::Url;

/// 源文件的稳定标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SourceID {
    /// 源文件的版本化编号。
    pub version_id: u32,
}

/// 源代码中的一段区间。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SourceSpan {
    /// 区间所属的源文件。
    pub source: SourceID,
    /// 在源文件中 byte offset 半开区间。
    #[cfg_attr(feature = "serde", serde(with = "range_u32_serde"))]
    pub span: Range<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Location {
    pub file: Url,
    pub position: Position,
}

impl PartialOrd for SourceSpan {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.source.partial_cmp(&other.source)
    }
}

impl Ord for SourceSpan {
    fn cmp(&self, other: &Self) -> Ordering {
        self.source.cmp(&other.source)
    }
}

impl SourceSpan {
    /// 使用起止偏移创建源码区间。
    pub fn new(source: SourceID, start: u32, end: u32) -> Self {
        Self { source, span: Range { start, end } }
    }

    /// 返回区间起始偏移。
    pub fn get_start(&self) -> u32 {
        self.span.start
    }

    /// 返回区间结束偏移。
    pub fn get_end(&self) -> u32 {
        self.span.end
    }
}

#[cfg(feature = "serde")]
mod range_u32_serde {
    use std::range::Range;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize, Deserialize)]
    struct RangeRepr {
        start: u32,
        end: u32,
    }

    pub fn serialize<S>(range: &Range<u32>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        RangeRepr { start: range.start, end: range.end }.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Range<u32>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let range = RangeRepr::deserialize(deserializer)?;
        Ok(Range { start: range.start, end: range.end })
    }
}
