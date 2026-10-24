//! 构造器链与 `super` 调用检查。

use crate::types::{Identifier, SourceSpan, hir::ValkyrieType as HirType};
use std::fmt;

/// 构造器链违规的类别，覆盖缺失、重复、顺序与实参不匹配等情形。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstructorChainErrorKind {
    /// 子类构造器缺少对父类的 super 调用。
    MissingSuperCall {
        /// 子类名。
        class_name: Identifier,
        /// 未被调用的父类名。
        parent: Identifier,
    },
    /// super 调用的实参个数与父类构造器不符。
    SuperCallArgumentMismatch {
        /// 子类名。
        class_name: Identifier,
        /// 被调用的父类名。
        parent: Identifier,
        /// 父类构造器要求的实参个数。
        expected: usize,
        /// 实际传入的实参个数。
        got: usize,
    },
    /// 多个父类的 super 调用顺序不符合 MRO 要求。
    InvalidSuperCallOrder {
        /// 子类名。
        class_name: Identifier,
        /// 期望的 super 调用顺序。
        expected: Vec<Identifier>,
        /// 实际的 super 调用顺序。
        actual: Vec<Identifier>,
    },
    /// 对同一父类重复发起 super 调用。
    DuplicateSuperCall {
        /// 子类名。
        class_name: Identifier,
        /// 被重复调用的父类名。
        parent: Identifier,
    },
    /// 在 `initiate` 之外的方法中调用 super。
    InvalidSuperCallMethod {
        /// 子类名。
        class_name: Identifier,
        /// 出现 super 调用的方法名。
        method: Identifier,
    },
    /// super 调用某个实参的类型与父类构造器形参不符。
    SuperCallArgumentTypeMismatch {
        /// 子类名。
        class_name: Identifier,
        /// 被调用的父类名。
        parent: Identifier,
        /// 不匹配的实参位置。
        index: usize,
        /// 父类构造器声明的形参类型。
        expected: HirType,
        /// 实际传入的实参类型。
        got: HirType,
    },
}

/// 单条构造器链违规：类别、渲染后的消息与源码位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstructorChainError {
    /// 违规类别。
    pub kind: ConstructorChainErrorKind,
    /// 人类可读的错误信息。
    pub message: String,
    /// 产生该违规的源码位置；不可定位时为 None。
    pub span: Option<SourceSpan>,
}

impl ConstructorChainError {
    /// 构造「缺少 super 调用」错误。
    pub fn missing_super_call(class_name: Identifier, parent: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: ConstructorChainErrorKind::MissingSuperCall { class_name: class_name.clone(), parent: parent.clone() },
            message: format!("{} is missing required super call to {}", class_name, parent),
            span,
        }
    }

    /// 构造「super 调用实参个数不符」错误。
    pub fn super_call_argument_mismatch(
        class_name: Identifier,
        parent: Identifier,
        expected: usize,
        got: usize,
        span: Option<SourceSpan>,
    ) -> Self {
        Self {
            kind: ConstructorChainErrorKind::SuperCallArgumentMismatch {
                class_name: class_name.clone(),
                parent: parent.clone(),
                expected,
                got,
            },
            message: format!("Expected {} arguments for {} super call in {}, got {}", expected, parent, class_name, got),
            span,
        }
    }

    /// 构造「super 调用顺序不符合 MRO」错误。
    pub fn invalid_super_call_order(
        class_name: Identifier,
        expected: Vec<Identifier>,
        actual: Vec<Identifier>,
        span: Option<SourceSpan>,
    ) -> Self {
        Self {
            kind: ConstructorChainErrorKind::InvalidSuperCallOrder {
                class_name: class_name.clone(),
                expected: expected.clone(),
                actual: actual.clone(),
            },
            message: format!("Invalid MRO super call order in {}", class_name),
            span,
        }
    }

    /// 构造「重复 super 调用」错误。
    pub fn duplicate_super_call(class_name: Identifier, parent: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: ConstructorChainErrorKind::DuplicateSuperCall { class_name: class_name.clone(), parent: parent.clone() },
            message: format!("Duplicate super call to {} in {}", parent, class_name),
            span,
        }
    }

    /// 构造「super 调用出现在非 `initiate` 方法」错误。
    pub fn invalid_super_call_method(class_name: Identifier, method: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: ConstructorChainErrorKind::InvalidSuperCallMethod { class_name: class_name.clone(), method: method.clone() },
            message: format!("{} may only call super from initiate, not {}", class_name, method),
            span,
        }
    }

    /// 构造「super 调用实参类型不符」错误。
    pub fn super_call_argument_type_mismatch(
        class_name: Identifier,
        parent: Identifier,
        index: usize,
        expected: HirType,
        got: HirType,
        span: Option<SourceSpan>,
    ) -> Self {
        Self {
            kind: ConstructorChainErrorKind::SuperCallArgumentTypeMismatch {
                class_name: class_name.clone(),
                parent: parent.clone(),
                index,
                expected: expected.clone(),
                got: got.clone(),
            },
            message: format!(
                "super call argument type mismatch in {} -> {} at {}: expected {:?}, got {:?}",
                class_name, parent, index, expected, got
            ),
            span,
        }
    }
}

impl fmt::Display for ConstructorChainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ConstructorChainError {}

/// 收集构造器链违规的检查器。
#[derive(Debug, Default)]
pub struct ConstructorChainChecker {
    errors: Vec<ConstructorChainError>,
}

impl ConstructorChainChecker {
    /// 创建空的构造器链检查器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 返回已收集的违规列表。
    pub fn errors(&self) -> &[ConstructorChainError] {
        &self.errors
    }
}
