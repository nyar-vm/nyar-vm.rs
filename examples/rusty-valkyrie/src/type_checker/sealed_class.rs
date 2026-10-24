//! sealed class、enum 注册表与穷尽性检查。

use crate::types::{
    Identifier, SourceSpan,
    hir::{HirModule, HirStruct},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

/// sealed class 与 enum 穷尽性检查的违规类别。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SealedClassErrorKind {
    /// match 未覆盖全部子类或变体。
    NonExhaustiveMatch,
    /// 引用了未知的 sealed class。
    UnknownSealedClass,
    /// 目标类型不是 sealed 基类。
    NotSealedBase,
    /// 同一分支被重复列出。
    DuplicateMatchArm,
}

/// 单条 sealed/enum 违规：类别、渲染后的消息与源码位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedClassError {
    /// 违规类别。
    pub kind: SealedClassErrorKind,
    /// 人类可读的错误信息。
    pub message: String,
    /// 产生该违规的源码位置；不可定位时为 None。
    pub span: Option<SourceSpan>,
}

impl fmt::Display for SealedClassError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SealedClassError {}

/// sealed class 及其被允许子类的注册表。
#[derive(Debug, Default, Clone)]
pub struct SealedClassRegistry {
    sealed_classes: BTreeSet<Identifier>,
    subclasses: BTreeMap<Identifier, BTreeSet<Identifier>>,
}

impl SealedClassRegistry {
    /// 创建空的 sealed class 注册表。
    pub fn new() -> Self {
        Self::default()
    }

    /// 若给定类标记为 sealed，则登记其类名。
    pub fn register_sealed_class(&mut self, class: &HirStruct) {
        if class.is_sealed {
            self.sealed_classes.insert(class.name.clone());
        }
    }

    /// 登记 sealed 基类的一个允许子类；基类未登记时返回错误。
    pub fn register_subclass(&mut self, sealed_name: &Identifier, subclass: &Identifier) -> Result<(), SealedClassError> {
        if !self.sealed_classes.contains(sealed_name) {
            return Err(SealedClassError {
                kind: SealedClassErrorKind::NotSealedBase,
                message: format!("{} is not a sealed class", sealed_name),
                span: None,
            });
        }
        self.subclasses.entry(sealed_name.clone()).or_default().insert(subclass.clone());
        Ok(())
    }

    /// 判断给定类名是否已登记为 sealed class。
    pub fn is_sealed_class(&self, name: &Identifier) -> bool {
        self.sealed_classes.contains(name)
    }

    /// 返回给定 sealed 基类已登记的允许子类列表。
    pub fn get_permitted_subclasses(&self, name: &Identifier) -> Vec<Identifier> {
        self.subclasses.get(name).map(|items| items.iter().cloned().collect()).unwrap_or_default()
    }

    /// 迭代全部已登记的 sealed 类名。
    pub fn sealed_class_names(&self) -> impl Iterator<Item = &Identifier> {
        self.sealed_classes.iter()
    }
}

/// enum/sum 类型名到其变体列表的注册表。
#[derive(Debug, Default, Clone)]
pub struct EnumRegistry {
    sum_types: BTreeMap<Identifier, Vec<Identifier>>,
}

impl EnumRegistry {
    /// 从模块的 enum 与导入 enum 构建注册表。
    pub fn from_module(module: &HirModule) -> Self {
        let mut registry = Self::default();
        for enum_def in module.enums.iter().chain(module.imported_enums.iter()).chain(module.imported_nominal_enums()) {
            registry.sum_types.insert(enum_def.name.clone(), enum_def.variants.iter().map(|variant| variant.name.clone()).collect());
        }
        registry
    }

    /// 判断给定名字是否为本注册表登记的 sum/enum 类型。
    pub fn is_sum_type(&self, name: &Identifier) -> bool {
        self.sum_types.contains_key(name)
    }

    /// 返回给定 sum 类型的变体名列表；未知类型返回空切片。
    pub fn variants(&self, name: &Identifier) -> &[Identifier] {
        self.sum_types.get(name).map(Vec::as_slice).unwrap_or(&[])
    }

    /// 由已覆盖变体反查唯一可能的 sum 类型；候选不唯一时返回 None。
    pub fn enum_from_covered_variants(&self, covered: &[Identifier]) -> Option<Identifier> {
        let mut candidates = BTreeSet::new();
        for (enum_name, variants) in &self.sum_types {
            if covered.iter().any(|variant| variants.contains(variant)) {
                candidates.insert(enum_name.clone());
            }
        }
        if candidates.len() == 1 { candidates.into_iter().next() } else { None }
    }
}

/// 组合 sealed 与 enum 注册表执行穷尽性判定的检查器。
#[derive(Debug, Clone)]
pub struct ExhaustivenessChecker {
    registry: SealedClassRegistry,
    enum_registry: EnumRegistry,
}

impl ExhaustivenessChecker {
    /// 基于 sealed class 注册表创建检查器（enum 注册表为空）。
    pub fn new(registry: SealedClassRegistry) -> Self {
        Self { registry, enum_registry: EnumRegistry::default() }
    }

    /// 同时携带 sealed class 与 enum 注册表创建检查器。
    pub fn with_enum_registry(registry: SealedClassRegistry, enum_registry: EnumRegistry) -> Self {
        Self { registry, enum_registry }
    }

    /// 返回内部使用的 enum 注册表。
    pub fn enum_registry(&self) -> &EnumRegistry {
        &self.enum_registry
    }

    /// 返回内部使用的 sealed class 注册表。
    pub fn sealed_registry(&self) -> &SealedClassRegistry {
        &self.registry
    }

    /// 校验 match 是否覆盖 sealed 基类的全部允许子类。
    pub fn check_exhaustiveness(&self, sealed_name: &Identifier, covered: &[Identifier]) -> Result<(), SealedClassError> {
        let declared = self.registry.get_permitted_subclasses(sealed_name);
        let covered_set = covered.iter().cloned().collect::<BTreeSet<_>>();
        let missing = declared.into_iter().filter(|name| !covered_set.contains(name)).collect::<Vec<_>>();
        if missing.is_empty() {
            Ok(())
        }
        else {
            Err(SealedClassError {
                kind: SealedClassErrorKind::NonExhaustiveMatch,
                message: format!(
                    "non exhaustive match for {}: missing {}",
                    sealed_name,
                    missing.iter().map(|item| item.to_string()).collect::<Vec<_>>().join(", ")
                ),
                span: None,
            })
        }
    }

    /// 判断存在 wildcard 分支时是否视为完全覆盖。
    pub fn is_wildcard_exhaustive(&self, _type_name: &Identifier, has_wildcard: bool) -> bool {
        has_wildcard
    }

    /// 检查已覆盖分支中是否存在重复项，命中则返回对应错误。
    pub fn check_duplicate_arms(&self, covered: &[Identifier]) -> Option<SealedClassError> {
        let mut seen = BTreeSet::new();
        for variant in covered {
            if !seen.insert(variant.clone()) {
                return Some(SealedClassError {
                    kind: SealedClassErrorKind::DuplicateMatchArm,
                    message: format!("duplicate match arm for {}", variant),
                    span: None,
                });
            }
        }
        None
    }

    /// 校验 match 是否覆盖 enum 的全部变体。
    pub fn check_variant_exhaustiveness(&self, enum_name: &Identifier, covered: &[Identifier]) -> Result<(), SealedClassError> {
        let declared = self.enum_registry.variants(enum_name);
        let covered_set = covered.iter().cloned().collect::<BTreeSet<_>>();
        let missing = declared.iter().filter(|variant| !covered_set.contains(*variant)).cloned().collect::<Vec<_>>();
        if missing.is_empty() {
            Ok(())
        }
        else {
            Err(SealedClassError {
                kind: SealedClassErrorKind::NonExhaustiveMatch,
                message: format!(
                    "non exhaustive match for {}: missing {}",
                    enum_name,
                    missing.iter().map(|item| item.to_string()).collect::<Vec<_>>().join(", ")
                ),
                span: None,
            })
        }
    }
}
