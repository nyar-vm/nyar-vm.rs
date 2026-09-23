//! 响应式类型（signal / stream / future / promise）名义类型注册。

use crate::types::Identifier;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReactiveTypeKind {
    Signal,
    Stream,
    Channel,
    Future,
    Promise,
    Observable,
}

/// 响应式包装类型描述。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReactiveType {
    pub kind: ReactiveTypeKind,
    pub element: Identifier,
}

impl ReactiveType {
    pub fn signal(element: Identifier) -> Self {
        Self { kind: ReactiveTypeKind::Signal, element }
    }

    pub fn future(element: Identifier) -> Self {
        Self { kind: ReactiveTypeKind::Future, element }
    }
}
