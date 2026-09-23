use nyar::ExternalImportLink;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClrHostMethodTarget<'a> {
    pub assembly: &'a str,
    pub owner: &'a str,
    pub method: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JvmHostPrintTarget<'a> {
    pub field_owner: &'a str,
    pub field_name: &'a str,
    pub stream_owner: &'a str,
    pub method_name: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WasmHostImportTarget<'a> {
    pub module: &'a str,
    pub field: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WasiHostImportTarget<'a> {
    pub module: &'a str,
    pub function: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Win32HostImportTarget<'a> {
    pub dll: &'a str,
    pub symbol: &'a str,
}

pub(crate) fn clr_host_method_target(link: &ExternalImportLink) -> Option<ClrHostMethodTarget<'_>> {
    if !link.matches_boundary("host") {
        return None;
    }

    match link.locator_segments() {
        [assembly, owner, method, ..] => Some(ClrHostMethodTarget { assembly, owner, method }),
        _ => None,
    }
}

/// How to repair the evaluation stack after a BCL call whose MSIL return differs from the
/// Valkyrie/host-contract desired type (flatten often copies the contract return onto the FFI).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClrHostReturnAdapt {
    /// Stack already matches `desired` (including void with nothing pushed).
    Identity,
    /// Pop the BCL reference return; if `push_true`, push `ldc.i4.1` for a bool contract.
    DiscardRef { push_true: bool },
    /// BCL returns void; push `ldc.i4.1` so a bool contract/local gets a value.
    PushTrue,
}

pub(crate) fn jvm_host_print_target(link: &ExternalImportLink) -> Option<JvmHostPrintTarget<'_>> {
    if !link.matches_boundary("host") {
        return None;
    }

    match link.locator_segments() {
        [stream_owner, method_name] => {
            Some(JvmHostPrintTarget { field_owner: "java/lang/System", field_name: "out", stream_owner, method_name })
        }
        [field_owner, field_name, stream_owner, method_name] => Some(JvmHostPrintTarget { field_owner, field_name, stream_owner, method_name }),
        _ => None,
    }
}

pub(crate) fn wasm_host_import_target(link: &ExternalImportLink) -> Option<WasmHostImportTarget<'_>> {
    if !link.matches_boundary("host") || !link.matches_platform_tag("wasm") {
        return None;
    }

    match link.locator_segments() {
        [module, field, ..] => Some(WasmHostImportTarget { module, field }),
        _ => None,
    }
}

pub(crate) fn wasi_host_import_target(link: &ExternalImportLink) -> Option<WasiHostImportTarget<'_>> {
    if !link.matches_boundary("host") {
        return None;
    }

    let module_looks_wasi = link.locator_segments().first().is_some_and(|module| module.starts_with("wasi:"));
    if !link.matches_platform_tag("wasi") && !module_looks_wasi {
        return None;
    }

    match link.locator_segments() {
        [module, function] => Some(WasiHostImportTarget { module, function }),
        _ => None,
    }
}

pub(crate) fn win32_host_import_target(link: &ExternalImportLink) -> Option<Win32HostImportTarget<'_>> {
    if !link.matches_host_platform("win32") {
        return None;
    }

    match link.locator_segments() {
        [dll, symbol] => Some(Win32HostImportTarget { dll, symbol }),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LinuxGnuHostImportTarget<'a> {
    pub library: &'a str,
    pub symbol: &'a str,
}

pub(crate) fn linux_gnu_host_import_target(link: &ExternalImportLink) -> Option<LinuxGnuHostImportTarget<'_>> {
    if link.matches_host_platform("linux-gnu") || link.matches_host_platform("linux") {
        return match link.locator_segments() {
            [library, symbol] => Some(LinuxGnuHostImportTarget { library, symbol }),
            _ => None,
        };
    }

    // Freestanding Linux print / I/O edges may be declared as `[syscall(N)]`
    // without a platform tag; recognize them as linux-gnu host imports.
    match link.locator_segments() {
        [library, number] if library.as_str() == "syscall" => Some(LinuxGnuHostImportTarget { library, symbol: number }),
        _ => None,
    }
}
