//! 对象布局描述：哪些槽可能持有托管引用。
//!
//! 外码 `NyarLayout` 只提供 `field_count`；加载后由 VM 解析为本描述符供 GC / JIT 共享。
//! 第一版默认全部槽位为 tagged [`crate::Value`]；日后可接入引用位图而不改外码合同。

/// 运行时布局标识（进程内，可带模块身份映射后的稠密下标）。
pub type LayoutId = u32;

/// 对象布局的 GC 可见描述。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutDescriptor {
    /// 与外码 / 模块 layouts 表对应的标识。
    pub layout_id: LayoutId,
    /// 值槽数量。
    pub field_count: u32,
    /// 若为 `None`，所有槽均按可能含引用处理（保守）。
    /// 若为 `Some`，长度为 `field_count`，`true` 表示该槽可能含托管引用。
    pub reference_slots: Option<Vec<bool>>,
}

impl LayoutDescriptor {
    /// 构造「全部槽位可能含引用」的保守布局。
    pub fn all_references(layout_id: LayoutId, field_count: u32) -> Self {
        Self { layout_id, field_count, reference_slots: None }
    }

    /// 指定引用位图；每一位对应一个字段槽。
    pub fn with_reference_slots(layout_id: LayoutId, reference_slots: Vec<bool>) -> Self {
        let field_count = reference_slots.len() as u32;
        Self { layout_id, field_count, reference_slots: Some(reference_slots) }
    }

    /// 槽 `index` 是否可能含托管引用。
    pub fn slot_may_contain_reference(&self, index: u32) -> bool {
        if index >= self.field_count {
            return false;
        }
        match &self.reference_slots {
            None => true,
            Some(bits) => bits.get(index as usize).copied().unwrap_or(true),
        }
    }
}
