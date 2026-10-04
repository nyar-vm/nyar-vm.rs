use miette::{Result, miette};
use nyar::{BackendCandidate, BackendSelector, PartitionBackendRequirement};

use crate::{DriverCompileReport, DriverCompileRequest};

#[cfg(feature = "nyar-vm-lane")]
mod nyar_vm;
mod wasm;

trait BundledBackendCompiler: Sync {
    fn compile(&self, request: DriverCompileRequest<'_>) -> Result<DriverCompileReport>;
}

struct DriverCompilerRegistration {
    name: &'static str,
    priority: u16,
    supports: fn(&PartitionBackendRequirement) -> bool,
    compiler: &'static dyn BundledBackendCompiler,
}

impl DriverCompilerRegistration {
    fn candidate(&self, requirement: &PartitionBackendRequirement) -> Option<BackendCandidate> {
        (self.supports)(requirement).then(|| BackendCandidate {
            name: self.name.to_string(),
            requirement: requirement.clone(),
            priority: self.priority,
        })
    }
}

static WASM_COMPILER: wasm::WasmFamilyCompiler = wasm::WasmFamilyCompiler;
#[cfg(feature = "nyar-vm-lane")]
static NYAR_VM_COMPILER: nyar_vm::NyarVmFamilyCompiler = nyar_vm::NyarVmFamilyCompiler;

#[cfg(feature = "nyar-vm-lane")]
static DRIVER_COMPILERS: [DriverCompilerRegistration; 2] = [
    DriverCompilerRegistration { name: "wasm-binary", priority: 100, supports: wasm::supports_requirement, compiler: &WASM_COMPILER },
    DriverCompilerRegistration { name: "nyar-vm", priority: 100, supports: nyar_vm::supports_requirement, compiler: &NYAR_VM_COMPILER },
];

#[cfg(not(feature = "nyar-vm-lane"))]
static DRIVER_COMPILERS: [DriverCompilerRegistration; 1] =
    [DriverCompilerRegistration { name: "wasm-binary", priority: 100, supports: wasm::supports_requirement, compiler: &WASM_COMPILER }];

pub(crate) fn compile(request: DriverCompileRequest<'_>) -> Result<DriverCompileReport> {
    let mut selector = BackendSelector::default();
    let mut matched_compilers = Vec::new();
    for registration in DRIVER_COMPILERS.iter() {
        if let Some(candidate) = registration.candidate(&request.requirement) {
            selector.register(candidate);
            matched_compilers.push((registration.name, registration.compiler));
        }
    }

    let Some(selected) = selector.select(&request.requirement)
    else {
        return Err(miette!("`emitter` 找不到满足需求的 backend 编译器：{:?}", request.requirement));
    };
    let Some((_, compiler)) = matched_compilers.into_iter().find(|(name, _)| *name == selected.name)
    else {
        return Err(miette!("选中的 backend `{}` 没有关联 driver compiler", selected.name));
    };
    compiler.compile(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nyar::{HostProjectionBoundary, TargetBackendFamily};

    #[test]
    fn frozen_families_have_neither_capability_nor_lowering() {
        for (family, boundary) in [
            (TargetBackendFamily::Clr, HostProjectionBoundary::Clr),
            (TargetBackendFamily::Jvm, HostProjectionBoundary::Jvm),
            (TargetBackendFamily::Native, HostProjectionBoundary::Native),
        ] {
            assert!(crate::bundled_backend_capability_descriptor(family).is_none());
            let result = crate::lowering::lower_fragment_to_driver_input(
                &crate::FragmentSubmission::default(),
                family,
                boundary,
                std::path::PathBuf::new(),
                "",
                crate::nyar_backend_wasi::WasmPackageKind::Binary,
            );
            let error = match result {
                Ok(_) => panic!("冻结目标不得存在正式 lowering"),
                Err(error) => error,
            };
            assert!(error.to_string().contains("拒绝切换目标或补造产物"), "{error}");
        }
        assert!(DRIVER_COMPILERS.iter().all(|registration| matches!(registration.name, "wasm-binary" | "nyar-vm")));
    }

    #[cfg(not(feature = "nyar-vm-lane"))]
    #[test]
    fn disabled_vm_does_not_advertise_capability() {
        assert!(crate::bundled_backend_capability_descriptor(TargetBackendFamily::NyarVm).is_none());
    }
}
