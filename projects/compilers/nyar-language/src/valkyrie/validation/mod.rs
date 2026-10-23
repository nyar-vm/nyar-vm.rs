//! `HIR / MIR` 主链校验入口。

use std::collections::BTreeSet;

use nyar_types::{DiagnosticSource, DiagnosticSpan};

use crate::{
    hir::{validate_control_flow_module, validate_try_propagate_module},
    mir::{
        MirModule,
        validation::{MirValidationError, validate_module as validate_mir_module},
    },
    type_checker::{
        CompositePatternExhaustivenessChecker, CopySemanticsValidator, LiteralExhaustivenessChecker, ParameterSemantics, ReturnSemantics,
        SealedMatchChecker, SingletonChecker, UnreachableArmChecker, ValueTypeChecker, check_pattern_refutability,
    },
    types::{
        Identifier,
        hir::{HirModule, ValkyrieType},
    },
    valkyrie::{
        HirError,
        hir::lowering::errors::{CopyDisciplineReason, CopyDisciplineViolation, SemanticCheckFailure, SemanticChecker},
    },
};

/// 语义层校验：sealed match 穷尽性、值类型 copy 纪律、字面量穷尽性等，并复用控制流校验。
///
/// 该函数聚合所有 checker 产生的错误消息后统一返回 `HirError`，而不是在首个错误处早返回，
/// 以便一次性向用户呈现尽可能多的诊断信息。
pub fn validate_semantic_module(module: &HirModule) -> Result<(), HirError> {
    validate_try_propagate_module(module)?;
    let copy_violations = run_copy_semantics_validator(module);
    if !copy_violations.is_empty() {
        return Err(HirError::CopyDisciplineViolations {
            violations: copy_violations,
            source: DiagnosticSource { id: "<hir_module>".into(), revision: None },
            span: DiagnosticSpan { start: 0, end: 0 },
        });
    }
    let mut checks: Vec<SemanticCheckFailure> = Vec::new();
    let mut value_checker = ValueTypeChecker::new();
    let value_errors = value_checker.check_module(module);
    if !value_errors.is_empty() {
        checks.push(SemanticCheckFailure {
            checker: SemanticChecker::ValueType,
            messages: value_errors.iter().map(|error| error.to_string()).collect(),
        });
    }
    let mut sealed_checker = SealedMatchChecker::new();
    let sealed_errors = sealed_checker.check_module(module);
    if !sealed_errors.is_empty() {
        checks.push(SemanticCheckFailure {
            checker: SemanticChecker::SealedMatch,
            messages: sealed_errors.into_iter().map(|error| error.message).collect(),
        });
    }
    let mut literal_checker = LiteralExhaustivenessChecker::new();
    let literal_errors = literal_checker.check_module(module);
    if !literal_errors.is_empty() {
        checks.push(SemanticCheckFailure {
            checker: SemanticChecker::LiteralExhaustiveness,
            messages: literal_errors.into_iter().map(|error| error.message).collect(),
        });
    }
    let mut unreachable_checker = UnreachableArmChecker::new();
    let unreachable_errors = unreachable_checker.check_module(module);
    if !unreachable_errors.is_empty() {
        checks.push(SemanticCheckFailure {
            checker: SemanticChecker::UnreachableArm,
            messages: unreachable_errors.into_iter().map(|error| error.message).collect(),
        });
    }
    let mut composite_checker = CompositePatternExhaustivenessChecker::new();
    let composite_errors = composite_checker.check_module(module);
    if !composite_errors.is_empty() {
        checks.push(SemanticCheckFailure {
            checker: SemanticChecker::CompositePatternExhaustiveness,
            messages: composite_errors.into_iter().map(|error| error.message).collect(),
        });
    }
    let mut singleton_checker = SingletonChecker::new();
    let singleton_errors = singleton_checker.check_module(module);
    if !singleton_errors.is_empty() {
        checks.push(SemanticCheckFailure {
            checker: SemanticChecker::Singleton,
            messages: singleton_errors.iter().map(|error| error.to_string()).collect(),
        });
    }
    if !checks.is_empty() {
        return Err(HirError::SemanticCheckViolations {
            checks,
            source: DiagnosticSource { id: "<hir_module>".into(), revision: None },
            span: DiagnosticSpan { start: 0, end: 0 },
        });
    }
    check_pattern_refutability(module)?;
    ControlFlowScheduler::validate_hir_module(module)
}

/// 在编译流水线中执行 `CopySemanticsValidator` 校验，并将违规转为 `HirError`。
///
/// 该函数遍历模块中的函数、结构体方法与 `impl` 块方法，对参数类型与返回类型
/// 调用 `CopySemanticsValidator` 的 `validate_parameter_passing` 与 `validate_return`。
///
/// 违规检测包含两类：
/// 1. 值类型结构体含有引用类型字段（堆数组、未注册命名类型等），使其无法被安全拷贝，
///    任何在参数/返回路径上使用该值类型的位置都会被标记为 copy discipline 违规；
/// 2. 防御性检查：若值类型参数/返回被 `CopySemanticsValidator` 标记为 `Reference` 语义
///    （当前校验器逻辑下不会发生，但保留以应对未来分类逻辑变更）。
fn run_copy_semantics_validator(module: &HirModule) -> Vec<CopyDisciplineViolation> {
    let mut validator = CopySemanticsValidator::new();
    for class in &module.structs {
        validator.register_value_type(class);
    }
    // 值类型声明可以来自直接依赖组（`usize` 在 core 声明、在 std 的聚合字段里使用）。
    // 只按声明身份登记，跨组字段才能与本地声明用同一套复制语义判定。
    for export in &module.imported_semantic_exports {
        for class in &export.structs {
            validator.register_value_type_declaration(class);
        }
    }
    let mut violations: Vec<CopyDisciplineViolation> = Vec::new();
    let mut violating_types: BTreeSet<Identifier> = BTreeSet::new();
    for class in &module.structs {
        if class.is_value_type {
            if let Some(field) = validator.find_non_copy_field(class) {
                violating_types.insert(class.name.clone());
                violations.push(CopyDisciplineViolation {
                    owner: class.name.to_string(),
                    item: Some(field.name.to_string()),
                    type_name: None,
                    reason: CopyDisciplineReason::ReferenceTypedField,
                });
            }
        }
    }
    for function in &module.functions {
        for param in &function.params {
            let semantics = validator.validate_parameter_passing(&param.ty);
            if validator.is_value_type(&param.ty) && semantics != ParameterSemantics::Copy {
                violations.push(CopyDisciplineViolation {
                    owner: function.name.to_string(),
                    item: Some(param.name.name.to_string()),
                    type_name: None,
                    reason: CopyDisciplineReason::ParameterReferenceSemantics,
                });
            }
            if let ValkyrieType::Named(name) = &param.ty {
                if violating_types.contains(name) {
                    violations.push(CopyDisciplineViolation {
                        owner: function.name.to_string(),
                        item: Some(param.name.name.to_string()),
                        type_name: Some(name.to_string()),
                        reason: CopyDisciplineReason::ParameterNonCopyValueType,
                    });
                }
            }
        }
        let return_semantics = validator.validate_return(&function.return_type);
        if validator.is_value_type(&function.return_type) && return_semantics != ReturnSemantics::Copy {
            violations.push(CopyDisciplineViolation {
                owner: function.name.to_string(),
                item: None,
                type_name: None,
                reason: CopyDisciplineReason::ReturnReferenceSemantics,
            });
        }
        if let ValkyrieType::Named(name) = &function.return_type {
            if violating_types.contains(name) {
                violations.push(CopyDisciplineViolation {
                    owner: function.name.to_string(),
                    item: None,
                    type_name: Some(name.to_string()),
                    reason: CopyDisciplineReason::ReturnNonCopyValueType,
                });
            }
        }
    }
    for class in &module.structs {
        for method in &class.methods {
            let owner = format!("{}.{}", class.name, method.name);
            for param in &method.params {
                let semantics = validator.validate_parameter_passing(&param.ty);
                if validator.is_value_type(&param.ty) && semantics != ParameterSemantics::Copy {
                    violations.push(CopyDisciplineViolation {
                        owner: owner.clone(),
                        item: Some(param.name.name.to_string()),
                        type_name: None,
                        reason: CopyDisciplineReason::ParameterReferenceSemantics,
                    });
                }
                if let ValkyrieType::Named(name) = &param.ty {
                    if violating_types.contains(name) {
                        violations.push(CopyDisciplineViolation {
                            owner: owner.clone(),
                            item: Some(param.name.name.to_string()),
                            type_name: Some(name.to_string()),
                            reason: CopyDisciplineReason::ParameterNonCopyValueType,
                        });
                    }
                }
            }
            let return_semantics = validator.validate_return(&method.return_type);
            if validator.is_value_type(&method.return_type) && return_semantics != ReturnSemantics::Copy {
                violations.push(CopyDisciplineViolation {
                    owner: owner.clone(),
                    item: None,
                    type_name: None,
                    reason: CopyDisciplineReason::ReturnReferenceSemantics,
                });
            }
            if let ValkyrieType::Named(name) = &method.return_type {
                if violating_types.contains(name) {
                    violations.push(CopyDisciplineViolation {
                        owner,
                        item: None,
                        type_name: Some(name.to_string()),
                        reason: CopyDisciplineReason::ReturnNonCopyValueType,
                    });
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
                    violations.push(CopyDisciplineViolation {
                        owner: owner.clone(),
                        item: Some(param.name.name.to_string()),
                        type_name: None,
                        reason: CopyDisciplineReason::ParameterReferenceSemantics,
                    });
                }
                if let ValkyrieType::Named(name) = &param.ty {
                    if violating_types.contains(name) {
                        violations.push(CopyDisciplineViolation {
                            owner: owner.clone(),
                            item: Some(param.name.name.to_string()),
                            type_name: Some(name.to_string()),
                            reason: CopyDisciplineReason::ParameterNonCopyValueType,
                        });
                    }
                }
            }
            let return_semantics = validator.validate_return(&method.return_type);
            if validator.is_value_type(&method.return_type) && return_semantics != ReturnSemantics::Copy {
                violations.push(CopyDisciplineViolation {
                    owner: owner.clone(),
                    item: None,
                    type_name: None,
                    reason: CopyDisciplineReason::ReturnReferenceSemantics,
                });
            }
            if let ValkyrieType::Named(name) = &method.return_type {
                if violating_types.contains(name) {
                    violations.push(CopyDisciplineViolation {
                        owner,
                        item: None,
                        type_name: Some(name.to_string()),
                        reason: CopyDisciplineReason::ReturnNonCopyValueType,
                    });
                }
            }
        }
    }
    violations
}

/// 顶层控制流调度与一致性校验入口。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ControlFlowScheduler;

impl ControlFlowScheduler {
    /// 校验 `HIR` 控制流约束是否已闭合。
    pub fn validate_hir_module(module: &HirModule) -> Result<(), HirError> {
        validate_control_flow_module(module)
    }

    /// 校验 `MIR` 控制流图的 block / terminator 一致性。
    pub fn validate_mir_module(module: &MirModule) -> Result<(), MirValidationError> {
        validate_mir_module(module)
    }
}
