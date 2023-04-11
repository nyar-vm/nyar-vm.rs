//! 去优化（deopt）元数据：与 GC [`crate::stack_map`] 分离。
//!
//! stack map 只描述托管根；deopt map 描述恢复解释器帧所需的位置与物化配方占位。
//! 第一版仅保存 safepoint → 逻辑帧链，不含寄存器/常量物化表达式。

/// 去优化时需恢复的一层逻辑帧。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeoptFrame {
    /// 模块函数下标。
    pub function_index: usize,
    /// 恢复后的内码指令下标（通常为 safepoint 下一条）。
    pub instruction_index: u32,
    /// 该帧 local 槽数量。
    pub local_count: u16,
}

/// 单个 safepoint 的去优化描述。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeoptMapEntry {
    /// 与内码 safepoint / stack map 对齐的指令下标。
    pub instruction_index: u32,
    /// 内联帧链：索引 0 为最内层（当前执行函数）。
    pub frames: Vec<DeoptFrame>,
}

/// 一个函数全部 safepoint 的 deopt 表。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeoptMap {
    /// 模块内函数下标。
    pub function_index: usize,
    /// 按指令下标升序。
    pub entries: Vec<DeoptMapEntry>,
}

impl DeoptMap {
    /// 空表。
    pub fn empty(function_index: usize) -> Self {
        Self { function_index, entries: Vec::new() }
    }

    /// 查找条目。
    pub fn entry_at(&self, instruction_index: u32) -> Option<&DeoptMapEntry> {
        self.entries.iter().find(|entry| entry.instruction_index == instruction_index)
    }
}

/// 由 safepoint 列表构造最小 deopt 表（每点一帧，无内联）。
pub fn build_baseline_deopt_map(
    function_index: usize,
    local_count: i32,
    safepoint_indices: &[u32],
) -> DeoptMap {
    let local_count = local_count.max(0) as u16;
    let mut indices = safepoint_indices.to_vec();
    indices.sort_unstable();
    indices.dedup();
    let entries = indices
        .into_iter()
        .map(|instruction_index| DeoptMapEntry {
            instruction_index,
            frames: vec![DeoptFrame {
                function_index,
                instruction_index: instruction_index.saturating_add(1),
                local_count,
            }],
        })
        .collect();
    DeoptMap { function_index, entries }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_deopt_covers_each_safepoint() {
        let map = build_baseline_deopt_map(2, 3, &[1, 1, 4]);
        assert_eq!(map.function_index, 2);
        assert_eq!(map.entries.len(), 2);
        let entry = map.entry_at(1).expect("entry");
        assert_eq!(entry.frames.len(), 1);
        assert_eq!(entry.frames[0].instruction_index, 2);
        assert_eq!(entry.frames[0].local_count, 3);
    }
}
