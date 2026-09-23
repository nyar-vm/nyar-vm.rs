/// Witness table 方法槽（driver 层载荷，非 COM vtable）。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WitnessMethodSlotSubmission {
    /// Trait 方法名。
    pub method_name: String,
    /// 实现函数在目标镜像中的符号标签。
    pub impl_symbol: String,
    /// 方法在见证表中的槽位索引。
    pub method_index: u32,
}

/// 单个 `imply Type: Trait` 的见证表提交载荷。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WitnessSubmission {
    /// 实现类型名。
    pub type_name: String,
    /// Trait 名。
    pub trait_name: String,
    /// `.rodata` / 线性内存中的见证表标签。
    pub table_label: String,
    /// 胖指针 `(data, witness_table)` 标签。
    pub fat_ptr_label: String,
    /// 方法槽列表。
    pub methods: Vec<WitnessMethodSlotSubmission>,
    /// 实现方法返回的字符串字面量（本轮 demo：`make_sound` → `"woof"`）。
    pub result_literal: String,
}

/// 入口处的 witness 动态调用边。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WitnessCallEdge {
    /// 调用的 trait 名。
    pub trait_name: String,
    /// 实现类型名。
    pub type_name: String,
    /// 见证表方法索引。
    pub method_index: u32,
    /// 调用后是否通过宿主 print/write 输出返回值。
    pub print_result: bool,
}
