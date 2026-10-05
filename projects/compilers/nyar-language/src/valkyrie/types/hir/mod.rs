#![doc = include_str!("readme.md")]

pub mod context;
pub mod r#enum;
pub mod export_spec;
pub mod expr;
pub mod function;
pub mod identifier;
pub mod r#impl;
pub mod module;
pub mod property;
pub mod statement;
pub mod r#struct;
pub mod r#trait;
pub mod type_family;
pub mod types;
pub mod visibility;
pub mod widget;

pub use types::{AccessLevel, *};

pub use context::RenameContext;
pub use r#enum::{HirEnum, HirFlagMember, HirFlags, HirVariant};
pub use export_spec::{
    HirExportSpec, parse_export_spec_from_annotations, parse_workload_phase_from_annotations, resolve_attribute_id, snake_case_to_camel_case,
};
pub use expr::{
    CaptureMode, CaptureStorage, HirCallArgument, HirCallKind, HirCallableDomain, HirCapture, HirExpr, HirExprKind, HirResolvedCall,
    hir_call_arg_values,
};
pub use function::HirFunction;
pub use identifier::HirIdentifier;
pub use r#impl::{HirAssociatedTypeBinding, HirDerive, HirImpl, HirTraitBound, HirWhereConstraint};
pub use module::{HirCompileWarning, HirDependencySemanticExport, HirDocumentation, HirImport, HirImportBinding, HirModule, HirTypeAlias};
pub use property::HirProperty;
pub use statement::{HirArgument, HirAttribute, HirBlock, HirMatchArm, HirStatement, HirStatementKind};
pub use r#struct::{AbstractPropertyRequirement, HirField, HirParent, HirStruct, InheritancePermission};
pub use r#trait::{HirAssociatedConst, HirAssociatedType, HirTrait};
pub use type_family::{HirTypeFamily, HirTypeFunction};
pub use visibility::HirVisibility;
pub use widget::{HirSingleton, HirWidget, HirWidgetLifecycle};
