//! JIT 代码假设：失效时必须丢弃依赖该假设的机器码缓存。

/// 一条可失效的编译假设。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JitAssumption {
    /// 基线标量叶形态（NJ1 blob 形状合同）。
    ScalarLeafShape,
    /// 依赖模块版本号（外码字节身份）。
    ModuleVersion(u32),
}

impl JitAssumption {
    /// 人类可读标签（日志 / 测试）。
    pub fn label(self) -> &'static str {
        match self {
            Self::ScalarLeafShape => "scalar-leaf-shape",
            Self::ModuleVersion(_) => "module-version",
        }
    }
}

/// 基线标量 JIT 默认挂载的假设集。
pub fn baseline_scalar_assumptions(module_version: u32) -> Vec<JitAssumption> {
    vec![JitAssumption::ScalarLeafShape, JitAssumption::ModuleVersion(module_version)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_assumptions_include_version() {
        let list = baseline_scalar_assumptions(3);
        assert!(list.contains(&JitAssumption::ScalarLeafShape));
        assert!(list.contains(&JitAssumption::ModuleVersion(3)));
    }
}
