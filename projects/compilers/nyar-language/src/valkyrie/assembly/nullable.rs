//! Nullable intrinsic profiles collected from HIR/MIR for [`super::AssembledFragment`].
//!
//! Profile structs live in `nyar-types` so language and driver share one ABI.

use std::collections::BTreeSet;

use nyar::QualifiedName;

use crate::{MirLowerer, MirOperand, MirOperation, types::hir::HirModule};

pub use nyar_types::{FragmentNullableBoolProfile, FragmentNullableIntrinsicKind, FragmentNullableIntrinsicUse, FragmentNullableTryCall};

pub(crate) fn collect_nullable_intrinsics_from_mir(module: &HirModule) -> Vec<FragmentNullableIntrinsicUse> {
    let mir = MirLowerer::lower_module_semantic(module);
    let mut uses = Vec::new();
    let mut seen = BTreeSet::new();
    for function in &mir.functions {
        let caller = function_symbol(&function.symbol);
        for block in &function.blocks {
            for instruction in &block.instructions {
                let MirOperation::Call { callee, .. } = &instruction.kind
                else {
                    continue;
                };
                let MirOperand::Symbol(path) = callee
                else {
                    continue;
                };
                let callee_name = path.name();
                let name = callee_name.as_str();
                let kind = match name {
                    "is_null" => Some(FragmentNullableIntrinsicKind::IsNull),
                    "unwrap_null" => Some(FragmentNullableIntrinsicKind::UnwrapNull),
                    _ => None,
                };
                let Some(kind) = kind
                else {
                    continue;
                };
                let key = (caller.to_string(), name.to_string());
                if seen.insert(key) {
                    uses.push(FragmentNullableIntrinsicUse { caller: caller.clone(), kind });
                }
            }
        }
    }
    uses
}

pub(crate) fn collect_nullable_try_calls(_module: &HirModule) -> Vec<FragmentNullableTryCall> {
    Vec::new()
}

pub(crate) fn collect_nullable_bool_profiles(_module: &HirModule) -> Vec<FragmentNullableBoolProfile> {
    Vec::new()
}

fn function_symbol(symbol: &str) -> QualifiedName {
    QualifiedName::new(symbol.split("::").map(nyar::Identifier::new).collect())
}
