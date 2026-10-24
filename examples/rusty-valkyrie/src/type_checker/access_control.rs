//! 访问级别与成员可见性检查。

use super::path_helpers::display_path;
use crate::types::{
    Identifier, NamePath, SourceSpan,
    hir::{AccessLevel, HirStruct},
};
use std::{collections::BTreeMap, fmt};

/// 成员可访问性判定的上下文：当前模块、所在类与方法。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessContext {
    /// 当前代码所在的模块路径。
    pub current_module: NamePath,
    /// 当前代码所在的类；位于模块级时为 None。
    pub current_class: Option<Identifier>,
    /// 当前代码所在的方法；不在方法体内时为 None。
    pub current_method: Option<Identifier>,
}

impl AccessContext {
    /// 构造模块级（不在任何类中）的访问上下文。
    pub fn new(current_module: NamePath) -> Self {
        Self { current_module, current_class: None, current_method: None }
    }

    /// 构造位于类体、但不在具体方法内的访问上下文。
    pub fn in_class(current_module: NamePath, current_class: Identifier) -> Self {
        Self { current_module, current_class: Some(current_class), current_method: None }
    }

    /// 构造位于类方法体内的访问上下文。
    pub fn in_method(current_module: NamePath, current_class: Identifier, current_method: Identifier) -> Self {
        Self { current_module, current_class: Some(current_class), current_method: Some(current_method) }
    }
}

/// 成员访问违规的类别，携带定位所需的名字与可见性信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessControlErrorKind {
    /// 从类外访问了 private 成员。
    PrivateMemberAccess {
        /// 成员所属类型。
        owner: Identifier,
        /// 被访问的成员名。
        member: Identifier,
        /// 发起访问的类；模块级访问为 None。
        accessor: Option<Identifier>,
    },
    /// 从非子类访问了 protected 成员。
    ProtectedMemberAccess {
        /// 成员所属类型。
        owner: Identifier,
        /// 被访问的成员名。
        member: Identifier,
        /// 发起访问的类。
        accessor: Identifier,
    },
    /// 跨模块访问了 internal 成员。
    InternalMemberAccess {
        /// 被访问的成员名。
        member: Identifier,
        /// 成员声明的模块路径。
        declared_module: NamePath,
        /// 发起访问的模块路径。
        current_module: NamePath,
    },
    /// 对 readonly 字段执行了写入。
    ReadonlyFieldWrite {
        /// 字段所属类型。
        owner: Identifier,
        /// 被写入的字段名。
        field: Identifier,
    },
    /// 实例化了构造器为 private 的类。
    PrivateConstructorInstantiation {
        /// 被实例化的类名。
        class_name: Identifier,
    },
    /// 子类成员降低了父类成员的可见级别。
    VisibilityReduction {
        /// 声明成员的类名。
        class_name: Identifier,
        /// 被降低可见性的成员名。
        member: Identifier,
        /// 父类侧原有的可见级别。
        from: AccessLevel,
        /// 子类侧改写后的可见级别。
        to: AccessLevel,
    },
}

/// 单条成员访问违规：类别、渲染后的消息与源码位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessControlError {
    /// 违规类别。
    pub kind: AccessControlErrorKind,
    /// 人类可读的错误信息。
    pub message: String,
    /// 产生该违规的源码位置；不可定位时为 None。
    pub span: Option<SourceSpan>,
}

impl AccessControlError {
    /// 构造「类外访问 private 成员」错误。
    pub fn private_member_access(owner: Identifier, member: Identifier, accessor: Option<Identifier>, span: Option<SourceSpan>) -> Self {
        let accessor_text = accessor.as_ref().map(|name| format!(" by {}", name)).unwrap_or_default();
        Self {
            kind: AccessControlErrorKind::PrivateMemberAccess { owner: owner.clone(), member: member.clone(), accessor },
            message: format!("private member access: {}.{}{}", owner, member, accessor_text),
            span,
        }
    }

    /// 构造「非子类访问 protected 成员」错误。
    pub fn protected_member_access(owner: Identifier, member: Identifier, accessor: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: AccessControlErrorKind::ProtectedMemberAccess { owner: owner.clone(), member: member.clone(), accessor: accessor.clone() },
            message: format!("protected member access denied: {}.{} from {}", owner, member, accessor),
            span,
        }
    }

    /// 构造「跨模块访问 internal 成员」错误。
    pub fn internal_member_access(member: Identifier, declared_module: NamePath, current_module: NamePath, span: Option<SourceSpan>) -> Self {
        Self {
            kind: AccessControlErrorKind::InternalMemberAccess {
                member: member.clone(),
                declared_module: declared_module.clone(),
                current_module: current_module.clone(),
            },
            message: format!(
                "internal member access denied: {} declared in {} from {}",
                member,
                display_path(&declared_module),
                display_path(&current_module)
            ),
            span,
        }
    }

    /// 构造「写入 readonly 字段」错误。
    pub fn readonly_field_write(owner: Identifier, field: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: AccessControlErrorKind::ReadonlyFieldWrite { owner: owner.clone(), field: field.clone() },
            message: format!("readonly field write denied: {}.{}", owner, field),
            span,
        }
    }

    /// 构造「实例化 private 构造器」错误。
    pub fn private_constructor_instantiation(class_name: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: AccessControlErrorKind::PrivateConstructorInstantiation { class_name: class_name.clone() },
            message: format!("private constructor instantiation denied: {} requires a factory method", class_name),
            span,
        }
    }

    /// 构造「子类降低父类成员可见性」错误。
    pub fn visibility_reduction(
        class_name: Identifier,
        member: Identifier,
        from: AccessLevel,
        to: AccessLevel,
        span: Option<SourceSpan>,
    ) -> Self {
        Self {
            kind: AccessControlErrorKind::VisibilityReduction { class_name: class_name.clone(), member: member.clone(), from, to },
            message: format!("reduced visibility is not allowed: {}.{} from {} to {}", class_name, member, from.as_str(), to.as_str()),
            span,
        }
    }
}

impl fmt::Display for AccessControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AccessControlError {}

/// 收集类与继承关系并产出成员可见性违规的检查器。
#[derive(Debug, Default)]
pub struct AccessControlChecker {
    errors: Vec<AccessControlError>,
    classes: BTreeMap<Identifier, HirStruct>,
    inheritance_map: BTreeMap<Identifier, Vec<Identifier>>,
}

impl AccessControlChecker {
    /// 创建空的访问控制检查器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 返回已收集的违规列表。
    pub fn errors(&self) -> &[AccessControlError] {
        &self.errors
    }

    /// 返回按类名索引的类定义表。
    pub fn classes(&self) -> &BTreeMap<Identifier, HirStruct> {
        &self.classes
    }

    /// 返回「类名 → 直接父类名」的继承关系表。
    pub fn inheritance_map(&self) -> &BTreeMap<Identifier, Vec<Identifier>> {
        &self.inheritance_map
    }
}
