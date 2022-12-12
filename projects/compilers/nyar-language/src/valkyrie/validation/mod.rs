//! `HIR / MIR` 主链校验入口。

use std::collections::BTreeSet;

use crate::{
    hir::{validate_control_flow_module, validate_try_propagate_module},
    mir::{MirModule, validation::validate_module as validate_mir_module},
    type_checker::{
        CompositePatternExhaustivenessChecker, CopySemanticsValidator, LiteralExhaustivenessChecker, ParameterSemantics, ReturnSemantics,
        SealedMatchChecker, SingletonChecker, UnreachableArmChecker, ValueTypeChecker, check_pattern_refutability,
    },
    types::{
        Identifier,
        hir::{HirModule, ValkyrieType},
    },
};
use std_data::text::valkyrie::ParseError;

/// 语义层校验：sealed match 穷尽性、值类型 copy 纪律、字面量穷尽性等，并复用控制流校验。
///
/// 该函数聚合所有 checker 产生的错误消息后统一返回 `ParseError`，而不是在首个错误处早返回，
/// 以便一次性向用户呈现尽可能多的诊断信息。
pub fn validate_semantic_module(module: &HirModule) -> Result<(), ParseError> {
    validate_try_propagate_module(module)?;
    let mut all_errors: Vec<String> = Vec::new();
    let mut value_checker = ValueTypeChecker::new();
    let value_errors = value_checker.check_module(module);
    for error in value_errors {
        all_errors.push(error.to_string());
    }
    if let Err(error) = run_copy_semantics_validator(module) {
        all_errors.push(error.to_string());
    }
    let mut sealed_checker = SealedMatchChecker::new();
    let sealed_errors = sealed_checker.check_module(module);
    for error in sealed_errors {
        all_errors.push(error.message);
    }
    let mut literal_checker = LiteralExhaustivenessChecker::new();
    let literal_errors = literal_checker.check_module(module);
    for error in literal_errors {
        all_errors.push(error.message);
    }
    let mut unreachable_checker = UnreachableArmChecker::new();
    let unreachable_errors = unreachable_checker.check_module(module);
    for error in unreachable_errors {
        all_errors.push(error.message);
    }
    let mut composite_checker = CompositePatternExhaustivenessChecker::new();
    let composite_errors = composite_checker.check_module(module);
    for error in composite_errors {
        all_errors.push(error.message);
    }
    let mut singleton_checker = SingletonChecker::new();
    let singleton_errors = singleton_checker.check_module(module);
    for error in singleton_errors {
        all_errors.push(error.to_string());
    }
    if !all_errors.is_empty() {
        let joined = all_errors.join("; ");
        return Err(ParseError::invalid(joined));
    }
    check_pattern_refutability(module)?;
    ControlFlowScheduler::validate_hir_module(module)
}

/// 在编译流水线中执行 `CopySemanticsValidator` 校验，并将违规转为 `ParseError`。
///
/// 该函数遍历模块中的函数、结构体方法与 `impl` 块方法，对参数类型与返回类型
/// 调用 `CopySemanticsValidator` 的 `validate_parameter_passing` 与 `validate_return`。
///
/// 违规检测包含两类：
/// 1. 值类型结构体含有引用类型字段（堆数组、未注册命名类型等），使其无法被安全拷贝，
///    任何在参数/返回路径上使用该值类型的位置都会被标记为 copy discipline 违规；
/// 2. 防御性检查：若值类型参数/返回被 `CopySemanticsValidator` 标记为 `Reference` 语义
///    （当前校验器逻辑下不会发生，但保留以应对未来分类逻辑变更）。
fn run_copy_semantics_validator(module: &HirModule) -> Result<(), ParseError> {
    let mut validator = CopySemanticsValidator::new();
    for class in &module.structs {
        validator.register_value_type(class);
    }
    let mut violations: Vec<String> = Vec::new();
    let mut violating_types: BTreeSet<Identifier> = BTreeSet::new();
    for class in &module.structs {
        if class.is_value_type {
            if let Some(field) = validator.find_non_copy_field(class) {
                violating_types.insert(class.name.clone());
                violations.push(format!(
                    "copy discipline violation: value type `{}` has reference-typed field `{}`, which breaks copy semantics on parameter/return paths",
                    class.name, field.name
                ));
            }
        }
    }
    for function in &module.functions {
        for param in &function.params {
            let semantics = validator.validate_parameter_passing(&param.ty);
            if validator.is_value_type(&param.ty) && semantics != ParameterSemantics::Copy {
                violations.push(format!(
                    "copy discipline violation: parameter `{}` of function `{}` is a value type but uses reference semantics",
                    param.name.name, function.name
                ));
            }
            if let ValkyrieType::Named(name) = &param.ty {
                if violating_types.contains(name) {
                    violations.push(format!(
                        "copy discipline violation: parameter `{}` of function `{}` uses value type `{}` which contains reference fields",
                        param.name.name, function.name, name
                    ));
                }
            }
        }
        let return_semantics = validator.validate_return(&function.return_type);
        if validator.is_value_type(&function.return_type) && return_semantics != ReturnSemantics::Copy {
            violations.push(format!(
                "copy discipline violation: return type of function `{}` is a value type but uses reference semantics",
                function.name
            ));
        }
        if let ValkyrieType::Named(name) = &function.return_type {
            if violating_types.contains(name) {
                violations.push(format!(
                    "copy discipline violation: return type of function `{}` uses value type `{}` which contains reference fields",
                    function.name, name
                ));
            }
        }
    }
    for class in &module.structs {
        for method in &class.methods {
            let owner = format!("{}.{}", class.name, method.name);
            for param in &method.params {
                let semantics = validator.validate_parameter_passing(&param.ty);
                if validator.is_value_type(&param.ty) && semantics != ParameterSemantics::Copy {
                    violations.push(format!(
                        "copy discipline violation: parameter `{}` of method `{}` is a value type but uses reference semantics",
                        param.name.name, owner
                    ));
                }
                if let ValkyrieType::Named(name) = &param.ty {
                    if violating_types.contains(name) {
                        violations.push(format!(
                            "copy discipline violation: parameter `{}` of method `{}` uses value type `{}` which contains reference fields",
                            param.name.name, owner, name
                        ));
                    }
                }
            }
            let return_semantics = validator.validate_return(&method.return_type);
            if validator.is_value_type(&method.return_type) && return_semantics != ReturnSemantics::Copy {
                violations
                    .push(format!("copy discipline violation: return type of method `{}` is a value type but uses reference semantics", owner));
            }
            if let ValkyrieType::Named(name) = &method.return_type {
                if violating_types.contains(name) {
                    violations.push(format!(
                        "copy discipline violation: return type of method `{}` uses value type `{}` which contains reference fields",
                        owner, name
                    ));
                }
            }
        }
    }
    for impl_block in &module.impls {
        for method in &impl_block.methods {
            let owner = format!("impl {}", method.name);
            for param in &method.params {
                let semantics = validator.validate_parameter_passing(&param.ty);
                if validator.is_value_type(&param.ty) && semantics != ParameterSemantics::Copy {
                    violations.push(format!(
                        "copy discipline violation: parameter `{}` of `{}` is a value type but uses reference semantics",
                        param.name.name, owner
                    ));
                }
                if let ValkyrieType::Named(name) = &param.ty {
                    if violating_types.contains(name) {
                        violations.push(format!(
                            "copy discipline violation: parameter `{}` of `{}` uses value type `{}` which contains reference fields",
                            param.name.name, owner, name
                        ));
                    }
                }
            }
            let return_semantics = validator.validate_return(&method.return_type);
            if validator.is_value_type(&method.return_type) && return_semantics != ReturnSemantics::Copy {
                violations.push(format!("copy discipline violation: return type of `{}` is a value type but uses reference semantics", owner));
            }
            if let ValkyrieType::Named(name) = &method.return_type {
                if violating_types.contains(name) {
                    violations.push(format!(
                        "copy discipline violation: return type of `{}` uses value type `{}` which contains reference fields",
                        owner, name
                    ));
                }
            }
        }
    }
    if !violations.is_empty() {
        let joined = violations.join("; ");
        return Err(ParseError::invalid(joined));
    }
    Ok(())
}

/// 顶层控制流调度与一致性校验入口。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ControlFlowScheduler;

impl ControlFlowScheduler {
    /// 校验 `HIR` 控制流约束是否已闭合。
    pub fn validate_hir_module(module: &HirModule) -> Result<(), ParseError> {
        validate_control_flow_module(module)
    }

    /// 校验 `MIR` 控制流图的 block / terminator 一致性。
    pub fn validate_mir_module(module: &MirModule) -> Result<(), ParseError> {
        validate_mir_module(module)
    }
}
