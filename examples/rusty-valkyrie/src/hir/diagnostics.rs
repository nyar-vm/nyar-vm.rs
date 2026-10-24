//! Minimal semantic diagnostic helpers.

use crate::{
    hir::{row::RowRequirementError, trait_system::TraitSatisfactionError},
    types::{Identifier, NamePath},
};

/// 语义层失败的分类，供统一诊断生成使用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticFailure {
    /// 名义类型不匹配。
    NominalMismatch {
        /// 期望的名义类型名。
        expected: Identifier,
        /// 实际得到的名义类型名。
        found: Identifier,
    },
    /// 匿名 row 要求不满足。
    Row {
        /// 具体的 row 校验失败。
        error: RowRequirementError,
    },
    /// 具名 trait 满足失败。
    Trait {
        /// 具体的 trait 满足失败。
        error: TraitSatisfactionError,
    },
    /// 开放 effect 缺少可用 handler 证据。
    Effect {
        /// 缺失 handler 的 effect 名称。
        effect_name: NamePath,
    },
}

/// 单条语义诊断：稳定错误码加人类可读消息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticDiagnostic {
    /// 稳定的诊断错误码。
    pub code: &'static str,
    /// 人类可读的诊断消息。
    pub message: String,
}

/// 将语义失败分类转换为可报告的诊断。
pub fn diagnose_failure(failure: &SemanticFailure) -> SemanticDiagnostic {
    match failure {
        SemanticFailure::NominalMismatch { expected, found } => SemanticDiagnostic {
            code: "E_NOMINAL_MISMATCH",
            message: format!("名义类型不匹配：期望 `{expected}`，实际得到 `{found}`"),
        },
        SemanticFailure::Row { error } => match error {
            RowRequirementError::MissingMethod { name } => {
                SemanticDiagnostic { code: "E_ROW_MISSING_METHOD", message: format!("匿名 row 不满足：缺少方法 `{name}`") }
            }
            RowRequirementError::SignatureMismatch { name, .. } => SemanticDiagnostic {
                code: "E_ROW_SIGNATURE_MISMATCH",
                message: format!("匿名 row 不满足：方法 `{name}` 的签名不匹配"),
            },
            RowRequirementError::DuplicateMethodRequirement { name } => {
                SemanticDiagnostic { code: "E_ROW_DUPLICATE_REQUIREMENT", message: format!("匿名 row 存在重复的方法要求 `{name}`") }
            }
            RowRequirementError::AmbiguousCandidateMethod { name } => SemanticDiagnostic {
                code: "E_ROW_AMBIGUOUS_METHOD",
                message: format!("匿名 row 匹配存在歧义：候选侧 `{name}` 有多个同名方法"),
            },
            RowRequirementError::UnsupportedAssociatedType { name } => {
                SemanticDiagnostic { code: "E_ROW_ASSOCIATED_TYPE", message: format!("匿名 row 不能声明关联类型 `{name}`") }
            }
        },
        SemanticFailure::Trait { error } => match error {
            TraitSatisfactionError::AmbiguousExplicitImpls { trait_path, .. } => SemanticDiagnostic {
                code: "E_TRAIT_AMBIGUOUS_IMPL",
                message: format!("具名 trait `{trait_path}` 存在多个显式实现，无法确定 witness"),
            },
            TraitSatisfactionError::MissingExplicitImplForSuperTraits { trait_path } => SemanticDiagnostic {
                code: "E_TRAIT_SUPER_IMPL_REQUIRED",
                message: format!("具名 trait `{trait_path}` 含有 super trait，必须提供显式实现"),
            },
            TraitSatisfactionError::MissingExplicitImplForAssociatedTypes { trait_path } => SemanticDiagnostic {
                code: "E_TRAIT_ASSOC_IMPL_REQUIRED",
                message: format!("具名 trait `{trait_path}` 含有关联类型，必须提供显式实现"),
            },
            TraitSatisfactionError::MissingMethodBinding { trait_path, name } => SemanticDiagnostic {
                code: "E_TRAIT_METHOD_MISSING",
                message: format!("具名 trait `{trait_path}` 缺少方法 `{name}` 的绑定"),
            },
            TraitSatisfactionError::AmbiguousMethodBinding { trait_path, name } => SemanticDiagnostic {
                code: "E_TRAIT_METHOD_AMBIGUOUS",
                message: format!("具名 trait `{trait_path}` 的方法 `{name}` 绑定不唯一"),
            },
            TraitSatisfactionError::MissingAssociatedTypeBinding { trait_path, name } => SemanticDiagnostic {
                code: "E_TRAIT_ASSOC_MISSING",
                message: format!("具名 trait `{trait_path}` 缺少关联类型 `{name}` 的绑定"),
            },
            TraitSatisfactionError::AmbiguousAssociatedTypeBinding { trait_path, name } => SemanticDiagnostic {
                code: "E_TRAIT_ASSOC_AMBIGUOUS",
                message: format!("具名 trait `{trait_path}` 的关联类型 `{name}` 绑定不唯一"),
            },
            TraitSatisfactionError::StructuralMismatch { .. } => SemanticDiagnostic {
                code: "E_TRAIT_STRUCTURAL_MISMATCH",
                message: "具名 trait 的结构入口检查失败，无法形成 witness".to_owned(),
            },
            TraitSatisfactionError::StructuralAmbiguity { .. } => SemanticDiagnostic {
                code: "E_TRAIT_STRUCTURAL_AMBIGUITY",
                message: "具名 trait 的结构入口存在歧义，无法形成唯一 witness".to_owned(),
            },
        },
        SemanticFailure::Effect { effect_name } => SemanticDiagnostic {
            code: "E_EFFECT_HANDLER_MISSING",
            message: format!("开放 effect `{effect_name}` 缺少可用 handler 证据"),
        },
    }
}
