use crate::{
    encoding::TextEncoding,
    engine::{CompiledUnit, Match},
};

/// Two-Way 匹配器占位实现，当前使用 `find` 子串搜索。
pub struct TwoWayMatcher;

impl TwoWayMatcher {
    /// 返回 `pattern` 在 `text` 中首次出现的起始索引；未找到时返回 `None`。
    pub fn index_of(text: &[u8], pattern: &[u8]) -> Option<usize> {
        if pattern.is_empty() {
            return Some(0);
        }
        text.windows(pattern.len()).position(|window| window == pattern)
    }
}

/// 字面量执行器。使用 `TwoWayMatcher` 进行快速字面量搜索。
pub struct LiteralExecutor {
    pattern: Vec<u8>,
    #[allow(dead_code)]
    encoding: TextEncoding,
}

impl LiteralExecutor {
    /// 使用给定的模式字节序列和编码创建字面量执行器。
    pub fn new(pattern: Vec<u8>, encoding: TextEncoding) -> Self {
        Self { pattern, encoding }
    }

    fn find_all_internal(&self, input: &[u8]) -> Vec<Match> {
        let mut matches = Vec::new();
        let mut search_start = 0;

        while search_start < input.len() {
            let slice = &input[search_start..];
            let Some(pos) = TwoWayMatcher::index_of(slice, &self.pattern)
            else {
                break;
            };

            let start = search_start + pos;
            let end = start + self.pattern.len();
            matches.push(Match::new(start, end));
            search_start = end;
        }

        matches
    }
}

impl CompiledUnit for LiteralExecutor {
    fn is_match(&self, input: &[u8]) -> bool {
        TwoWayMatcher::index_of(input, &self.pattern).is_some()
    }

    fn find_first(&self, input: &[u8]) -> Option<Match> {
        let pos = TwoWayMatcher::index_of(input, &self.pattern)?;
        Some(Match::new(pos, pos + self.pattern.len()))
    }

    fn find_all(&self, input: &[u8]) -> Vec<Match> {
        self.find_all_internal(input)
    }

    fn replace(&self, input: &[u8], replacement: &[u8]) -> Vec<u8> {
        let matches = self.find_all_internal(input);
        if matches.is_empty() {
            return input.to_vec();
        }

        let pattern_len = self.pattern.len();
        let repl_len = replacement.len();
        let mut new_size = input.len();
        new_size += matches.len() * repl_len.saturating_sub(pattern_len);

        let mut result = Vec::with_capacity(new_size);
        let mut src_pos = 0;

        for m in matches {
            result.extend_from_slice(&input[src_pos..m.start]);
            result.extend_from_slice(replacement);
            src_pos = m.end;
        }

        result.extend_from_slice(&input[src_pos..]);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_finds_substring() {
        let exec = LiteralExecutor::new(b"hello".to_vec(), TextEncoding::Utf8);
        assert!(exec.is_match(b"say hello world"));
    }
}
