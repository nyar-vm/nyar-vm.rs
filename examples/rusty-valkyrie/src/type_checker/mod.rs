//! Valkyrie 类型检查器 facade。
//!
//! 每个 checker 只读取 HIR 与类型事实，不在检查过程中写入新的语义 identity
//! 或补默认类型。根模块只声明子模块并重导出公共类型，不承载 checker 实现。
//!
//! 子模块分工：
//!
//! - 既有子模块：类语义、复合模式穷尽、控制流、effect 类型、imply、字面量穷尽、oop、模式、singleton、不可达分支。
//! - 访问控制、构造器链、属性、value 类型、复制语义、sealed/enum 穷尽、widget 检查各自独立成文件。

mod access_control;
mod class_semantics;
mod composite_pattern_exhaustiveness;
mod constructor_chain;
mod control_flow_validation;
mod copy_semantics;
mod effect_typing;
mod imply_validation;
mod literal_exhaustiveness;
mod oop_checks;
mod path_helpers;
mod pattern_checks;
mod property_check;
mod sealed_class;
mod singleton_checks;
mod unreachable_arm;
mod value_type;
mod widget_check;

pub use self::{
    access_control::{AccessContext, AccessControlChecker, AccessControlError, AccessControlErrorKind},
    class_semantics::{AbstractClassChecker, AbstractClassError, AbstractClassErrorKind, FinalClassChecker, FinalClassError},
    composite_pattern_exhaustiveness::{
        CompositePatternExhaustivenessChecker, CompositePatternExhaustivenessError, CompositePatternExhaustivenessErrorKind,
    },
    constructor_chain::{ConstructorChainChecker, ConstructorChainError, ConstructorChainErrorKind},
    control_flow_validation::{InferenceTypeVar, TypeError, TypeInference},
    copy_semantics::{AssignmentSemantics, CopySemanticsValidator, ParameterSemantics, ReturnSemantics},
    effect_typing::{
        AssociatedTypeConstraint, ConstraintChainNode, ConstraintError, ConstraintErrorKind, ConstraintPropagator, ConstraintReport,
        ConstraintSolver, FixSuggestion, Lifetime, LifetimeConstraint, LifetimeConstraintKind, MultiTraitBound, TraitBoundChecker, TraitImpl,
        TypeConstraint, TypeVar, WhereBound, WhereClause,
    },
    imply_validation::{ImplyChecker, ImplyError, ImplyErrorKind},
    literal_exhaustiveness::{LiteralExhaustivenessChecker, LiteralExhaustivenessError, LiteralExhaustivenessErrorKind},
    oop_checks::SealedMatchChecker,
    pattern_checks::check_pattern_refutability,
    property_check::{PropertyChecker, PropertyError, PropertyErrorKind, SetterValidationAnalyzer},
    sealed_class::{EnumRegistry, ExhaustivenessChecker, SealedClassError, SealedClassErrorKind, SealedClassRegistry},
    singleton_checks::{SingletonChecker, SingletonError, SingletonErrorKind},
    unreachable_arm::{UnreachableArmChecker, UnreachableArmError, UnreachableArmErrorKind},
    value_type::{ValueTypeChecker, ValueTypeError, ValueTypeErrorKind},
    widget_check::{WidgetChecker, WidgetError, WidgetErrorKind},
};

// 既有子模块通过 `super::last_name` / `super::display_path` 依赖这两个 helper。
use path_helpers::{display_path, last_name};
