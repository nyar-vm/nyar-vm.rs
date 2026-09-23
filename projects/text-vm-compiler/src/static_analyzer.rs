use crate::parser::ParsedPattern;

/// 模式复杂度等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Complexity {
    /// 纯字面量。
    Literal,
    /// 正则（DFA 可处理）。
    Regular,
    /// 上下文无关（含反向引用等）。
    ContextFree,
}

/// 对解析后的模式执行静态分析并返回复杂度。
pub fn analyze(pattern: &ParsedPattern) -> Complexity {
    // 当前解析器只产生字面量，因此始终路由到 Literal 执行器。
    let _ = pattern;
    Complexity::Literal
}
