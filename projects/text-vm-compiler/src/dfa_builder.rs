use crate::parser::ParsedPattern;

/// Brzozowski 导数 DFA 构造器占位实现。
pub struct DfaBuilder {
    pattern: ParsedPattern,
}

impl DfaBuilder {
    /// 使用给定模式创建 DFA 构造器。
    pub fn new(pattern: ParsedPattern) -> Self {
        Self { pattern }
    }

    /// 构建 DFA（占位，尚未实现）。
    pub fn build(&mut self) {
        let _ = &self.pattern;
    }

    /// 序列化 DFA 表（占位，返回空表）。
    pub fn serialize(&self) -> Vec<u8> {
        Vec::new()
    }

    /// DFA 状态数量。
    pub fn state_count(&self) -> u32 {
        0
    }
}
