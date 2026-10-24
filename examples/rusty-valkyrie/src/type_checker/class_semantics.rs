use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use crate::types::{
    Identifier, SourceSpan,
    hir::{HirModule, HirStruct},
};

use super::last_name;

/// final 类与 final 成员重写违规的类别。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinalClassErrorKind {
    /// 继承了 final 类。
    FinalClassInheritance {
        /// 子类名。
        class_name: Identifier,
        /// 被继承的 final 父类名。
        parent: Identifier,
    },
    /// 重写了父类的 final 方法。
    FinalMethodOverride {
        /// 子类名。
        class_name: Identifier,
        /// 被重写的方法名。
        method: Identifier,
        /// 声明该 final 方法的父类名。
        parent: Identifier,
    },
    /// 重写了父类的 final 属性。
    FinalPropertyOverride {
        /// 子类名。
        class_name: Identifier,
        /// 被重写的属性名。
        property: Identifier,
        /// 声明该 final 属性的父类名。
        parent: Identifier,
    },
}

/// 单条 final 类违规：类别、渲染后的消息与源码位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalClassError {
    /// 违规类别。
    pub kind: FinalClassErrorKind,
    /// 人类可读的错误信息。
    pub message: String,
    /// 产生该违规的源码位置；不可定位时为 None。
    pub span: Option<SourceSpan>,
}

impl FinalClassError {
    /// 构造「继承 final 类」错误。
    pub fn final_class_inheritance(class_name: Identifier, parent: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: FinalClassErrorKind::FinalClassInheritance { class_name: class_name.clone(), parent: parent.clone() },
            message: format!("{} 不能继承 final 类 {}", class_name, parent),
            span,
        }
    }

    /// 构造「重写 final 方法」错误。
    pub fn final_method_override(class_name: Identifier, method: Identifier, parent: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: FinalClassErrorKind::FinalMethodOverride { class_name: class_name.clone(), method: method.clone(), parent: parent.clone() },
            message: format!("{} 不能重写 final 方法 {}::{}", class_name, parent, method),
            span,
        }
    }

    /// 构造「重写 final 属性」错误。
    pub fn final_property_override(class_name: Identifier, property: Identifier, parent: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: FinalClassErrorKind::FinalPropertyOverride {
                class_name: class_name.clone(),
                property: property.clone(),
                parent: parent.clone(),
            },
            message: format!("{} 不能重写 final 属性 {}::{}", class_name, parent, property),
            span,
        }
    }
}

impl fmt::Display for FinalClassError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for FinalClassError {}

/// 依据类继承关系校验 final 类与 final 成员重写的检查器。
#[derive(Debug, Default)]
pub struct FinalClassChecker {
    class_map: BTreeMap<Identifier, HirStruct>,
    inheritance_map: BTreeMap<Identifier, Vec<Identifier>>,
    final_class_names: BTreeSet<Identifier>,
    final_methods: BTreeMap<Identifier, BTreeSet<Identifier>>,
    final_properties: BTreeMap<Identifier, BTreeSet<Identifier>>,
    errors: Vec<FinalClassError>,
}

impl FinalClassChecker {
    /// 创建空的 final 类检查器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 检查模块内 final 类继承与 final 成员重写并返回违规列表。
    pub fn check_module(&mut self, module: &HirModule) -> Vec<FinalClassError> {
        self.clear();
        for class in &module.structs {
            self.class_map.insert(class.name.clone(), class.clone());
            self.inheritance_map.insert(class.name.clone(), class.parents.iter().filter_map(|parent| last_name(&parent.name)).collect());
            if class.is_final {
                self.final_class_names.insert(class.name.clone());
            }
            self.final_methods
                .insert(class.name.clone(), class.methods.iter().filter(|method| method.is_final).map(|method| method.name.clone()).collect());
            self.final_properties.insert(
                class.name.clone(),
                class.properties.iter().filter(|property| property.is_final).map(|property| property.name.clone()).collect(),
            );
        }

        for class in &module.structs {
            let parents = self.ancestor_chain(&class.name);
            for parent in &parents {
                if self.final_class_names.contains(parent) {
                    self.errors.push(FinalClassError::final_class_inheritance(class.name.clone(), parent.clone(), None));
                }
                if let Some(methods) = self.final_methods.get(parent) {
                    for method in &class.methods {
                        if methods.contains(&method.name) {
                            self.errors.push(FinalClassError::final_method_override(
                                class.name.clone(),
                                method.name.clone(),
                                parent.clone(),
                                None,
                            ));
                        }
                    }
                }
                if let Some(properties) = self.final_properties.get(parent) {
                    for property in &class.properties {
                        if properties.contains(&property.name) {
                            self.errors.push(FinalClassError::final_property_override(
                                class.name.clone(),
                                property.name.clone(),
                                parent.clone(),
                                None,
                            ));
                        }
                    }
                }
            }
        }

        self.errors.clone()
    }

    fn ancestor_chain(&self, class_name: &Identifier) -> Vec<Identifier> {
        let mut result = Vec::new();
        let mut seen = BTreeSet::new();
        self.collect_ancestors(class_name, &mut seen, &mut result);
        result
    }

    fn collect_ancestors(&self, class_name: &Identifier, seen: &mut BTreeSet<Identifier>, out: &mut Vec<Identifier>) {
        if let Some(parents) = self.inheritance_map.get(class_name) {
            for parent in parents {
                if seen.insert(parent.clone()) {
                    out.push(parent.clone());
                    self.collect_ancestors(parent, seen, out);
                }
            }
        }
    }

    /// 判断给定类名是否为 final 类。
    pub fn is_final_class(&self, name: &Identifier) -> bool {
        self.final_class_names.contains(name)
    }

    /// 返回全部 final 类名。
    pub fn get_final_class_names(&self) -> Vec<Identifier> {
        self.final_class_names.iter().cloned().collect()
    }

    /// 返回给定类声明的 final 方法名集合。
    pub fn get_final_methods(&self, name: &Identifier) -> Option<&BTreeSet<Identifier>> {
        self.final_methods.get(name)
    }

    /// 返回给定类声明的 final 属性名集合。
    pub fn get_final_properties(&self, name: &Identifier) -> Option<&BTreeSet<Identifier>> {
        self.final_properties.get(name)
    }

    /// 返回按类名索引的类定义表。
    pub fn class_map(&self) -> &BTreeMap<Identifier, HirStruct> {
        &self.class_map
    }

    /// 返回「类名 → 直接父类名」的继承关系表。
    pub fn inheritance_map(&self) -> &BTreeMap<Identifier, Vec<Identifier>> {
        &self.inheritance_map
    }

    /// 返回已收集的违规列表。
    pub fn errors(&self) -> &[FinalClassError] {
        &self.errors
    }

    /// 清空已收集的类信息与违规列表。
    pub fn clear(&mut self) {
        self.class_map.clear();
        self.inheritance_map.clear();
        self.final_class_names.clear();
        self.final_methods.clear();
        self.final_properties.clear();
        self.errors.clear();
    }
}

/// 抽象类语义违规的类别。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbstractClassErrorKind {
    /// 实例化了抽象类。
    AbstractClassInstantiation {
        /// 被实例化的抽象类名。
        class_name: Identifier,
    },
    /// 具体子类未实现继承来的抽象方法。
    AbstractMethodNotImplemented {
        /// 子类名。
        class_name: Identifier,
        /// 未实现的抽象方法名。
        method: Identifier,
        /// 声明该抽象方法的父类名。
        parent: Identifier,
    },
    /// 抽象方法带了方法体。
    AbstractMethodWithBody {
        /// 方法所属类名。
        class_name: Identifier,
        /// 带方法体的抽象方法名。
        method: Identifier,
    },
}

/// 单条抽象类违规：类别、渲染后的消息与源码位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbstractClassError {
    /// 违规类别。
    pub kind: AbstractClassErrorKind,
    /// 人类可读的错误信息。
    pub message: String,
    /// 产生该违规的源码位置；不可定位时为 None。
    pub span: Option<SourceSpan>,
}

impl AbstractClassError {
    /// 构造「实例化抽象类」错误。
    pub fn abstract_class_instantiation(class_name: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: AbstractClassErrorKind::AbstractClassInstantiation { class_name: class_name.clone() },
            message: format!("不能实例化抽象类 {}", class_name),
            span,
        }
    }

    /// 构造「抽象方法未实现」错误。
    pub fn abstract_method_not_implemented(class_name: Identifier, method: Identifier, parent: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: AbstractClassErrorKind::AbstractMethodNotImplemented {
                class_name: class_name.clone(),
                method: method.clone(),
                parent: parent.clone(),
            },
            message: format!("{} 未实现来自 {} 的抽象方法 {}", class_name, parent, method),
            span,
        }
    }

    /// 构造「抽象方法带方法体」错误。
    pub fn abstract_method_with_body(class_name: Identifier, method: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: AbstractClassErrorKind::AbstractMethodWithBody { class_name: class_name.clone(), method: method.clone() },
            message: format!("{} 的抽象方法 {} 不能带方法体", class_name, method),
            span,
        }
    }
}

impl fmt::Display for AbstractClassError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AbstractClassError {}

/// 依据继承关系校验抽象类实例化与抽象方法实现的检查器。
#[derive(Debug, Default)]
pub struct AbstractClassChecker {
    abstract_classes: BTreeSet<Identifier>,
    inheritance_map: BTreeMap<Identifier, Vec<Identifier>>,
    class_methods: BTreeMap<Identifier, BTreeSet<Identifier>>,
    abstract_methods: BTreeMap<Identifier, BTreeSet<Identifier>>,
    errors: Vec<AbstractClassError>,
}

impl AbstractClassChecker {
    /// 创建空的抽象类检查器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 检查模块内抽象类与抽象方法实现并返回违规列表。
    pub fn check_module(&mut self, module: &HirModule) -> Vec<AbstractClassError> {
        self.clear();
        let class_map = module.structs.iter().map(|class| (class.name.clone(), class)).collect::<BTreeMap<_, _>>();

        for class in &module.structs {
            self.inheritance_map.insert(class.name.clone(), class.parents.iter().filter_map(|parent| last_name(&parent.name)).collect());
            self.class_methods.insert(class.name.clone(), class.methods.iter().map(|method| method.name.clone()).collect());
            if class.is_abstract {
                self.abstract_classes.insert(class.name.clone());
            }
            self.abstract_methods.insert(class.name.clone(), class.abstract_methods.iter().cloned().collect());

            for method in &class.methods {
                if method.is_abstract && (method.body.expr.is_some() || !method.body.statements.is_empty()) {
                    self.errors.push(AbstractClassError::abstract_method_with_body(class.name.clone(), method.name.clone(), None));
                }
            }
        }

        for class in &module.structs {
            if class.is_abstract {
                continue;
            }
            for parent in self.ancestor_chain(&class.name) {
                if let Some(parent_struct) = class_map.get(&parent) {
                    let required = if !parent_struct.abstract_methods.is_empty() {
                        parent_struct.abstract_methods.clone()
                    }
                    else {
                        parent_struct.methods.iter().filter(|method| method.is_abstract).map(|method| method.name.clone()).collect()
                    };
                    for method in required {
                        if !class.methods.iter().any(|item| item.name == method) {
                            self.errors.push(AbstractClassError::abstract_method_not_implemented(
                                class.name.clone(),
                                method,
                                parent.clone(),
                                None,
                            ));
                        }
                    }
                }
            }
        }

        self.errors.clone()
    }

    fn ancestor_chain(&self, class_name: &Identifier) -> Vec<Identifier> {
        let mut result = Vec::new();
        let mut seen = BTreeSet::new();
        self.collect_ancestors(class_name, &mut seen, &mut result);
        result
    }

    fn collect_ancestors(&self, class_name: &Identifier, seen: &mut BTreeSet<Identifier>, out: &mut Vec<Identifier>) {
        if let Some(parents) = self.inheritance_map.get(class_name) {
            for parent in parents {
                if seen.insert(parent.clone()) {
                    out.push(parent.clone());
                    self.collect_ancestors(parent, seen, out);
                }
            }
        }
    }

    /// 判断给定类名是否为抽象类。
    pub fn is_abstract_class(&self, name: &Identifier) -> bool {
        self.abstract_classes.contains(name)
    }

    /// 返回全部抽象类名。
    pub fn get_abstract_class_names(&self) -> Vec<Identifier> {
        self.abstract_classes.iter().cloned().collect()
    }

    /// 返回给定类声明的抽象方法名集合。
    pub fn get_abstract_methods(&self, name: &Identifier) -> Option<&BTreeSet<Identifier>> {
        self.abstract_methods.get(name)
    }

    /// 返回已登记的抽象类集合。
    pub fn abstract_classes(&self) -> &BTreeSet<Identifier> {
        &self.abstract_classes
    }

    /// 返回「类名 → 直接父类名」的继承关系表。
    pub fn inheritance_map(&self) -> &BTreeMap<Identifier, Vec<Identifier>> {
        &self.inheritance_map
    }

    /// 返回「类名 → 方法名集合」的方法表。
    pub fn class_methods(&self) -> &BTreeMap<Identifier, BTreeSet<Identifier>> {
        &self.class_methods
    }

    /// 返回已收集的违规列表。
    pub fn errors(&self) -> &[AbstractClassError] {
        &self.errors
    }

    /// 清空已收集的类信息与违规列表。
    pub fn clear(&mut self) {
        self.abstract_classes.clear();
        self.inheritance_map.clear();
        self.class_methods.clear();
        self.abstract_methods.clear();
        self.errors.clear();
    }
}
