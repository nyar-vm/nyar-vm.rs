//! 去优化（deopt）元数据：与 GC [`crate::stack_map`] 分离。
//!
//! stack map 只描述托管根；deopt map 描述恢复解释器帧所需的位置与物化配方占位。
//! 第一版仅保存 safepoint → 逻辑帧链，并提供无机器码的解释器帧物化合同。
//! 寄存器/常量物化表达式仍为后续能力。

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

/// 已物化为解释器可恢复形状的一帧（无机器寄存器）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoredInterpreterFrame {
    /// 模块函数下标。
    pub function_index: usize,
    /// 恢复后的内码指令下标。
    pub instruction_index: u32,
    /// local 槽（长度等于 [`DeoptFrame::local_count`]；缺省填空槽标记）。
    pub locals: Vec<RestoredLocal>,
}

/// 物化后的 local 槽内容（JIT 包不依赖 `nyar-gc::Value`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoredLocal {
    /// 未提供物化值：解释器应以 `Null` 填槽。
    Absent,
    /// 由调用方提供的不透明槽载荷（通常为序列化后的 Value 字节或句柄）。
    Provided(Vec<u8>),
}

/// 物化失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeoptRestoreError {
    /// 条目不含任何帧。
    EmptyFrameChain,
    /// 提供的 locals 组数与帧链长度不一致。
    FrameCountMismatch {
        /// 条目中的帧数。
        expected: usize,
        /// 调用方提供的组数。
        actual: usize,
    },
    /// 某一帧提供的 local 数量超过 `local_count`。
    LocalOverflow {
        /// 帧在链中的下标。
        frame: usize,
        /// 合同声明的 local 槽数。
        local_count: u16,
        /// 调用方提供的条目数。
        provided: usize,
    },
}

impl std::fmt::Display for DeoptRestoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyFrameChain => write!(f, "deopt entry has empty frame chain"),
            Self::FrameCountMismatch { expected, actual } => {
                write!(f, "deopt frame count mismatch: expected {expected}, got {actual}")
            }
            Self::LocalOverflow { frame, local_count, provided } => {
                write!(
                    f,
                    "deopt frame {frame}: provided {provided} locals but local_count is {local_count}"
                )
            }
        }
    }
}

impl std::error::Error for DeoptRestoreError {}

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

/// 内联帧链中的一层规格（索引 0 为最内层）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineFrameSpec {
    /// 模块函数下标。
    pub function_index: usize,
    /// local 槽数量。
    pub local_count: u16,
    /// 恢复后的内码下标。
    ///
    /// 最内层可填 `None`：表示对该 safepoint 使用 `safepoint + 1`。
    /// 外层必须给出调用返回点（或其它约定恢复点）。
    pub resume_instruction: Option<u32>,
}

/// 由 safepoint 列表构造最小 deopt 表（每点一帧，无内联）。
pub fn build_baseline_deopt_map(
    function_index: usize,
    local_count: i32,
    safepoint_indices: &[u32],
) -> DeoptMap {
    build_inline_deopt_map(
        function_index,
        &[InlineFrameSpec {
            function_index,
            local_count: local_count.max(0) as u16,
            resume_instruction: None,
        }],
        safepoint_indices,
    )
}

/// 由内联帧链规格构造 deopt 表（每 safepoint 共享同一逻辑帧链形状）。
///
/// `frame_chain[0]` 为最内层。空链得到空 `entries` 的表（仍记录 `owning_function_index`）。
pub fn build_inline_deopt_map(
    owning_function_index: usize,
    frame_chain: &[InlineFrameSpec],
    safepoint_indices: &[u32],
) -> DeoptMap {
    let mut indices = safepoint_indices.to_vec();
    indices.sort_unstable();
    indices.dedup();
    if frame_chain.is_empty() {
        return DeoptMap {
            function_index: owning_function_index,
            entries: Vec::new(),
        };
    }
    let entries = indices
        .into_iter()
        .map(|safepoint| {
            let frames = frame_chain
                .iter()
                .enumerate()
                .map(|(depth, spec)| {
                    let instruction_index = if depth == 0 {
                        spec.resume_instruction
                            .unwrap_or_else(|| safepoint.saturating_add(1))
                    } else {
                        spec.resume_instruction.unwrap_or(0)
                    };
                    DeoptFrame {
                        function_index: spec.function_index,
                        instruction_index,
                        local_count: spec.local_count,
                    }
                })
                .collect();
            DeoptMapEntry {
                instruction_index: safepoint,
                frames,
            }
        })
        .collect();
    DeoptMap {
        function_index: owning_function_index,
        entries,
    }
}

/// 将 deopt 条目物化为解释器帧链（无机器码、无寄存器分配）。
///
/// `provided_locals[i]` 对应 `entry.frames[i]`：长度可小于 `local_count`（其余 `Absent`），
/// 不可更长。传入与帧数等长的空切片列表表示全部填 `Absent`。
pub fn materialize_interpreter_frames(
    entry: &DeoptMapEntry,
    provided_locals: &[Vec<Option<Vec<u8>>>],
) -> Result<Vec<RestoredInterpreterFrame>, DeoptRestoreError> {
    if entry.frames.is_empty() {
        return Err(DeoptRestoreError::EmptyFrameChain);
    }
    if provided_locals.len() != entry.frames.len() {
        return Err(DeoptRestoreError::FrameCountMismatch {
            expected: entry.frames.len(),
            actual: provided_locals.len(),
        });
    }
    let mut restored = Vec::with_capacity(entry.frames.len());
    for (frame_index, (frame, provided)) in entry.frames.iter().zip(provided_locals.iter()).enumerate() {
        if provided.len() > frame.local_count as usize {
            return Err(DeoptRestoreError::LocalOverflow {
                frame: frame_index,
                local_count: frame.local_count,
                provided: provided.len(),
            });
        }
        let mut locals = Vec::with_capacity(frame.local_count as usize);
        for slot in 0..frame.local_count as usize {
            let cell = match provided.get(slot) {
                Some(Some(bytes)) => RestoredLocal::Provided(bytes.clone()),
                Some(None) | None => RestoredLocal::Absent,
            };
            locals.push(cell);
        }
        restored.push(RestoredInterpreterFrame {
            function_index: frame.function_index,
            instruction_index: frame.instruction_index,
            locals,
        });
    }
    Ok(restored)
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

    #[test]
    fn materialize_fills_absent_locals_to_local_count() {
        let map = build_baseline_deopt_map(0, 3, &[10]);
        let entry = map.entry_at(10).expect("entry");
        let frames = materialize_interpreter_frames(entry, &[vec![Some(vec![1, 2])]]).expect("ok");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].instruction_index, 11);
        assert_eq!(frames[0].locals.len(), 3);
        assert_eq!(frames[0].locals[0], RestoredLocal::Provided(vec![1, 2]));
        assert_eq!(frames[0].locals[1], RestoredLocal::Absent);
        assert_eq!(frames[0].locals[2], RestoredLocal::Absent);
    }

    #[test]
    fn materialize_rejects_local_overflow() {
        let map = build_baseline_deopt_map(0, 1, &[0]);
        let entry = map.entry_at(0).expect("entry");
        let err = materialize_interpreter_frames(entry, &[vec![None, None]]).expect_err("overflow");
        assert!(matches!(err, DeoptRestoreError::LocalOverflow { .. }));
    }

    #[test]
    fn inline_deopt_map_preserves_outer_resume_pcs() {
        let map = build_inline_deopt_map(
            1,
            &[
                InlineFrameSpec {
                    function_index: 1,
                    local_count: 2,
                    resume_instruction: None,
                },
                InlineFrameSpec {
                    function_index: 0,
                    local_count: 3,
                    resume_instruction: Some(99),
                },
            ],
            &[10],
        );
        let entry = map.entry_at(10).expect("entry");
        assert_eq!(entry.frames.len(), 2);
        assert_eq!(entry.frames[0].function_index, 1);
        assert_eq!(entry.frames[0].instruction_index, 11);
        assert_eq!(entry.frames[0].local_count, 2);
        assert_eq!(entry.frames[1].function_index, 0);
        assert_eq!(entry.frames[1].instruction_index, 99);
        assert_eq!(entry.frames[1].local_count, 3);
    }
}
