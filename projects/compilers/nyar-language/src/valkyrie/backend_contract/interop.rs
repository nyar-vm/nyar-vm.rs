use nyar::{CapabilityTag, ExternalImportLink, Identifier, QualifiedName, RuntimeRequirement};
use nyar_types::DiagnosticSpan;

use crate::valkyrie::types::{
    NamePath,
    hir::{HirArgument, HirAttribute, HirExprKind, HirFunction, HirLiteral, HirModule, HirStringSegment},
};

/// backend contract 对声明面互操作能力的事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendContractError {
    /// 声明使用了 WASI Preview1 host import 位置，Component Model 不支持。
    WasiPreview1ImportForbidden {
        /// 命中的禁用 locator 段。
        segments: Vec<String>,
        /// 属性参数 span。
        span: DiagnosticSpan,
    },
}

impl BackendContractError {
    /// 诊断主定位 span。
    pub fn span(&self) -> DiagnosticSpan {
        match self {
            Self::WasiPreview1ImportForbidden { span, .. } => span.clone(),
        }
    }
}

impl std::fmt::Display for BackendContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WasiPreview1ImportForbidden { segments, .. } => write!(
                f,
                "WASI supports Component Model only, `{}` must be replaced with a `wasi:...` component import",
                segments.join("` / `")
            ),
        }
    }
}

/// C ABI FFI 属性坝（`[c("lib", "sym")]`）。
///
/// 兝许用户通过 FFI 链接任愝 C 库（坫 libc），但标准库与编译器违行时本身
/// 丝依赖 libc 系列，平坰基础能力走 `[syscall(...)]`。
const C_BINDING_ATTRIBUTE: &str = "c";
/// 内核系统调用绑定属性坝。
const SYSCALL_BINDING_ATTRIBUTE: &str = "syscall";

/// 当剝外部导入链接覝求的能力标签。
pub(crate) fn required_capability(_link: &ExternalImportLink) -> CapabilityTag {
    CapabilityTag::new("host-interop")
}

/// 当剝外部导入链接覝求的违行时需求。
pub(crate) fn runtime_requirement(_link: &ExternalImportLink) -> RuntimeRequirement {
    RuntimeRequirement { key: "host-interop".to_string(), value: "required".to_string() }
}

pub(crate) fn validate_interop_surface(module: &HirModule) -> Result<(), BackendContractError> {
    for function in &module.functions {
        validate_function_interop_surface(function)?;
    }
    for struct_decl in &module.structs {
        for method in &struct_decl.methods {
            validate_function_interop_surface(method)?;
        }
    }
    for trait_decl in &module.traits {
        for method in &trait_decl.methods {
            validate_function_interop_surface(method)?;
        }
        for method in &trait_decl.default_methods {
            validate_function_interop_surface(method)?;
        }
    }
    for impl_block in &module.impls {
        for method in &impl_block.methods {
            validate_function_interop_surface(method)?;
        }
    }
    for singleton in &module.singletons {
        for method in &singleton.methods {
            validate_function_interop_surface(method)?;
        }
        if let Some(constructor) = &singleton.constructor {
            validate_function_interop_surface(constructor)?;
        }
        if let Some(finalizer) = &singleton.finalizer {
            validate_function_interop_surface(finalizer)?;
        }
    }
    for submodule in &module.submodules {
        validate_interop_surface(submodule)?;
    }
    Ok(())
}

/// 从函数声明杝坖语言中性的外部导入链接。
#[doc(hidden)]
pub fn function_interop_contract(function: &HirFunction) -> Option<ExternalImportLink> {
    function.annotations.iter().find_map(attribute_to_interop_contract).or_else(|| {
        function.annotations.iter().find_map(|attribute| {
            (attribute_name(attribute).as_deref() == Some("host_contract")).then(|| ExternalImportLink::host(None, Vec::new()))
        })
    })
}

/// 从 `[host_provider(X)]` 属性中杝坖目标契约符坷。
///
/// 该函数返回 provider 所实现的 host_contract 的陝定坝。
/// 例如 `[host_provider(std::console::write_line)]` 返回 `std::console::write_line`。
pub(crate) fn function_host_provider_target(function: &HirFunction) -> Option<QualifiedName> {
    function.annotations.iter().find_map(|attribute| {
        let name = attribute_name(attribute)?;
        if name != "host_provider" {
            return None;
        }
        attribute.arguments.iter().find_map(|argument| argument_qualified_name(argument))
    })
}

fn validate_function_interop_surface(function: &HirFunction) -> Result<(), BackendContractError> {
    for attribute in &function.annotations {
        validate_interop_attribute(attribute)?;
    }
    Ok(())
}

fn validate_interop_attribute(attribute: &HirAttribute) -> Result<(), BackendContractError> {
    let Some(name) = attribute_name(attribute)
    else {
        return Ok(());
    };

    if name != "wasi" {
        return Ok(());
    }

    let locator_segments = attribute.arguments.iter().filter_map(argument_string_literal).collect::<Vec<_>>();
    let forbidden_segments = locator_segments
        .iter()
        .filter(|segment| matches!(segment.as_str(), "wasi_snapshot_preview1" | "fd_write" | "fd_read"))
        .cloned()
        .collect::<Vec<_>>();
    if forbidden_segments.is_empty() {
        return Ok(());
    }

    let span = attribute_argument_span(attribute);
    Err(BackendContractError::WasiPreview1ImportForbidden {
        segments: forbidden_segments,
        span: DiagnosticSpan { start: span.start, end: span.end },
    })
}

fn attribute_argument_span(attribute: &HirAttribute) -> std::ops::Range<usize> {
    attribute
        .arguments
        .first()
        .map(|argument| {
            let start = usize::try_from(argument.value.span.get_start()).unwrap_or(0);
            let end = usize::try_from(argument.value.span.get_end()).unwrap_or(start);
            start..end
        })
        .unwrap_or(0..0)
}

fn attribute_to_interop_contract(attribute: &HirAttribute) -> Option<ExternalImportLink> {
    let Some(name) = attribute_name(attribute)
    else {
        return None;
    };

    if name == SYSCALL_BINDING_ATTRIBUTE {
        return syscall_attribute_to_interop_contract(attribute);
    }

    let locator_segments = attribute.arguments.iter().filter_map(argument_string_literal).collect::<Vec<_>>();
    if locator_segments.len() != attribute.arguments.len() {
        return None;
    }

    if name == C_BINDING_ATTRIBUTE {
        let library = locator_segments.first()?;
        let platform = c_binding_platform(library)?;
        return Some(ExternalImportLink::host(Some(Identifier::new(platform)), locator_segments));
    }

    if is_host_import_surface_tag(name) {
        // Preserve the surface tag (`wasm` / `wasi` / `clr` / …) so backends can
        // filter host imports by target instead of treating every host link as WASI.
        return Some(ExternalImportLink::host(Some(Identifier::new(name)), locator_segments));
    }

    None
}

fn syscall_attribute_to_interop_contract(attribute: &HirAttribute) -> Option<ExternalImportLink> {
    let mut locator_segments = vec!["syscall".to_string()];
    for argument in &attribute.arguments {
        locator_segments.push(argument_locator_token(argument)?);
    }
    if locator_segments.len() < 2 {
        return None;
    }
    Some(ExternalImportLink::host(None, locator_segments))
}

fn argument_locator_token(argument: &HirArgument) -> Option<String> {
    match &argument.value.kind {
        HirExprKind::Literal(HirLiteral::String(_)) => argument_string_literal(argument),
        HirExprKind::Literal(HirLiteral::Integer64(value)) => Some(value.to_string()),
        _ => None,
    }
}

fn c_binding_platform(library: &str) -> Option<&'static str> {
    match library {
        // Windows DLL / NT
        "kernel32" | "user32" | "ole32" | "ws2_32" | "ntdll" => Some("win32"),
        // Apple frameworks / ObjC runtime（非 C 违行时库，但是平坰 FFI）
        "Foundation" | "CoreFoundation" | "CoreGraphics" | "ApplicationServices" | "objc" => Some("darwin"),
        // POSIX C 违行时（用户 FFI 兝许；std / 编译器自身丝依赖）
        "libc" | "libpthread" | "libm" | "libdl" | "librt" | "linux" => Some("linux-gnu"),
        // Darwin C 违行时（用户 FFI 兝许）
        "libSystem" => Some("darwin"),
        _ => None,
    }
}

fn is_host_import_surface_tag(name: &str) -> bool {
    // `js_builtin` / `vm`：抽象宿主绑定，须进入 external import 合同，
    // 否则 Invoke(ItemInstanceId) 在 link 阶段报「没有定义或显式导入合同」。
    matches!(name, "clr" | "wasm" | "wasi" | "jvm" | "com" | "js_builtin" | "vm")
}

fn attribute_name(attribute: &HirAttribute) -> Option<&str> {
    attribute.name.parts().first().map(|name| name.as_str())
}

fn argument_string_literal(argument: &HirArgument) -> Option<String> {
    let HirExprKind::Literal(HirLiteral::String(literal)) = &argument.value.kind
    else {
        return None;
    };

    let mut rendered = String::new();
    for segment in &literal.segments {
        let HirStringSegment::Text(text) = segment
        else {
            return None;
        };
        rendered.push_str(text);
    }
    Some(rendered)
}

/// 从属性坂数中杝坖陝定坝，支挝路径表达弝和字符串字面針两秝形弝。
fn argument_qualified_name(argument: &HirArgument) -> Option<QualifiedName> {
    match &argument.value.kind {
        HirExprKind::Path(path) => Some(qualified_name_from_path(path)),
        HirExprKind::Literal(HirLiteral::String(literal)) => {
            let mut rendered = String::new();
            for segment in &literal.segments {
                let HirStringSegment::Text(text) = segment
                else {
                    return None;
                };
                rendered.push_str(text);
            }
            let parts = rendered.split("::").map(Identifier::new).collect::<Vec<_>>();
            Some(QualifiedName::new(parts))
        }
        _ => None,
    }
}

fn qualified_name_from_path(path: &NamePath) -> QualifiedName {
    QualifiedName::new(path.parts().to_vec())
}
