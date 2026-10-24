//! 属性修饰符与 setter 校验检查。

use crate::types::{
    Identifier, SourceSpan,
    hir::{HirModule, HirProperty, HirStruct},
};
use std::fmt;

/// 属性声明违规的类别。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyErrorKind {
    /// 属性同时被声明为 virtual 与 static。
    VirtualStaticConflict {
        /// 涉事属性名。
        property: Identifier,
    },
    /// static 属性引用了 self。
    StaticWithSelf {
        /// 涉事属性名。
        property: Identifier,
    },
    /// lazy 属性定义了 setter。
    LazyPropertyWithSetter {
        /// 涉事属性名。
        property: Identifier,
    },
    /// 无父类却声明为 override。
    InvalidOverride {
        /// 属性所属类名。
        class_name: Identifier,
        /// 涉事属性名。
        property: Identifier,
    },
    /// 抽象属性带具体实现体。
    AbstractPropertyWithBody {
        /// 属性所属类名。
        class_name: Identifier,
        /// 涉事属性名。
        property: Identifier,
    },
}

/// 单条属性违规：类别、渲染后的消息与源码位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyError {
    /// 违规类别。
    pub kind: PropertyErrorKind,
    /// 人类可读的错误信息。
    pub message: String,
    /// 产生该违规的源码位置；不可定位时为 None。
    pub span: Option<SourceSpan>,
}

impl PropertyError {
    /// 构造「virtual 与 static 冲突」错误。
    pub fn virtual_static_conflict(property: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: PropertyErrorKind::VirtualStaticConflict { property: property.clone() },
            message: format!("property {} cannot be both virtual and static", property),
            span,
        }
    }

    /// 构造「static 属性引用 self」错误。
    pub fn static_with_self(property: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: PropertyErrorKind::StaticWithSelf { property: property.clone() },
            message: format!("static property {} cannot use self", property),
            span,
        }
    }

    /// 构造「lazy 属性带 setter」错误。
    pub fn lazy_with_setter(property: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: PropertyErrorKind::LazyPropertyWithSetter { property: property.clone() },
            message: format!("lazy property {} cannot define a setter", property),
            span,
        }
    }

    /// 构造「无父类的 override」错误。
    pub fn invalid_override(class_name: Identifier, property: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: PropertyErrorKind::InvalidOverride { class_name: class_name.clone(), property: property.clone() },
            message: format!("property {} in {} cannot override without a parent", property, class_name),
            span,
        }
    }

    /// 构造「抽象属性带实现体」错误。
    pub fn abstract_property_with_body(class_name: Identifier, property: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: PropertyErrorKind::AbstractPropertyWithBody { class_name: class_name.clone(), property: property.clone() },
            message: format!("abstract property {} in {} cannot have a concrete body", property, class_name),
            span,
        }
    }
}

impl fmt::Display for PropertyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for PropertyError {}

/// 遍历类定义、校验属性修饰符组合的检查器。
#[derive(Debug, Default)]
pub struct PropertyChecker {
    errors: Vec<PropertyError>,
}

impl PropertyChecker {
    /// 创建空的属性检查器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 检查模块内所有类的属性声明并返回违规列表。
    pub fn check_module(&mut self, module: &HirModule) -> Vec<PropertyError> {
        self.errors.clear();
        for class in &module.structs {
            for property in &class.properties {
                self.check_property(class, property);
            }
        }
        self.errors.clone()
    }

    fn check_property(&mut self, class: &HirStruct, property: &HirProperty) {
        if property.is_static && property.is_virtual {
            self.errors.push(PropertyError::virtual_static_conflict(property.name.clone(), None));
        }
        if property.is_lazy && property.setter.is_some() {
            self.errors.push(PropertyError::lazy_with_setter(property.name.clone(), None));
        }
        if property.is_override && class.parents.is_empty() {
            self.errors.push(PropertyError::invalid_override(class.name.clone(), property.name.clone(), None));
        }
        if property.is_abstract && !class.is_abstract {
            self.errors.push(PropertyError::abstract_property_with_body(class.name.clone(), property.name.clone(), None));
        }
    }
}

/// 为属性 setter 生成校验片段的分析器。
#[derive(Debug, Default)]
pub struct SetterValidationAnalyzer;

impl SetterValidationAnalyzer {
    /// 创建 setter 校验分析器。
    pub fn new() -> Self {
        Self
    }

    /// 为给定属性生成 setter 校验描述列表。
    pub fn analyze(&mut self, property: &HirProperty) -> Vec<String> {
        vec![format!("validate setter for {}", property.name)]
    }

    /// 按条件与属性名生成取值校验的 panic 代码片段。
    pub fn generate_panic_code(property_name: &str, condition: &str) -> String {
        format!("if !({}) {{ panic!(\"invalid setter value for {}\"); }}", condition, property_name)
    }
}
