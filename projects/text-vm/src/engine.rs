use std_data::binary::tvm::{TvmFlags, TvmHeader, TvmHeaderError};

use crate::{
    encoding::TextEncoding,
    executors::{BacktrackExecutor, DfaExecutor, LiteralExecutor},
};

/// 匹配结果，使用字节偏移量表示，指向原始输入切片（零拷贝语义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Match {
    /// 匹配起始位置（字节偏移量，包含）。
    pub start: usize,
    /// 匹配结束位置（字节偏移量，不包含）。
    pub end: usize,
}

impl Match {
    /// 使用指定的起始和结束偏移量创建匹配结果。
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// 匹配长度（字节数）。
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// 是否为零长度匹配。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// 编译后的执行单元。所有执行策略都实现此 trait。
pub trait CompiledUnit: Send + Sync {
    /// 判断输入中是否存在匹配。
    fn is_match(&self, input: &[u8]) -> bool;

    /// 查找第一个匹配区间。
    fn find_first(&self, input: &[u8]) -> Option<Match>;

    /// 查找所有匹配区间（非重叠，左到右）。
    fn find_all(&self, input: &[u8]) -> Vec<Match>;

    /// 执行替换，返回替换后的新字节序列。
    fn replace(&self, input: &[u8], replacement: &[u8]) -> Vec<u8>;
}

/// 引擎加载器。从 `.tvm` 字节数据中加载 `CompiledUnit`。
pub struct EngineLoader;

impl EngineLoader {
    /// 从 `.tvm` 字节数据中加载编译后的执行单元。
    pub fn load(engine_bytes: &[u8]) -> Result<Box<dyn CompiledUnit>, TvmHeaderError> {
        Self::create_unit(engine_bytes)
    }

    fn create_unit(engine_bytes: &[u8]) -> Result<Box<dyn CompiledUnit>, TvmHeaderError> {
        let header = TvmHeader::decode(engine_bytes)?;
        let encoding = TextEncoding::from_u8(header.encoding).unwrap_or(TextEncoding::Utf8);

        if header.flags & TvmFlags::IS_LITERAL != 0 {
            return Ok(Box::new(create_literal_executor(engine_bytes, &header, encoding)));
        }

        if header.flags & TvmFlags::IS_NFA != 0 {
            return Ok(Box::new(create_nfa_executor(engine_bytes, &header, encoding)));
        }

        Ok(Box::new(create_dfa_executor(engine_bytes, &header, encoding)))
    }
}

fn create_literal_executor(engine_bytes: &[u8], header: &TvmHeader, encoding: TextEncoding) -> LiteralExecutor {
    let pattern_offset = header.literal_prefix_offset as usize;
    let pattern_len = header.min_match_len as usize;
    let pattern = engine_bytes[pattern_offset..pattern_offset + pattern_len].to_vec();
    LiteralExecutor::new(pattern, encoding)
}

fn create_nfa_executor(engine_bytes: &[u8], header: &TvmHeader, encoding: TextEncoding) -> BacktrackExecutor {
    let nfa_offset = header.dfa_table_offset as usize;
    let _nfa_data = &engine_bytes[nfa_offset..];
    let _ = encoding;
    BacktrackExecutor::new(vec![], 0, 100)
}

fn create_dfa_executor(engine_bytes: &[u8], header: &TvmHeader, encoding: TextEncoding) -> DfaExecutor {
    let dfa_offset = header.dfa_table_offset as usize;
    let dfa_data = engine_bytes[dfa_offset..].to_vec();
    DfaExecutor::new(dfa_data, encoding)
}
