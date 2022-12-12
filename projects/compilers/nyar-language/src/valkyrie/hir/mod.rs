//! `valkyrie-compiler` 的 `HIR` 入口。
//! 语义归属仍然在编译器主链中，这里只把实际承载类型
//! 收口到一个稳定入口，避免上层直接散落依赖 `crate::types::hir::*`。

/// 匿名 class 提升。
pub mod anonymous_class_hoist;
/// `AST -> HIR` 前的结构预校验。
pub mod ast_validation;
/// Positional / keyword call-argument binding.
pub mod call_binding;
/// 闭包捕获分析。
pub mod capture_analysis;
/// Field-name binding for brace struct literals.
pub mod construct_binding;
/// `HIR` 控制流语义校验。
pub mod control_flow_validation;
/// 语义诊断工具。
pub mod diagnostics;
/// `AST -> HIR` lowering 与编译 facade。
pub mod lowering;
/// 名义类型相关能力。
pub mod nominal;
/// 名义类型注册表。
pub mod nominal_registry;
/// 重载解析相关能力。
pub mod overload;
/// Row 类型相关能力。
pub mod row;
/// Trait 系统相关能力。
pub mod trait_system;
/// `expr?` 传播语义。
pub mod try_propagate;
/// `AST` 类型到 `HIR` 类型的 lowering。
pub mod type_lowering;
/// 统一类型判定入口。
pub mod type_relation;

pub use crate::types::hir::*;
pub use anonymous_class_hoist::hoist_anonymous_classes;
pub(crate) use ast_validation::validate_ast_root;
pub use capture_analysis::CaptureAnalyzer;
pub use construct_binding::bind_construct_fields;
pub use control_flow_validation::{
    function_body_contains_await, function_body_contains_block_on, function_body_contains_yield, function_can_effect_handle,
    function_can_suspend, function_has_suspend_effects, function_is_async, function_needs_suspend_fragment, validate_control_flow_module,
};
pub use lowering::{AstToHir, FrontendBuildOutput, ValkyrieCompiler, compute_nominal_layouts};
pub use nominal_registry::{NominalTypeRegistry, variant_constructor_param_types};
pub use try_propagate::{
    TryPropagateKind, classify_try_operand, is_nullable_type, is_option_apply_type, is_result_apply_type, nullable_payload_type,
    validate_try_propagate_module,
};
pub(crate) use type_lowering::{
    BuiltinTypeAliasScope, ModuleTypeAliasScope, lower_type_expression, render_type_expression, validate_type_expression,
};
