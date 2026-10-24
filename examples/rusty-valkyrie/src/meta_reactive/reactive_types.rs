//! 响应式类型（signal / stream / future / promise）名义类型注册。

use crate::types::Identifier;

/// 响应式包装类型种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReactiveTypeKind {
    /// 信号类型。
    Signal,
    /// 流类型。
    Stream,
    /// 通道类型。
    Channel,
    /// 未来值类型。
    Future,
    /// Promise 类型。
    Promise,
    /// 可观察序列类型。
    Observable,
}

/// 响应式包装类型描述。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReactiveType {
    /// 响应式包装种类。
    pub kind: ReactiveTypeKind,
    /// 元素类型名。
    pub element: Identifier,
}

impl ReactiveType {
    /// 构造 `Signal<T>` 描述。
    pub fn signal(element: Identifier) -> Self {
        Self { kind: ReactiveTypeKind::Signal, element }
    }

    /// 构造 `Future<T>` 描述。
    pub fn future(element: Identifier) -> Self {
        Self { kind: ReactiveTypeKind::Future, element }
    }
}
