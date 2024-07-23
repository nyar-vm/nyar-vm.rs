#![doc = include_str!("readme.md")]

mod builder;
pub(crate) mod builtin_helpers;
pub(crate) mod builtin_nominals;
mod callable_types;
mod contracts;
mod control_flow_context;
mod control_flow_lowering;
mod effect_lowering;
mod exit_lowering;
mod expr_helpers;
mod expr_lowering;
mod function_lowering;
pub(crate) mod generic_call_instantiation;
mod hir_identity;
mod lowerer;
mod match_lowering;
mod method_lowering;
mod model;
mod null_intrinsic_lowering;
mod pattern_lowering;
mod struct_facts;
/// `MIR` 单元测试辅助工具（构造 builder / 断言结构）。
pub mod test_support;
mod try_propagate_lowering;
mod try_scope_lowering;
mod type_analysis;
mod value_semantics;

// IntrinsicOpcode 权威已删除 — 不得 `pub use` opcode 枚举。
pub use value_semantics::{
    AggregateLayout, AggregateLayoutPlan, FieldLayout, FlagsLayout, LayoutId, MirStorageKind, SumTypeLayout, SumVariantLayout,
    compute_aggregate_layout_plan, ensure_layout_for_type, layout_id_for_type, layout_key_for_type, storage_kind_for_type,
};

// 集成测试入口：仅暴露被迁移的 `tests/` 目标所需的最小内部 item。
#[doc(hidden)]
pub use builtin_helpers::resolve_intrinsic_id;
#[doc(hidden)]
pub use match_lowering::arm_contributes_to_match_merge;
#[doc(hidden)]
pub use value_semantics::{FieldSpec, build_aggregate_layout, ensure_unite_tagged_layout};

pub use lowerer::MirLowerer;
pub use model::*;

use builder::MirBuilder;
use callable_types::CallableReturnTypes;
use function_lowering::lower_function_semantic;
use struct_facts::resolve_self_type_with_owner;
pub(crate) use type_analysis::type_contains_non_monomorphic_signature_reference;
pub use type_analysis::type_identity_table;

// 以下导入同时是子模块通过 `super::` 或 `use super::*;` 依赖的名称，
// 因此必须保留在本模块命名空间中。
use std::collections::BTreeMap;

use crate::types::hir::{HirExpr, HirFunction, HirMatchArm, HirModule, HirPattern, HirStatementKind, ValkyrieType};

use super::MirSumDeclaration;
use builtin_helpers::plain_type_pattern_matches;
use control_flow_context::{MirHandlerDispatchContext, MirResumeContinuationContext};
use expr_helpers::{
    callee_name_matches, future_resume_type, infer_builder_operand_type, lower_callee_operand, lower_resolved_callee, named_type_name,
};
use expr_lowering::lower_literal;
use hir_identity::nominal_type_name;
