//! AWSL 文档解析辅助

use std_data::text::awsl::{AwslParser, AwslRoot};

/// 判断 URI 是否为 AWSL 文件
pub fn is_awsl_uri(uri: &str) -> bool {
    uri.ends_with(".awsl") || uri.contains(".awsl?")
}

/// 解析 AWSL 源文件
pub fn parse_awsl(text: &str) -> Result<AwslRoot, String> {
    AwslParser::parse_root(text).map_err(|e| e.message)
}

/// 若 `offset` 落在 `<script>` 块内，返回块范围及块内相对偏移
pub fn script_offset_at(
    root: &AwslRoot,
    source: &str,
    offset: usize,
) -> Option<(std::ops::Range<usize>, usize)> {
    let range = script_offset_range(root, source)?;
    if range.contains(&offset) {
        Some((range, offset - range.start))
    }
    else {
        None
    }
}

/// 返回 `<script>` 块内容在源文件中的字节范围（不含标签本身）
pub fn script_offset_range(root: &AwslRoot, source: &str) -> Option<std::ops::Range<usize>> {
    let script = root.script.as_ref()?;
    let open = "<script";
    let open_pos = source.find(open)?;
    let body_start = source[open_pos..].find('>')? + open_pos + 1;
    let close = "</script>";
    let close_pos = source[body_start..].find(close)? + body_start;
    if close_pos > body_start {
        Some(body_start..close_pos)
    }
    else {
        None
    }
}
