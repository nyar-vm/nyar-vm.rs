use crate::{
    encoding::TextEncoding,
    engine::{CompiledUnit, Match},
    inst::Inst,
};

/// 基于回溯的虚拟机执行器占位实现。
pub struct BacktrackExecutor {
    #[allow(dead_code)]
    program: Vec<Inst>,
    #[allow(dead_code)]
    capture_count: usize,
    #[allow(dead_code)]
    backtrack_timeout_ms: u32,
}

impl BacktrackExecutor {
    /// 创建回溯执行器。
    pub fn new(program: Vec<Inst>, capture_count: usize, backtrack_timeout_ms: u32) -> Self {
        Self { program, capture_count, backtrack_timeout_ms }
    }
}

impl CompiledUnit for BacktrackExecutor {
    fn is_match(&self, _input: &[u8]) -> bool {
        false
    }

    fn find_first(&self, _input: &[u8]) -> Option<Match> {
        None
    }

    fn find_all(&self, _input: &[u8]) -> Vec<Match> {
        Vec::new()
    }

    fn replace(&self, input: &[u8], _replacement: &[u8]) -> Vec<u8> {
        input.to_vec()
    }
}

#[allow(dead_code)]
const DEFAULT_ENCODING: TextEncoding = TextEncoding::Utf8;
