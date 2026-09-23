use nyar::{CapabilityTag, ExternalImportLink, Identifier, QualifiedName, RuntimeRequirement};
use std_data::text::valkyrie::ParseError;

use crate::valkyrie::types::{
    NamePath,
    hir::{HirArgument, HirAttribute, HirExprKind, HirFunction, HirLiteral, HirModule, HirStringSegment},
};

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

pub(crate) fn validate_interop_surface(module: &HirModule) -> Result<(), ParseError> {
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
pub(crate) fn function_interop_contract(function: &HirFunction) -> Option<ExternalImportLink> {
    function
        .annotations
        .iter()
        .find_map(attribute_to_interop_contract)
        .or_else(|| function.is_abstract.then(|| ExternalImportLink::host(None, Vec::new())))
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

fn validate_function_interop_surface(function: &HirFunction) -> Result<(), ParseError> {
    for attribute in &function.annotations {
        validate_interop_attribute(attribute)?;
    }
    Ok(())
}

fn validate_interop_attribute(attribute: &HirAttribute) -> Result<(), ParseError> {
    let Some(name) = attribute_name(attribute)
    else {
        return Ok(());
    };

    if name != "wasi" {
        return Ok(());
    }

    let locator_segments = attribute.arguments.iter().filter_map(argument_string_literal).collect::<Vec<_>>();
    let forbidden_preview1 =
        locator_segments.iter().any(|segment| segment == "wasi_snapshot_preview1" || segment == "fd_write" || segment == "fd_read");
    if !forbidden_preview1 {
        return Ok(());
    }

    Err(ParseError::invalid_at(
        "WASI 坪支挝 Component Model；禝止使用 `wasi_snapshot_preview1` / `fd_write` / `fd_read`，请改为 `wasi:...` component import",
        attribute_argument_span(attribute),
    ))
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
    matches!(name, "clr" | "wasm" | "wasi" | "jvm" | "com")
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

#[cfg(test)]
mod tests {
    use nyar_types::SourceID;

    use super::*;
    use crate::valkyrie::hir::ValkyrieCompiler;

    #[test]
    fn maps_c_binding_attribute_to_win32_host_link() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 900 });
        let module = compiler
            .compile_source(
                r#"
[c("kernel32", "WriteFile")]
micro helper(message: utf8): unit {
    return;
}
"#,
            )
            .unwrap();
        let contract = function_interop_contract(&module.functions[0]).expect("expected interop contract");
        assert!(contract.matches_host_platform("win32"));
        assert_eq!(contract.locator_segments(), &["kernel32", "WriteFile"]);
    }

    #[test]
    fn maps_syscall_binding_to_neutral_host_link() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 901 });
        let module = compiler
            .compile_source(
                r#"
[syscall(1)]
micro helper(message: utf8): unit {
    return;
}
"#,
            )
            .unwrap();
        let contract = function_interop_contract(&module.functions[0]).expect("expected interop contract");
        assert!(contract.matches_boundary("host"));
        assert!(contract.platform_tag.is_none());
        assert_eq!(contract.locator_segments(), &["syscall", "1"]);
    }

    #[test]
    fn maps_abstract_syscall_binding_and_call_edge() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 902 });
        let module = compiler
            .compile_source(
                r#"
[syscall(1)]
micro console_write(message: utf8): i32;

[main]
micro main() -> i64 {
    console_write("hello from linux")
    return 0;
}
"#,
            )
            .unwrap();
        let write = module.functions.iter().find(|function| function.name.as_str() == "console_write").expect("console_write");
        assert!(write.is_abstract);
        let contract = function_interop_contract(write).expect("expected syscall contract");
        assert_eq!(contract.locator_segments(), &["syscall", "1"]);

        let build_output = compiler
            .compile_source_to_build_output(
                r#"
[syscall(1)]
micro console_write(message: utf8): i32;

[main]
micro main() -> i64 {
    console_write("hello from linux")
    return 0;
}
"#,
            )
            .unwrap();
        let plan = build_output.neutral_plan();
        assert_eq!(plan.semantic_fragments[0].external_import_links.len(), 1, "links={:?}", plan.semantic_fragments[0].external_import_links);
        assert_eq!(plan.semantic_fragments[0].external_call_edges.len(), 1, "edges={:?}", plan.semantic_fragments[0].external_call_edges);
    }

    #[test]
    fn maps_c_libc_binding_to_linux_gnu_host_link() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 910 });
        let module = compiler
            .compile_source(
                r#"
[c("libc", "write")]
micro helper(message: utf8): unit {
    return;
}
"#,
            )
            .unwrap();
        let contract = function_interop_contract(&module.functions[0]).expect("expected interop contract");
        assert!(contract.matches_host_platform("linux-gnu"));
        assert_eq!(contract.locator_segments(), &["libc", "write"]);
    }

    #[test]
    fn maps_c_libsystem_binding_to_darwin_host_link() {
        let compiler = ValkyrieCompiler::new(SourceID { version_id: 911 });
        let module = compiler
            .compile_source(
                r#"
[c("libSystem", "write")]
micro helper(message: utf8): unit {
    return;
}
"#,
            )
            .unwrap();
        let contract = function_interop_contract(&module.functions[0]).expect("expected interop contract");
        assert!(contract.matches_host_platform("darwin"));
        assert_eq!(contract.locator_segments(), &["libSystem", "write"]);
    }

    #[test]
    fn collapses_host_import_shapes_into_same_neutral_contract() {
        for (version_id, tag, arguments) in [
            (801, "clr", r#""mscorlib", "System.Console", "WriteLine""#),
            (802, "wasm", r#""env", "memory""#),
            (803, "wasi", r#""wasi:io/streams", "blocking-write-and-flush""#),
            (804, "wasi", r#""wasi:clocks/monotonic-clock", "now""#),
            (805, "jvm", r#""java/lang/System", "currentTimeMillis""#),
            (806, "com", r#""Excel.Application", "Visible", "set""#),
        ] {
            let compiler = ValkyrieCompiler::new(SourceID { version_id });
            let module = compiler
                .compile_source(&format!(
                    r#"
[{tag}({arguments})]
micro helper(message: utf16) {{
    return;
}}
"#
                ))
                .unwrap();
            let contract = function_interop_contract(&module.functions[0]).expect("expected interop contract");

            assert!(contract.matches_boundary("host"));
            assert!(contract.matches_platform_tag(tag), "expected platform_tag={tag}, got {:?}", contract.platform_tag);
            assert_eq!(contract.locator_segments().len(), module.functions[0].annotations[0].arguments.len());
        }
    }

    #[test]
    fn rejects_wasi_preview1_surface_imports() {
        for (version_id, source) in [
            (
                807,
                r#"
[wasi("wasi_snapshot_preview1", "fd_write")]
micro console_write_line(message: utf8): unit;
"#,
            ),
            (
                808,
                r#"
[wasi("fd_write")]
micro console_write_line(message: utf8): unit;
"#,
            ),
            (
                809,
                r#"
[wasi("fd_read")]
micro console_read(message: utf8): unit;
"#,
            ),
        ] {
            let compiler = ValkyrieCompiler::new(SourceID { version_id });
            let error = compiler.compile_source(source).expect_err("preview1 fd I/O should be rejected");

            match error {
                ParseError::Invalid { message, .. } => {
                    assert!(message.contains("Component Model"), "message: {message}");
                }
                other => panic!("unexpected error: {other:?}"),
            }
        }
    }

    #[test]
    fn program_facts_include_imported_wasm_host_links() {
        use crate::valkyrie::{frontend_contract::hir_module_to_program_facts, types::hir::HirDependencySemanticExport};
        use nyar::Identifier;
        use nyar_types::NamePath;

        let compiler = ValkyrieCompiler::new(SourceID { version_id: 920 });
        let dependency = compiler
            .compile_source(
                r#"
namespace std.io;

[wasm("env", "directory_exists")]
private micro __host_directory_exists(path: utf8): bool;

micro directory_exists(path: utf8) -> bool {
    return __host_directory_exists(path)
}
"#,
            )
            .expect("dependency module");

        let mut consumer = compiler
            .compile_source(
                r#"
[main]
micro main() -> i32 {
    return 0;
}
"#,
            )
            .expect("consumer module");
        consumer.imported_semantic_exports = vec![HirDependencySemanticExport {
            module: NamePath::new(vec![Identifier::new("std")]),
            functions: dependency.functions.clone(),
            structs: Vec::new(),
            enums: Vec::new(),
            traits: Vec::new(),
            type_aliases: Vec::new(),
            impls: Vec::new(),
        }];

        let facts = hir_module_to_program_facts(&consumer);
        let host = facts
            .functions
            .iter()
            .find(|function| {
                function.symbol.to_string().contains("__host_directory_exists")
                    || function
                        .external_import_link
                        .as_ref()
                        .is_some_and(|link| link.locator_segments().last().map(String::as_str) == Some("directory_exists"))
            })
            .expect("imported wasm host link missing from program facts");
        assert!(host.external_import_link.is_some());
    }
}
