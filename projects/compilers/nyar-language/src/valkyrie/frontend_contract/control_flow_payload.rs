//! Future/suspend 协议校验与 witness 绑定（语言语义层）。

use nyar::SuspendWitnessBinding;

use crate::valkyrie::{
    mir::ssa::MirEffectKind,
    types::hir::{HirImpl, HirModule, ValkyrieType},
};

/// 与 [`witness_bindings_for_effect`] 等价的入口，但同时返回在合成假 witness 时产生的诊断。
pub fn witness_bindings_for_effect_with_diagnostics(
    hir_module: &HirModule,
    effect: MirEffectKind,
    payload_type: Option<&ValkyrieType>,
) -> (Vec<SuspendWitnessBinding>, Vec<ProtocolDiagnostic>) {
    let mut diagnostics: Vec<ProtocolDiagnostic> = Vec::new();
    let bindings = match effect {
        MirEffectKind::DelegateYield if generator_trait_name(payload_type).is_some() => {
            collect_witness_binding(hir_module, "Iterator", "next", 0, payload_type, &mut diagnostics)
        }
        MirEffectKind::Await if future_trait_name(payload_type).is_some() => {
            collect_future_poll_and_output_bindings(hir_module, payload_type, &mut diagnostics)
        }
        MirEffectKind::AsyncSpawn if future_trait_name(payload_type).is_some() => {
            collect_witness_binding(hir_module, "Future", "awake", 0, payload_type, &mut diagnostics)
        }
        MirEffectKind::AsyncBlock if future_trait_name(payload_type).is_some() => {
            collect_future_poll_and_output_bindings(hir_module, payload_type, &mut diagnostics)
        }
        _ => Vec::new(),
    };
    (bindings, diagnostics)
}

fn collect_future_poll_and_output_bindings(
    hir_module: &HirModule,
    payload_type: Option<&ValkyrieType>,
    diagnostics: &mut Vec<ProtocolDiagnostic>,
) -> Vec<SuspendWitnessBinding> {
    let mut bindings = collect_witness_binding(hir_module, "Future", "poll", 0, payload_type, diagnostics);
    bindings.extend(collect_witness_binding(hir_module, "Future", "output", 1, payload_type, diagnostics));
    let impl_declares_is_cancelled = find_trait_impl(hir_module, "Future")
        .map(|impl_block| impl_block.methods.iter().any(|method| method.name.as_str() == "is_cancelled"))
        .unwrap_or(false);
    if impl_declares_is_cancelled {
        if let Some(cancel_binding) = resolved_witness_binding(hir_module, "Future", "is_cancelled", 2) {
            bindings.push(cancel_binding);
        }
    }
    bindings
}

fn collect_witness_binding(
    hir_module: &HirModule,
    trait_name: &str,
    method_name: &str,
    method_index: u32,
    payload_type: Option<&ValkyrieType>,
    diagnostics: &mut Vec<ProtocolDiagnostic>,
) -> Vec<SuspendWitnessBinding> {
    if let Some(binding) = resolved_witness_binding(hir_module, trait_name, method_name, method_index) {
        return vec![binding];
    }
    // 假闭环修复：witness 无法解析时不再合成假绑定（impl_symbol: None），
    // 改为记录诊断并返回空 Vec。上游 witness_bindings_for_effect 会检查诊断并在有错误时
    // panic，阻止假绑定流入降级管线。这消除了"编译通过但运行时崩溃"的假闭环风险。
    let type_name = payload_type.and_then(|ty| match ty {
        ValkyrieType::Apply(base, _) => named_type_name(base).map(str::to_string),
        ValkyrieType::Named(name) => Some(name.as_str().to_string()),
        _ => None,
    });
    diagnostics.push(ProtocolDiagnostic::WitnessMethodUnresolved {
        type_name,
        trait_name: trait_name.to_string(),
        method_name: method_name.to_string(),
    });
    Vec::new()
}

fn resolved_witness_binding(hir_module: &HirModule, trait_name: &str, method_name: &str, method_index: u32) -> Option<SuspendWitnessBinding> {
    let trait_impl = find_trait_impl(hir_module, trait_name)?;
    let type_name = impl_type_name(trait_impl)?;
    let impl_symbol = trait_impl
        .methods
        .iter()
        .enumerate()
        .find(|(_, method)| method.name.as_str() == method_name)
        .map(|(index, method)| format!("{}.{}", type_name, method.name))
        .or_else(|| trait_impl.methods.first().map(|method| format!("{}.{}", type_name, method.name)))?;
    let resolved_index = trait_impl
        .methods
        .iter()
        .position(|method| method.name.as_str() == method_name)
        .map(|index| u32::try_from(index).unwrap_or(method_index))
        .unwrap_or(method_index);
    Some(SuspendWitnessBinding {
        trait_name: trait_name.to_string(),
        method_name: method_name.to_string(),
        method_index: resolved_index,
        type_name: Some(type_name),
        impl_symbol: Some(impl_symbol),
    })
}

fn find_trait_impl<'a>(hir_module: &'a HirModule, trait_name: &str) -> Option<&'a HirImpl> {
    hir_module
        .impls
        .iter()
        .filter(|item| item.trait_path.as_ref().is_some_and(|path| path.name().as_str() == trait_name))
        .find(|item| impl_type_name(item).is_some())
}

fn impl_type_name(trait_impl: &HirImpl) -> Option<String> {
    match &trait_impl.target {
        ValkyrieType::Named(name) => Some(name.as_str().to_string()),
        ValkyrieType::Apply(base, _) => named_type_name(base).map(str::to_string),
        _ => None,
    }
}

fn generator_trait_name(payload_type: Option<&ValkyrieType>) -> Option<&str> {
    match payload_type {
        Some(ValkyrieType::Apply(base, _)) => match named_type_name(base) {
            Some("Generator" | "Iterator" | "Coroutine") => Some("Iterator"),
            _ => None,
        },
        Some(ValkyrieType::Named(name)) if matches!(name.as_str(), "Generator" | "Iterator" | "Coroutine") => Some("Iterator"),
        _ => None,
    }
}

fn future_trait_name(payload_type: Option<&ValkyrieType>) -> Option<&str> {
    match payload_type {
        Some(ValkyrieType::Apply(base, _)) => match named_type_name(base) {
            Some("Future" | "Promise") => Some("Future"),
            _ => None,
        },
        _ => None,
    }
}

fn named_type_name(ty: &ValkyrieType) -> Option<&str> {
    match ty {
        ValkyrieType::Named(name) => Some(name.as_str()),
        ValkyrieType::Apply(base, _) => named_type_name(base),
        _ => None,
    }
}

/// Future 协议校验产生的诊断。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolDiagnostic {
    UnresolvedImpl { type_name: String, trait_name: String },
    MissingMethod { type_name: String, trait_name: String, missing_method: String },
    WitnessMethodUnresolved { type_name: Option<String>, trait_name: String, method_name: String },
}

/// 校验 payload 的静态类型是否满足正式 `Future` 协议。
pub fn validate_future_protocol(hir_module: &HirModule, payload_type: &ValkyrieType) -> Result<(), ProtocolDiagnostic> {
    let type_name = named_type_name(payload_type).unwrap_or("").to_string();
    let trait_name = "Future";
    let trait_impl = match find_trait_impl(hir_module, trait_name) {
        Some(impl_block) => impl_block,
        None => {
            return Err(ProtocolDiagnostic::UnresolvedImpl { type_name, trait_name: trait_name.to_string() });
        }
    };
    for required_method in ["poll", "output"] {
        if !trait_impl.methods.iter().any(|method| method.name.as_str() == required_method) {
            return Err(ProtocolDiagnostic::MissingMethod {
                type_name: type_name.clone(),
                trait_name: trait_name.to_string(),
                missing_method: required_method.to_string(),
            });
        }
    }
    Ok(())
}
