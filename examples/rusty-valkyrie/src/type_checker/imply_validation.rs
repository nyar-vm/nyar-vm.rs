use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use crate::types::{
    Identifier, NamePath, SourceSpan,
    hir::{HirFunction, HirImpl, HirTrait, ValkyrieType as HirType},
};

use super::{TypeInference, display_path, last_name};

/// imply/trait 实现校验违规的类别。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImplyErrorKind {
    /// 引用了未知 trait。
    UnknownTrait {
        /// 未能解析的 trait 路径。
        trait_name: NamePath,
    },
    /// 同一 (target, trait) 组合重复实现。
    DuplicateImpl {
        /// 实现的 target 类型。
        target: HirType,
        /// 被实现的 trait 路径；inherent impl 为 None。
        trait_name: Option<NamePath>,
    },
    /// 两个实现的 where 约束互不包含，无法判定特异度。
    OverlappingImpl {
        /// 冲突实现的 target 类型。
        target: HirType,
        /// 冲突实现的 trait 路径；inherent impl 为 None。
        trait_name: Option<NamePath>,
    },
    /// 实现了 trait 却缺少其 super trait 的显式实现。
    MissingSuperTraitImpl {
        /// 被实现的 trait 路径。
        trait_name: NamePath,
        /// 缺失实现的 super trait 路径。
        super_trait: NamePath,
        /// 实现所在的 target 类型。
        target: HirType,
    },
    /// 实现中缺少 trait 声明的必需方法。
    MissingMethod {
        /// trait 路径。
        trait_name: NamePath,
        /// 缺失的方法名。
        method: Identifier,
    },
    /// 实现中重复定义同名方法。
    DuplicateMethod {
        /// trait 路径。
        trait_name: NamePath,
        /// 重复定义的方法名。
        method: Identifier,
    },
    /// 实现中带有 trait 未声明的方法。
    ExtraMethod {
        /// trait 路径。
        trait_name: NamePath,
        /// 多出的方法名。
        method: Identifier,
    },
    /// 实现方法的签名与 trait 声明不一致。
    MethodSignatureMismatch {
        /// trait 路径。
        trait_name: NamePath,
        /// 签名不一致的方法名。
        method: Identifier,
        /// trait 声明的签名渲染文本。
        expected: String,
        /// 实现侧的方法签名渲染文本。
        found: String,
    },
    /// 实现缺少 trait 要求的关联类型。
    MissingAssociatedType {
        /// trait 路径。
        trait_name: NamePath,
        /// 缺失的关联类型名。
        name: Identifier,
    },
    /// 同一关联类型被重复绑定。
    DuplicateAssociatedType {
        /// trait 路径。
        trait_name: NamePath,
        /// 重复绑定的关联类型名。
        name: Identifier,
    },
    /// 为 trait 未声明的关联类型提供了绑定。
    UnknownAssociatedType {
        /// trait 路径。
        trait_name: NamePath,
        /// 未声明的关联类型名。
        name: Identifier,
    },
    /// 实现缺少 trait 要求的关联常量。
    MissingAssociatedConst {
        /// trait 路径。
        trait_name: NamePath,
        /// 缺失的关联常量名。
        name: Identifier,
    },
    /// 同一关联常量被重复绑定。
    DuplicateAssociatedConst {
        /// trait 路径。
        trait_name: NamePath,
        /// 重复绑定的关联常量名。
        name: Identifier,
    },
    /// 为 trait 未声明的关联常量提供了绑定。
    UnknownAssociatedConst {
        /// trait 路径。
        trait_name: NamePath,
        /// 未声明的关联常量名。
        name: Identifier,
    },
    /// 关联常量声明类型与 trait 声明不符。
    AssociatedConstTypeMismatch {
        /// trait 路径。
        trait_name: NamePath,
        /// 关联常量名。
        name: Identifier,
        /// trait 声明的常量类型。
        expected: HirType,
        /// 实现侧声明的常量类型。
        found: HirType,
    },
    /// 关联常量取值表达式的类型与 trait 声明不符。
    AssociatedConstValueTypeMismatch {
        /// trait 路径。
        trait_name: NamePath,
        /// 关联常量名。
        name: Identifier,
        /// trait 声明的常量类型。
        expected: HirType,
        /// 由取值表达式推断出的类型。
        found: HirType,
    },
}

/// 单条 imply 违规：类别、渲染后的消息与源码位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplyError {
    /// 违规类别。
    pub kind: ImplyErrorKind,
    /// 人类可读的错误信息。
    pub message: String,
    /// 产生该违规的源码位置；不可定位时为 None。
    pub span: Option<SourceSpan>,
}

impl fmt::Display for ImplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ImplyError {}

/// 校验 trait 实现完整性、重复/重叠与关联项绑定的检查器。
#[derive(Debug, Default)]
pub struct ImplyChecker {
    traits: BTreeMap<Identifier, HirTrait>,
    errors: Vec<ImplyError>,
}

impl ImplyChecker {
    /// 创建空的 imply 检查器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 返回已收集的违规列表。
    pub fn errors(&self) -> &[ImplyError] {
        &self.errors
    }

    /// 检查模块内全部 impl 块并返回违规列表。
    pub fn check_module(&mut self, module: &crate::types::hir::HirModule) -> Vec<ImplyError> {
        self.clear();
        for trait_def in &module.traits {
            self.traits.insert(trait_def.name.clone(), trait_def.clone());
        }
        for (index, impl_block) in module.impls.iter().enumerate() {
            self.check_duplicate_impl(index, &module.impls);
            self.check_impl(impl_block, &module.impls);
        }
        self.errors.clone()
    }

    /// 清空已登记的 trait 与违规列表。
    pub fn clear(&mut self) {
        self.traits.clear();
        self.errors.clear();
    }

    fn check_duplicate_impl(&mut self, index: usize, impls: &[HirImpl]) {
        let impl_block = &impls[index];
        for existing in impls[..index].iter().filter(|existing| same_trait_impl_head(existing, impl_block)) {
            match compare_impl_specificity(existing, impl_block) {
                ImplSpecificity::Equivalent => self.errors.push(ImplyError {
                    kind: ImplyErrorKind::DuplicateImpl { target: impl_block.target.clone(), trait_name: impl_block.trait_path.clone() },
                    message: format!(
                        "重复 imply 实现: target={:?}, trait={}",
                        impl_block.target,
                        impl_block.trait_path.as_ref().map(display_path).unwrap_or_else(|| "<inherent>".to_string())
                    ),
                    span: None,
                }),
                ImplSpecificity::Incomparable => self.errors.push(ImplyError {
                    kind: ImplyErrorKind::OverlappingImpl { target: impl_block.target.clone(), trait_name: impl_block.trait_path.clone() },
                    message: format!(
                        "重叠 imply 实现: target={:?}, trait={}",
                        impl_block.target,
                        impl_block.trait_path.as_ref().map(display_path).unwrap_or_else(|| "<inherent>".to_string())
                    ),
                    span: None,
                }),
                ImplSpecificity::MoreSpecific | ImplSpecificity::LessSpecific => {}
            }
        }
    }

    fn check_impl(&mut self, impl_block: &HirImpl, impls: &[HirImpl]) {
        let Some(trait_path) = &impl_block.trait_path
        else {
            return;
        };
        let Some(trait_name) = last_name(trait_path)
        else {
            self.errors.push(ImplyError {
                kind: ImplyErrorKind::UnknownTrait { trait_name: trait_path.clone() },
                message: format!("未知 trait {}", display_path(trait_path)),
                span: None,
            });
            return;
        };
        let Some(trait_def) = self.traits.get(&trait_name).cloned()
        else {
            self.errors.push(ImplyError {
                kind: ImplyErrorKind::UnknownTrait { trait_name: trait_path.clone() },
                message: format!("未知 trait {}", display_path(trait_path)),
                span: None,
            });
            return;
        };

        self.check_super_traits(impl_block, &trait_def, trait_path, impls);
        self.check_trait_methods(impl_block, &trait_def, trait_path);
        self.check_associated_types(impl_block, &trait_def, trait_path);
        self.check_associated_consts(impl_block, &trait_def, trait_path);
    }

    fn check_super_traits(&mut self, impl_block: &HirImpl, trait_def: &HirTrait, trait_path: &NamePath, impls: &[HirImpl]) {
        for super_trait_path in collect_super_trait_paths(&self.traits, trait_def) {
            if !impls.iter().any(|candidate| candidate.target == impl_block.target && candidate.trait_path.as_ref() == Some(&super_trait_path))
            {
                self.errors.push(ImplyError {
                    kind: ImplyErrorKind::MissingSuperTraitImpl {
                        trait_name: trait_path.clone(),
                        super_trait: super_trait_path.clone(),
                        target: impl_block.target.clone(),
                    },
                    message: format!("trait impl {} 缺少 super trait {} 的显式实现", display_path(trait_path), display_path(&super_trait_path)),
                    span: None,
                });
            }
        }
    }

    fn check_trait_methods(&mut self, impl_block: &HirImpl, trait_def: &HirTrait, trait_path: &NamePath) {
        let mut trait_methods = BTreeMap::new();
        for method in &trait_def.methods {
            trait_methods.insert(method.name.clone(), method);
        }
        for method in &trait_def.default_methods {
            trait_methods.insert(method.name.clone(), method);
        }

        let mut impl_methods = BTreeMap::<Identifier, Vec<&HirFunction>>::new();
        for method in &impl_block.methods {
            impl_methods.entry(method.name.clone()).or_default().push(method);
        }

        for (name, items) in &impl_methods {
            if items.len() > 1 {
                self.errors.push(ImplyError {
                    kind: ImplyErrorKind::DuplicateMethod { trait_name: trait_path.clone(), method: name.clone() },
                    message: format!("trait impl {} 中的方法 {} 重复定义", display_path(trait_path), name),
                    span: None,
                });
            }
            if !trait_methods.contains_key(name) {
                self.errors.push(ImplyError {
                    kind: ImplyErrorKind::ExtraMethod { trait_name: trait_path.clone(), method: name.clone() },
                    message: format!("trait impl {} 中的方法 {} 不在 trait 声明中", display_path(trait_path), name),
                    span: None,
                });
            }
        }

        for method in &trait_def.methods {
            match impl_methods.get(&method.name) {
                None => self.errors.push(ImplyError {
                    kind: ImplyErrorKind::MissingMethod { trait_name: trait_path.clone(), method: method.name.clone() },
                    message: format!("trait impl {} 缺少必需方法 {}", display_path(trait_path), method.name),
                    span: None,
                }),
                Some(items) if items.len() == 1 => {
                    let found = items[0];
                    if !same_method_signature(method, found) {
                        self.errors.push(ImplyError {
                            kind: ImplyErrorKind::MethodSignatureMismatch {
                                trait_name: trait_path.clone(),
                                method: method.name.clone(),
                                expected: render_method_signature(method),
                                found: render_method_signature(found),
                            },
                            message: format!("trait impl {} 的方法 {} 签名不匹配", display_path(trait_path), method.name),
                            span: Some(found.span.clone()),
                        });
                    }
                }
                Some(_) => {}
            }
        }

        for method in &trait_def.default_methods {
            if let Some(items) = impl_methods.get(&method.name) {
                if items.len() == 1 && !same_method_signature(method, items[0]) {
                    self.errors.push(ImplyError {
                        kind: ImplyErrorKind::MethodSignatureMismatch {
                            trait_name: trait_path.clone(),
                            method: method.name.clone(),
                            expected: render_method_signature(method),
                            found: render_method_signature(items[0]),
                        },
                        message: format!("trait impl {} 重写默认方法 {} 时签名不匹配", display_path(trait_path), method.name),
                        span: Some(items[0].span.clone()),
                    });
                }
            }
        }
    }

    fn check_associated_types(&mut self, impl_block: &HirImpl, trait_def: &HirTrait, trait_path: &NamePath) {
        let declared = trait_def.associated_types.iter().map(|item| (item.name.clone(), item)).collect::<BTreeMap<_, _>>();
        let mut bound_counts = BTreeMap::<Identifier, usize>::new();

        for binding in &impl_block.associated_type_impls {
            *bound_counts.entry(binding.name.clone()).or_default() += 1;
            if !declared.contains_key(&binding.name) {
                self.errors.push(ImplyError {
                    kind: ImplyErrorKind::UnknownAssociatedType { trait_name: trait_path.clone(), name: binding.name.clone() },
                    message: format!("trait impl {} 提供了未知关联类型 {}", display_path(trait_path), binding.name),
                    span: Some(binding.span.clone()),
                });
            }
        }

        for (name, count) in &bound_counts {
            if *count > 1 {
                self.errors.push(ImplyError {
                    kind: ImplyErrorKind::DuplicateAssociatedType { trait_name: trait_path.clone(), name: name.clone() },
                    message: format!("trait impl {} 的关联类型 {} 重复绑定", display_path(trait_path), name),
                    span: None,
                });
            }
        }

        for item in &trait_def.associated_types {
            if item.default.is_none() && !bound_counts.contains_key(&item.name) {
                self.errors.push(ImplyError {
                    kind: ImplyErrorKind::MissingAssociatedType { trait_name: trait_path.clone(), name: item.name.clone() },
                    message: format!("trait impl {} 缺少关联类型 {}", display_path(trait_path), item.name),
                    span: None,
                });
            }
        }
    }

    fn check_associated_consts(&mut self, impl_block: &HirImpl, trait_def: &HirTrait, trait_path: &NamePath) {
        let declared = trait_def.associated_constants.iter().map(|item| (item.name.clone(), item)).collect::<BTreeMap<_, _>>();
        let mut bound_counts = BTreeMap::<Identifier, usize>::new();

        for binding in &impl_block.associated_const_impls {
            *bound_counts.entry(binding.name.clone()).or_default() += 1;
            let Some(declared_item) = declared.get(&binding.name)
            else {
                self.errors.push(ImplyError {
                    kind: ImplyErrorKind::UnknownAssociatedConst { trait_name: trait_path.clone(), name: binding.name.clone() },
                    message: format!("trait impl {} 提供了未知关联常量 {}", display_path(trait_path), binding.name),
                    span: Some(binding.span.clone()),
                });
                continue;
            };
            if let Some(found) = &binding.const_type {
                if found != &declared_item.const_type {
                    self.errors.push(ImplyError {
                        kind: ImplyErrorKind::AssociatedConstTypeMismatch {
                            trait_name: trait_path.clone(),
                            name: binding.name.clone(),
                            expected: declared_item.const_type.clone(),
                            found: found.clone(),
                        },
                        message: format!("trait impl {} 的关联常量 {} 类型不匹配", display_path(trait_path), binding.name),
                        span: Some(binding.span.clone()),
                    });
                }
            }
            let mut inference = TypeInference::new();
            if let Ok(found_value_type) = inference.infer(&binding.value) {
                if found_value_type != declared_item.const_type {
                    self.errors.push(ImplyError {
                        kind: ImplyErrorKind::AssociatedConstValueTypeMismatch {
                            trait_name: trait_path.clone(),
                            name: binding.name.clone(),
                            expected: declared_item.const_type.clone(),
                            found: found_value_type.clone(),
                        },
                        message: format!("trait impl {} 的关联常量 {} 值类型不匹配", display_path(trait_path), binding.name),
                        span: Some(binding.span.clone()),
                    });
                }
            }
        }

        for (name, count) in &bound_counts {
            if *count > 1 {
                self.errors.push(ImplyError {
                    kind: ImplyErrorKind::DuplicateAssociatedConst { trait_name: trait_path.clone(), name: name.clone() },
                    message: format!("trait impl {} 的关联常量 {} 重复绑定", display_path(trait_path), name),
                    span: None,
                });
            }
        }

        for item in &trait_def.associated_constants {
            if item.default_value.is_none() && !bound_counts.contains_key(&item.name) {
                self.errors.push(ImplyError {
                    kind: ImplyErrorKind::MissingAssociatedConst { trait_name: trait_path.clone(), name: item.name.clone() },
                    message: format!("trait impl {} 缺少关联常量 {}", display_path(trait_path), item.name),
                    span: None,
                });
            }
        }
    }
}

fn same_trait_impl_head(left: &HirImpl, right: &HirImpl) -> bool {
    match (&left.trait_path, &right.trait_path) {
        (Some(left_trait), Some(right_trait)) => left.target == right.target && left_trait == right_trait,
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImplSpecificity {
    Equivalent,
    MoreSpecific,
    LessSpecific,
    Incomparable,
}

fn compare_impl_specificity(left: &HirImpl, right: &HirImpl) -> ImplSpecificity {
    let left_pairs = impl_where_constraint_pairs(left);
    let right_pairs = impl_where_constraint_pairs(right);

    match (left_pairs == right_pairs, left_pairs.is_superset(&right_pairs), right_pairs.is_superset(&left_pairs)) {
        (true, _, _) => ImplSpecificity::Equivalent,
        (false, true, false) => ImplSpecificity::MoreSpecific,
        (false, false, true) => ImplSpecificity::LessSpecific,
        _ => ImplSpecificity::Incomparable,
    }
}

fn impl_where_constraint_pairs(impl_block: &HirImpl) -> BTreeSet<(HirType, crate::types::hir::HirTraitBound)> {
    impl_block
        .where_constraints
        .iter()
        .flat_map(|constraint| {
            constraint.bounds.iter().cloned().map(|bound| {
                let mut bound = bound;
                bound.associated_types.sort();
                (constraint.target.clone(), bound)
            })
        })
        .collect()
}
fn same_method_signature(expected: &HirFunction, found: &HirFunction) -> bool {
    expected.generics.len() == found.generics.len()
        && expected.return_type == found.return_type
        && expected.params.len() == found.params.len()
        && expected.params.iter().zip(&found.params).all(|(left, right)| left.ty == right.ty)
}

fn render_method_signature(method: &HirFunction) -> String {
    let params = method.params.iter().map(|param| format!("{:?}", param.ty)).collect::<Vec<_>>().join(", ");
    format!("{}({}) -> {:?}", method.name, params, method.return_type)
}

fn trait_type_to_path(ty: &HirType) -> Option<NamePath> {
    match ty {
        HirType::Named(name) => Some(NamePath::new(vec![name.clone()])),
        HirType::Apply(base, _) => match base.as_ref() {
            HirType::Named(name) => Some(NamePath::new(vec![name.clone()])),
            _ => None,
        },
        _ => None,
    }
}

fn collect_super_trait_paths(traits: &BTreeMap<Identifier, HirTrait>, trait_def: &HirTrait) -> Vec<NamePath> {
    let mut visited = BTreeSet::new();
    let mut collected = Vec::new();
    collect_super_trait_paths_inner(traits, trait_def, &mut visited, &mut collected);
    collected
}

fn collect_super_trait_paths_inner(
    traits: &BTreeMap<Identifier, HirTrait>,
    trait_def: &HirTrait,
    visited: &mut BTreeSet<NamePath>,
    collected: &mut Vec<NamePath>,
) {
    for super_trait in &trait_def.super_traits {
        let Some(super_trait_path) = trait_type_to_path(super_trait)
        else {
            continue;
        };
        if !visited.insert(super_trait_path.clone()) {
            continue;
        }
        collected.push(super_trait_path.clone());
        if let Some(super_trait_name) = last_name(&super_trait_path) {
            if let Some(super_trait_def) = traits.get(&super_trait_name) {
                collect_super_trait_paths_inner(traits, super_trait_def, visited, collected);
            }
        }
    }
}
