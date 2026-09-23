use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use crate::types::{
    Identifier, SourceSpan,
    hir::{HirModule, HirStruct},
};

use super::last_name;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinalClassErrorKind {
    FinalClassInheritance { class_name: Identifier, parent: Identifier },
    FinalMethodOverride { class_name: Identifier, method: Identifier, parent: Identifier },
    FinalPropertyOverride { class_name: Identifier, property: Identifier, parent: Identifier },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalClassError {
    pub kind: FinalClassErrorKind,
    pub message: String,
    pub span: Option<SourceSpan>,
}

impl FinalClassError {
    pub fn final_class_inheritance(class_name: Identifier, parent: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: FinalClassErrorKind::FinalClassInheritance { class_name: class_name.clone(), parent: parent.clone() },
            message: format!("{} 不能继承 final 类 {}", class_name, parent),
            span,
        }
    }

    pub fn final_method_override(class_name: Identifier, method: Identifier, parent: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: FinalClassErrorKind::FinalMethodOverride { class_name: class_name.clone(), method: method.clone(), parent: parent.clone() },
            message: format!("{} 不能重写 final 方法 {}::{}", class_name, parent, method),
            span,
        }
    }

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
    pub fn new() -> Self {
        Self::default()
    }

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

    pub fn is_final_class(&self, name: &Identifier) -> bool {
        self.final_class_names.contains(name)
    }

    pub fn get_final_class_names(&self) -> Vec<Identifier> {
        self.final_class_names.iter().cloned().collect()
    }

    pub fn get_final_methods(&self, name: &Identifier) -> Option<&BTreeSet<Identifier>> {
        self.final_methods.get(name)
    }

    pub fn get_final_properties(&self, name: &Identifier) -> Option<&BTreeSet<Identifier>> {
        self.final_properties.get(name)
    }

    pub fn class_map(&self) -> &BTreeMap<Identifier, HirStruct> {
        &self.class_map
    }

    pub fn inheritance_map(&self) -> &BTreeMap<Identifier, Vec<Identifier>> {
        &self.inheritance_map
    }

    pub fn errors(&self) -> &[FinalClassError] {
        &self.errors
    }

    pub fn clear(&mut self) {
        self.class_map.clear();
        self.inheritance_map.clear();
        self.final_class_names.clear();
        self.final_methods.clear();
        self.final_properties.clear();
        self.errors.clear();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbstractClassErrorKind {
    AbstractClassInstantiation { class_name: Identifier },
    AbstractMethodNotImplemented { class_name: Identifier, method: Identifier, parent: Identifier },
    AbstractMethodWithBody { class_name: Identifier, method: Identifier },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbstractClassError {
    pub kind: AbstractClassErrorKind,
    pub message: String,
    pub span: Option<SourceSpan>,
}

impl AbstractClassError {
    pub fn abstract_class_instantiation(class_name: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: AbstractClassErrorKind::AbstractClassInstantiation { class_name: class_name.clone() },
            message: format!("不能实例化抽象类 {}", class_name),
            span,
        }
    }

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

#[derive(Debug, Default)]
pub struct AbstractClassChecker {
    abstract_classes: BTreeSet<Identifier>,
    inheritance_map: BTreeMap<Identifier, Vec<Identifier>>,
    class_methods: BTreeMap<Identifier, BTreeSet<Identifier>>,
    abstract_methods: BTreeMap<Identifier, BTreeSet<Identifier>>,
    errors: Vec<AbstractClassError>,
}

impl AbstractClassChecker {
    pub fn new() -> Self {
        Self::default()
    }

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

    pub fn is_abstract_class(&self, name: &Identifier) -> bool {
        self.abstract_classes.contains(name)
    }

    pub fn get_abstract_class_names(&self) -> Vec<Identifier> {
        self.abstract_classes.iter().cloned().collect()
    }

    pub fn get_abstract_methods(&self, name: &Identifier) -> Option<&BTreeSet<Identifier>> {
        self.abstract_methods.get(name)
    }

    pub fn abstract_classes(&self) -> &BTreeSet<Identifier> {
        &self.abstract_classes
    }

    pub fn inheritance_map(&self) -> &BTreeMap<Identifier, Vec<Identifier>> {
        &self.inheritance_map
    }

    pub fn class_methods(&self) -> &BTreeMap<Identifier, BTreeSet<Identifier>> {
        &self.class_methods
    }

    pub fn errors(&self) -> &[AbstractClassError] {
        &self.errors
    }

    pub fn clear(&mut self) {
        self.abstract_classes.clear();
        self.inheritance_map.clear();
        self.class_methods.clear();
        self.abstract_methods.clear();
        self.errors.clear();
    }
}
