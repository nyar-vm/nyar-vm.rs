//! Stable RenderIR identifier newtypes.

/// Node identifier in canonical RenderIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RenderNodeId(pub u32);

/// Expression side-table identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RenderExprId(pub u32);

/// Event handler side-table identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RenderEventId(pub u32);

/// Binding side-table identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RenderBindingId(pub u32);

/// Ordered node list region identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RenderRegionId(pub u32);

impl RenderNodeId {
    /// 无效节点 id 哨兵值。
    pub const INVALID: Self = Self(u32::MAX);
}

impl RenderExprId {
    /// 无效表达式 id 哨兵值。
    pub const INVALID: Self = Self(u32::MAX);
}

impl RenderRegionId {
    /// 空区域 id 哨兵值；`region()` 对它返回共享空区域。
    pub const EMPTY: Self = Self(u32::MAX);

    /// Whether this region id refers to an allocated region table entry.
    pub fn is_valid(self) -> bool {
        self != Self::EMPTY
    }
}
