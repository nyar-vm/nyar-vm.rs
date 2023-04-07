//! GC stack map：每个 safepoint 上解释器/机器码须提供的根槽描述。
//!
//! 第一版为保守地图：全部 local 槽视为可能含引用。未来可由验证器栈高与
//! 布局位图收紧；deopt map 是独立结构，不得与本表混为一谈。

/// 单个 safepoint 的根描述。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackMapEntry {
    /// 函数内指令下标（与内码 `ExecOp` / safepoint 侧表对齐）。
    pub instruction_index: u32,
    /// 可能持有托管引用的 local 槽下标（0-based）。
    pub local_root_slots: Vec<u16>,
    /// 操作数栈上从栈底起可能含引用的槽数；`None` 表示“整栈皆根”（解释器默认）。
    pub operand_root_depth: Option<u16>,
}

/// 一个函数在全部 safepoint 上的 stack map 集合。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FunctionStackMaps {
    /// 模块内函数下标。
    pub function_index: usize,
    /// 按指令下标升序的条目。
    pub entries: Vec<StackMapEntry>,
}

impl FunctionStackMaps {
    /// 空表。
    pub fn empty(function_index: usize) -> Self {
        Self { function_index, entries: Vec::new() }
    }

    /// 查找某指令下标的条目。
    pub fn entry_at(&self, instruction_index: u32) -> Option<&StackMapEntry> {
        self.entries.iter().find(|entry| entry.instruction_index == instruction_index)
    }
}

/// 由 safepoint 下标与 local 数量构造保守 stack map（全部 local 为根，整栈为根）。
pub fn build_conservative_stack_maps(
    function_index: usize,
    local_count: i32,
    safepoint_indices: &[u32],
) -> FunctionStackMaps {
    let local_count = local_count.max(0) as u16;
    let local_root_slots: Vec<u16> = (0..local_count).collect();
    let mut indices = safepoint_indices.to_vec();
    indices.sort_unstable();
    indices.dedup();
    let entries = indices
        .into_iter()
        .map(|instruction_index| StackMapEntry {
            instruction_index,
            local_root_slots: local_root_slots.clone(),
            operand_root_depth: None,
        })
        .collect();
    FunctionStackMaps { function_index, entries }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conservative_maps_cover_all_locals_at_each_safepoint() {
        let maps = build_conservative_stack_maps(3, 4, &[0, 2, 2, 5]);
        assert_eq!(maps.function_index, 3);
        assert_eq!(maps.entries.len(), 3);
        assert_eq!(maps.entries[0].local_root_slots, vec![0, 1, 2, 3]);
        assert!(maps.entry_at(2).is_some());
        assert!(maps.entry_at(1).is_none());
    }
}
