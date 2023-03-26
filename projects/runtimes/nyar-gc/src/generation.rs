//! 分代标签与 nursery 记账（第一版：标签 + 晋升，无物理分区拷贝）。

/// 对象所属代。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Generation {
    /// 新分配对象；nursery 回收的主要目标。
    #[default]
    Nursery,
    /// 已晋升或显式分配到老年代。
    Tenured,
}
