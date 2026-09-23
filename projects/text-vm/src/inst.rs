/// 回溯虚拟机指令类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum InstKind {
    /// 匹配指定字符。
    Char,
    /// 匹配任意码点。
    Any,
    /// 字符类匹配。
    CharClass,
    /// 分支：先尝试左分支，失败则尝试右分支。
    Split,
    /// 保存当前字节偏移到捕获槽。
    Save,
    /// 无条件跳转。
    Jump,
    /// 匹配成功。
    Match,
    /// 匹配失败。
    Fail,
    /// 反向引用：匹配之前捕获的相同内容。
    Backref,
    /// 锚点断言。
    Anchor,
}

/// 字符范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CharRange {
    /// 范围起始（含）。
    pub lo: u32,
    /// 范围结束（含）。
    pub hi: u32,
}

/// 回溯虚拟机指令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inst {
    /// 指令类型。
    pub kind: InstKind,
    /// `Char` 指令的字符值 / `Anchor` 的锚点类型。
    pub int_arg1: i32,
    /// `Split` 的右分支 / `Jump` 的目标。
    pub int_arg2: i32,
    /// 字符类指令的区间数组。
    pub ranges: Vec<CharRange>,
    /// 字符类是否取反。
    pub negated: bool,
}

impl Inst {
    /// 创建字符匹配指令。
    pub fn char(ch: u32) -> Self {
        Self { kind: InstKind::Char, int_arg1: ch as i32, int_arg2: 0, ranges: Vec::new(), negated: false }
    }

    /// 任意字符匹配指令。
    pub fn any() -> Self {
        Self { kind: InstKind::Any, int_arg1: 0, int_arg2: 0, ranges: Vec::new(), negated: false }
    }

    /// 匹配成功指令。
    pub fn success() -> Self {
        Self { kind: InstKind::Match, int_arg1: 0, int_arg2: 0, ranges: Vec::new(), negated: false }
    }

    /// 匹配失败指令。
    pub fn failure() -> Self {
        Self { kind: InstKind::Fail, int_arg1: 0, int_arg2: 0, ranges: Vec::new(), negated: false }
    }
}
