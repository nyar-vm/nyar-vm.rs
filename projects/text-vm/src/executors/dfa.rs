use crate::{
    encoding::TextEncoding,
    engine::{CompiledUnit, Match},
};

/// DFA 执行器占位实现。
pub struct DfaExecutor {
    #[allow(dead_code)]
    dfa_data: Vec<u8>,
    #[allow(dead_code)]
    encoding: TextEncoding,
}

impl DfaExecutor {
    /// 使用给定的序列化 DFA 表字节数据和编码创建 DFA 执行器。
    pub fn new(dfa_data: Vec<u8>, encoding: TextEncoding) -> Self {
        Self { dfa_data, encoding }
    }
}

impl CompiledUnit for DfaExecutor {
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
