use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use miette::{IntoDiagnostic, Result, WrapErr};
use nyar::{
    abstractions::ArtifactFormat,
    packaging::{ArtifactDescriptor, TargetLane},
};

use crate::backend::binding_builders::{BindingGenerationContext, HostBindingBuilder};

/// WASI component-model preview / package train used by the shared wasm emit path.
///
/// Core wasm codegen is shared; only WIT package versions and command-world
/// exports differ between Preview2 (`wasip2`) and 0.3 (`wasip3`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WasiPreview {
    /// WASI 0.2.x (`wasip2`) package train.
    #[default]
    Preview2,
    /// WASI 0.3.x (`wasip3`) package train.
    Preview3,
}

impl WasiPreview {
    /// Package version stamped onto unversioned `wasi:*` imports / exports.
    pub fn package_version(self) -> &'static str {
        match self {
            Self::Preview2 => WASI_PREVIEW2_VERSION,
            Self::Preview3 => WASI_PREVIEW3_VERSION,
        }
    }

    /// Derive preview from a host-flavor token (`wasi-component-model` / `…-p3`).
    pub fn from_host_flavor(host_flavor: &str) -> Self {
        let lower = host_flavor.to_ascii_lowercase();
        if lower.contains("wasip3") || lower.ends_with("-p3") || lower.contains("component-model-p3") { Self::Preview3 } else { Self::Preview2 }
    }
}

/// `WIT` 文本接口描述生成器。
///
/// 该生成器产出 `.component.wit` 文本文件，描述 `WASM` 模块对宿主暴露的接口契约，
/// 并显式生成 `world command`。
///
/// 这里仍然只是文本级镜像，不等同于真正的 `WASI component model` 二进制包装：
/// - 当前只把导入按本地 interface 形式镜像到 WIT 文本；
/// - 外部依赖（例如 `wasi:*` 包）尚未 vendoring/lock 成可直接喂给 component
///   封装器的完整依赖树；
/// - 真正的 component 二进制生成仍需后续单独实现。
pub(crate) struct WitBindingBuilder;

impl HostBindingBuilder for WitBindingBuilder {
    fn build(&self, context: &BindingGenerationContext<'_>) -> Result<Vec<ArtifactDescriptor>> {
        write_component_wit_package_for(context.output_dir, context.artifact_name, context.imports, context.wasi_preview)?;

        Ok(vec![ArtifactDescriptor {
            name: format!("{}.component", context.artifact_name),
            kind: nyar::ArtifactKind::AssemblyListing,
            format: ArtifactFormat::RawBinary,
            target: context.target.clone(),
            lane: TargetLane::Wasm,
        }])
    }
}

pub(crate) fn write_component_wit_package(output_dir: &Path, artifact_name: &str, imports: &[(String, String)]) -> Result<PathBuf> {
    write_component_wit_package_for(output_dir, artifact_name, imports, WasiPreview::Preview2)
}

pub(crate) fn write_component_wit_package_for(
    output_dir: &Path,
    artifact_name: &str,
    imports: &[(String, String)],
    preview: WasiPreview,
) -> Result<PathBuf> {
    let package_dir = output_dir.join(format!("{}.component-wit", artifact_name));
    if package_dir.exists() {
        fs::remove_dir_all(&package_dir)
            .into_diagnostic()
            .wrap_err_with(|| format!("清理旧的 WIT package 目录失败：{}", package_dir.display()))?;
    }
    fs::create_dir_all(package_dir.join("deps"))
        .into_diagnostic()
        .wrap_err_with(|| format!("创建 WIT package 目录失败：{}", package_dir.display()))?;

    let wit_path = package_dir.join("component.wit");
    let wit_text = build_component_wit_document_for(artifact_name, imports, preview);
    fs::write(&wit_path, wit_text).into_diagnostic().wrap_err_with(|| format!("写入 `WIT` 接口描述失败：{}", wit_path.display()))?;

    for (file_name, dependency_text) in build_dependency_documents_for(imports, preview) {
        let dependency_path = package_dir.join("deps").join(file_name);
        fs::write(&dependency_path, dependency_text)
            .into_diagnostic()
            .wrap_err_with(|| format!("写入 `WIT` 依赖失败：{}", dependency_path.display()))?;
    }

    Ok(package_dir)
}

pub(crate) fn package_core_wasm_as_component(core_wasm_path: &Path, wit_package_path: &Path, output_path: &Path) -> Result<()> {
    // `run_wasm_tools` executes with the artifact directory as its cwd. The
    // compiler can hand us relative paths, so normalize all paths before
    // passing them to avoid resolving `tmp/foo/tmp/foo/...`.
    let output_parent = output_path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(output_parent)
        .into_diagnostic()
        .wrap_err_with(|| format!("无法创建 WASI component 输出目录: {}", output_parent.display()))?;
    let output_parent = fs::canonicalize(output_parent).into_diagnostic().wrap_err("无法解析 WASI component 输出目录")?;
    let output_path = output_parent.join(output_path.file_name().unwrap_or_default());
    let core_wasm_path = fs::canonicalize(core_wasm_path).into_diagnostic().wrap_err("无法解析 WASI core wasm 路径")?;
    let wit_package_path = fs::canonicalize(wit_package_path).into_diagnostic().wrap_err("无法解析 WASI WIT package 路径")?;
    let embedded_path = output_path.with_extension("embedded.core.wasm");

    run_wasm_tools(
        [
            "component",
            "embed",
            wit_package_path.to_string_lossy().as_ref(),
            core_wasm_path.to_string_lossy().as_ref(),
            "--world",
            "command",
            "-o",
            embedded_path.to_string_lossy().as_ref(),
        ],
        Some(output_parent.as_path()),
        "将 WIT 元数据嵌入 core wasm 失败",
    )?;

    run_wasm_tools(
        ["component", "new", embedded_path.to_string_lossy().as_ref(), "-o", output_path.to_string_lossy().as_ref()],
        Some(output_parent.as_path()),
        "将嵌入元数据的 core wasm 封装为 component 失败",
    )?;

    let _ = fs::remove_file(&embedded_path);
    Ok(())
}

fn run_wasm_tools<const N: usize>(args: [&str; N], cwd: Option<&Path>, failure_message: &str) -> Result<()> {
    let mut command = Command::new("wasm-tools");
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command.output().into_diagnostic().wrap_err("调用 `wasm-tools` 失败")?;
    if output.status.success() {
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(miette::miette!("{failure_message}\ncommand: wasm-tools {}\nstdout:\n{}\nstderr:\n{}", args.join(" "), stdout, stderr,))
}

pub(crate) fn build_component_wit_document(artifact_name: &str, imports: &[(String, String)]) -> String {
    build_component_wit_document_for(artifact_name, imports, WasiPreview::Preview2)
}

pub(crate) fn build_component_wit_document_for(artifact_name: &str, imports: &[(String, String)], preview: WasiPreview) -> String {
    let version = preview.package_version();
    let package_name = format!("nyar:{}@0.1.0", sanitize_wit_package_segment(artifact_name));
    let local_interfaces = collect_local_interfaces(imports);
    let external_modules = collect_external_import_modules_for(imports, preview);
    let mut wit = String::new();
    wit.push_str(&format!("package {};\n\n", package_name));

    for (interface_name, fields) in &local_interfaces {
        wit.push_str(&format!("interface {} {{\n", interface_name));
        for field in fields {
            wit.push_str(&wit_func_stub_line("", interface_name.as_str(), field.as_str()));
        }
        wit.push_str("}\n\n");
    }

    wit.push_str("world command {\n");
    for (interface_name, _) in &local_interfaces {
        wit.push_str(&format!("  import {};\n", interface_name));
    }
    for module in &external_modules {
        wit.push_str(&format!("  import {};\n", module));
    }
    // Host runtimes resolve `wasi:cli/run@…#run`, not a bare `run` export.
    wit.push_str(&format!("  export wasi:cli/run@{version};\n"));
    wit.push_str("}\n");
    wit
}

/// WASI Preview2 package version used for command-world linking with wasmtime.
pub(crate) const WASI_PREVIEW2_VERSION: &str = "0.2.12";

/// WASI 0.3 (`wasip3`) package version used for command-world linking with wasmtime `-S p3`.
pub(crate) const WASI_PREVIEW3_VERSION: &str = "0.3.0";

/// Core export name for `wasi:cli/run#run` (wit-bindgen / wasmtime convention).
pub(crate) fn wasi_cli_run_export_name() -> String {
    wasi_cli_run_export_name_for(WasiPreview::Preview2)
}

pub(crate) fn wasi_cli_run_export_name_for(preview: WasiPreview) -> String {
    format!("wasi:cli/run@{}#run", preview.package_version())
}

/// Attach the default Preview2 package version to unversioned `wasi:*` import modules.
pub(crate) fn wasi_versioned_import_module(module: &str) -> String {
    wasi_versioned_import_module_for(module, WasiPreview::Preview2)
}

/// Attach a preview-specific package version to unversioned `wasi:*` import modules.
pub(crate) fn wasi_versioned_import_module_for(module: &str, preview: WasiPreview) -> String {
    if module.contains('@') || !module.starts_with("wasi:") {
        return module.to_string();
    }
    format!("{module}@{}", preview.package_version())
}

/// Adapt a source-level WASI import `(module, field)` to the selected package train.
///
/// **p3-first:** Preview3 targets emit / link against WASI 0.3 WIT (`wasmtime -S p3`).
/// Legacy Preview1/Preview2 names are remapped to their 0.3 counterparts — they are not
/// kept as a long-term guest ABI.
///
/// Preview3 remaps:
/// - clocks: `resolution` → `get-resolution`; `wall-clock` → `system-clock`
/// - console: `wasi:io/streams#blocking-write-and-flush` → `wasi:cli/stdout#write-via-stream`
///   (p3 has no `wasi:io`; stdout is stream/future based)
///
/// Returns `None` only when an import has no honest p3 equivalent yet (should be rare).
pub(crate) fn wasi_adapt_import_for_preview(module: &str, field: &str, preview: WasiPreview) -> Option<(String, String)> {
    match preview {
        WasiPreview::Preview2 => Some((module.to_string(), field.to_string())),
        WasiPreview::Preview3 => {
            let bare = module.split('@').next().unwrap_or(module);
            // p2 wasi:io → p3 wasi:cli stdout/stderr stream writes
            if bare == "wasi:io/streams" && field == "blocking-write-and-flush" {
                return Some(("wasi:cli/stdout".to_string(), "write-via-stream".to_string()));
            }
            if bare.starts_with("wasi:io/") {
                // Remaining wasi:io/* has no p3 twin; omit rather than emit a dead p2 import.
                return None;
            }
            let adapted_module = if bare == "wasi:clocks/wall-clock" || bare.starts_with("wasi:clocks/wall-clock@") {
                bare.replacen("wall-clock", "system-clock", 1)
            }
            else {
                bare.to_string()
            };
            let adapted_field =
                if field == "resolution" && (adapted_module.contains("monotonic-clock") || adapted_module.contains("system-clock")) {
                    "get-resolution"
                }
                else {
                    field
                };
            Some((adapted_module, adapted_field.to_string()))
        }
    }
}

/// Bare module names (`env`, …) become same-package WIT interfaces.
fn collect_local_interfaces(imports: &[(String, String)]) -> BTreeMap<String, Vec<String>> {
    let mut local: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (module, field) in imports {
        if split_wit_import_module(module).is_some() {
            continue;
        }
        let interface_name = sanitize_wit_identifier(module);
        let fields = local.entry(interface_name).or_default();
        let field_name = sanitize_wit_identifier(field);
        if !fields.contains(&field_name) {
            fields.push(field_name);
        }
    }
    local
}

/// `package/interface` style modules (`wasi:io/streams`, …) are world-imported as-is
/// (with preview-specific versions attached for unversioned `wasi:*` modules).
fn collect_external_import_modules(imports: &[(String, String)]) -> Vec<String> {
    collect_external_import_modules_for(imports, WasiPreview::Preview2)
}

fn collect_external_import_modules_for(imports: &[(String, String)], preview: WasiPreview) -> Vec<String> {
    let mut modules = Vec::new();
    for (module, _) in imports {
        if split_wit_import_module(module).is_none() {
            continue;
        }
        let versioned = wasi_versioned_import_module_for(module, preview);
        if !modules.contains(&versioned) {
            modules.push(versioned);
        }
    }
    modules
}

fn build_dependency_documents(imports: &[(String, String)]) -> Vec<(String, String)> {
    build_dependency_documents_for(imports, WasiPreview::Preview2)
}

fn build_dependency_documents_for(imports: &[(String, String)], preview: WasiPreview) -> Vec<(String, String)> {
    let mut dependency_map: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    for (module, field) in imports {
        let versioned = wasi_versioned_import_module_for(module, preview);
        let Some((package_name, interface_name)) = split_wit_import_module(&versioned)
        else {
            continue;
        };
        let package_interfaces = dependency_map.entry(package_name).or_default();
        let fields = package_interfaces.entry(interface_name).or_default();
        let field_name = sanitize_wit_identifier(field);
        if !fields.contains(&field_name) {
            fields.push(field_name);
        }
    }

    // Command world always exports wasi:cli/run; ensure the package document defines it.
    let cli_package = format!("wasi:cli@{}", preview.package_version());
    dependency_map.entry(cli_package.clone()).or_default().entry("run".to_string()).or_default();

    // WASI 0.3 cli stdout/stderr/stdin use `error-code` from `wasi:cli/types`.
    if preview == WasiPreview::Preview3 {
        if let Some(cli) = dependency_map.get_mut(&cli_package) {
            let needs_types = ["stdout", "stderr", "stdin"].iter().any(|name| cli.contains_key(*name));
            if needs_types {
                cli.entry("types".to_string()).or_default();
            }
        }
    }

    dependency_map
        .into_iter()
        .map(|(package_name, interfaces)| {
            let file_stem = package_name.replace('@', "-").replace(':', "-");
            let file_name = format!("{file_stem}.wit");
            let mut wit = String::new();
            wit.push_str(&format!("package {};\n\n", package_name));
            let is_cli_p3 = package_name == format!("wasi:cli@{}", WASI_PREVIEW3_VERSION);
            for (interface_name, fields) in interfaces {
                wit.push_str(&format!("interface {} {{\n", interface_name));
                // wall/system-clock WIT datetime record (flat core lowering is (i64,i32)).
                if matches!(interface_name.as_str(), "wall-clock" | "system-clock") {
                    wit.push_str("  record datetime {\n");
                    wit.push_str("    seconds: u64,\n");
                    wit.push_str("    nanoseconds: u32,\n");
                    wit.push_str("  }\n");
                }
                // Official wasi:cli@0.3 types (wasmtime HostWithStore error-code).
                if is_cli_p3 && interface_name == "types" {
                    wit.push_str("  enum error-code {\n");
                    wit.push_str("    io,\n");
                    wit.push_str("    illegal-byte-sequence,\n");
                    wit.push_str("    pipe,\n");
                    wit.push_str("  }\n");
                    wit.push_str("}\n\n");
                    continue;
                }
                if is_cli_p3 && matches!(interface_name.as_str(), "stdout" | "stderr" | "stdin") {
                    wit.push_str("  use types.{error-code};\n");
                }
                if interface_name == "run" && fields.is_empty() {
                    // p3 command world: async run; p2 remains sync result.
                    if is_cli_p3 {
                        wit.push_str("  run: async func() -> result;\n");
                    }
                    else {
                        wit.push_str("  run: func() -> result;\n");
                    }
                }
                else {
                    for field in fields {
                        wit.push_str(&wit_func_stub_line(&package_name, interface_name.as_str(), field.as_str()));
                    }
                }
                wit.push_str("}\n\n");
            }
            (file_name, wit)
        })
        .collect()
}

fn wit_func_stub_line(package_name: &str, interface_name: &str, field: &str) -> String {
    // Honest WASI Preview2 / Preview3 signatures for interfaces we know how to lower;
    // remaining stubs stay `(s32)->s32` until Canonical ABI is complete.
    if package_name.starts_with("wasi:cli@") && interface_name == "environment" && field == "get-arguments" {
        return format!("  {field}: func() -> list<string>;\n");
    }
    let is_cli_p3 = package_name == format!("wasi:cli@{}", WASI_PREVIEW3_VERSION);
    // Preview2 stdout/stderr are already lowered from the component-model
    // stream/future signature. Keep that shape in WIT so wasm-tools can
    // resolve the generated `[stream-*]write-via-stream` imports.
    if package_name == format!("wasi:cli@{}", WASI_PREVIEW2_VERSION)
        && matches!(interface_name, "stdout" | "stderr")
        && field == "write-via-stream"
    {
        return format!("  {field}: func(data: stream<u8>) -> future<result>;\n");
    }
    // Official WASI 0.3 / wasmtime p3 HostWithStore:
    //   write-via-stream: func(data: stream<u8>) -> future<result<_, error-code>>;
    // NOT `async func(...) -> result` (that mismatches the host linker).
    if is_cli_p3 && matches!(interface_name, "stdout" | "stderr") && field == "write-via-stream" {
        return format!("  {field}: func(data: stream<u8>) -> future<result<_, error-code>>;\n");
    }
    // Official WASI 0.3 stdin:
    //   read-via-stream: func() -> tuple<stream<u8>, future<result<_, error-code>>>;
    if is_cli_p3 && interface_name == "stdin" && field == "read-via-stream" {
        return format!("  {field}: func() -> tuple<stream<u8>, future<result<_, error-code>>>;\n");
    }
    // wall/system-clock: honest datetime (core flat (i64,i32)); memory-record cabi still absent.
    if package_name.starts_with("wasi:clocks@")
        && matches!(interface_name, "wall-clock" | "system-clock")
        && matches!(field, "now" | "resolution" | "get-resolution")
    {
        return format!("  {field}: func() -> datetime;\n");
    }
    // monotonic-clock: instant / duration are u64.
    if package_name.starts_with("wasi:clocks@") && matches!(field, "now" | "resolution" | "get-resolution") {
        return format!("  {field}: func() -> u64;\n");
    }
    format!("  {field}: func(arg0: s32) -> s32;\n")
}

fn split_wit_import_module(module: &str) -> Option<(String, String)> {
    let (module_path, version) = match module.rsplit_once('@') {
        Some((path, ver)) if path.contains('/') => (path, Some(ver)),
        _ => (module, None),
    };
    let (package_name, interface_name) = module_path.rsplit_once('/')?;
    let package = match version {
        Some(ver) => format!("{package_name}@{ver}"),
        None => package_name.to_string(),
    };
    Some((package, sanitize_wit_identifier(interface_name)))
}

fn sanitize_wit_identifier(raw: &str) -> String {
    let mut sanitized = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' {
            sanitized.push(ch.to_ascii_lowercase());
        }
        else {
            sanitized.push('-');
        }
    }

    while sanitized.contains("--") {
        sanitized = sanitized.replace("--", "-");
    }
    let sanitized = sanitized.trim_matches('-').to_string();

    if sanitized.is_empty() {
        return "generated".to_string();
    }

    if sanitized.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        return format!("generated-{}", sanitized);
    }

    sanitized
}

fn sanitize_wit_package_segment(raw: &str) -> String {
    let mut sanitized = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' {
            sanitized.push(ch.to_ascii_lowercase());
        }
        else {
            sanitized.push('-');
        }
    }

    // WIT package ids reject empty kebab segments (`--`); collapse like identifiers.
    while sanitized.contains("--") {
        sanitized = sanitized.replace("--", "-");
    }
    let trimmed = sanitized.trim_matches('-');
    if trimmed.is_empty() { "generated".to_string() } else { trimmed.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_double_dashes_in_package_segment() {
        // Artifact stems like `legion__main_legion` must not yield WIT-illegal `--`.
        let wit = build_component_wit_document("legion__main_legion", &[]);
        assert!(wit.contains("package nyar:legion-main-legion@0.1.0;"), "wit={wit}");
        assert!(!wit.contains("--"), "wit={wit}");
    }

    #[test]
    fn emits_local_interface_for_bare_env_module() {
        let wit = build_component_wit_document("demo_wasi", &[("env".to_string(), "cli_get_project".to_string())]);
        assert!(wit.contains("interface env {"), "wit={wit}");
        assert!(wit.contains("cli-get-project: func(arg0: s32) -> s32;"), "wit={wit}");
        assert!(wit.contains("import env;"), "wit={wit}");
    }

    #[test]
    fn emits_command_world_without_imports() {
        let wit = build_component_wit_document("demo_wasi", &[]);
        assert!(wit.contains("package nyar:demo-wasi@0.1.0;"), "wit={wit}");
        assert!(wit.contains("world command"), "wit={wit}");
        assert!(wit.contains("export wasi:cli/run@0.2.12;"), "wit={wit}");
        assert!(!wit.contains("import "), "wit={wit}");
    }

    #[test]
    fn emits_wasip3_command_world_with_0_3_package_train() {
        let wit = build_component_wit_document_for("demo_wasi", &[], WasiPreview::Preview3);
        assert!(wit.contains("export wasi:cli/run@0.3.0;"), "wit={wit}");
        assert!(!wit.contains("@0.2.12"), "wit={wit}");
    }

    #[test]
    fn wasip3_remaps_console_to_cli_stdout_and_renames_resolution() {
        // p3-first: p2 wasi:io console → wasi:cli/stdout; clocks rename resolution.
        assert_eq!(
            wasi_adapt_import_for_preview("wasi:io/streams", "blocking-write-and-flush", WasiPreview::Preview3),
            Some(("wasi:cli/stdout".to_string(), "write-via-stream".to_string()))
        );
        assert_eq!(
            wasi_adapt_import_for_preview("wasi:clocks/monotonic-clock", "resolution", WasiPreview::Preview3),
            Some(("wasi:clocks/monotonic-clock".to_string(), "get-resolution".to_string()))
        );
        let adapted: Vec<_> = [
            ("wasi:clocks/monotonic-clock", "now"),
            ("wasi:clocks/monotonic-clock", "resolution"),
            ("wasi:io/streams", "blocking-write-and-flush"),
        ]
        .into_iter()
        .filter_map(|(module, field)| {
            let (module, field) = wasi_adapt_import_for_preview(module, field, WasiPreview::Preview3)?;
            Some((wasi_versioned_import_module_for(&module, WasiPreview::Preview3), field))
        })
        .collect();
        let wit = build_component_wit_document_for("demo_wasi", &adapted, WasiPreview::Preview3);
        assert!(wit.contains("import wasi:clocks/monotonic-clock@0.3.0;"), "wit={wit}");
        assert!(wit.contains("import wasi:cli/stdout@0.3.0;"), "wit={wit}");
        assert!(!wit.contains("wasi:io"), "wit={wit}");
        let deps = build_dependency_documents_for(&adapted, WasiPreview::Preview3);
        let clocks = deps.iter().find(|(name, _)| name.contains("clocks")).expect("clocks dep");
        assert!(clocks.1.contains("get-resolution: func() -> u64;"), "wit={}", clocks.1);
        // Avoid false positive: `get-resolution: func` contains the substring `resolution: func`.
        assert!(!clocks.1.lines().any(|line| line.trim_start().starts_with("resolution:")), "wit={}", clocks.1);
        let cli = deps.iter().find(|(name, _)| name.contains("cli")).expect("cli dep");
        assert!(cli.1.contains("write-via-stream: func(data: stream<u8>) -> future<result<_, error-code>>;"), "wit={}", cli.1);
        assert!(cli.1.contains("interface types"), "wit={}", cli.1);
        assert!(cli.1.contains("enum error-code"), "wit={}", cli.1);
        assert!(cli.1.contains("run: async func() -> result;"), "wit={}", cli.1);
        assert!(!cli.1.contains("async func(data: stream"), "wit={}", cli.1);
        assert!(!cli.1.contains("list<u8>"), "wit={}", cli.1);
    }

    #[test]
    fn wasip2_cli_stream_methods_use_component_model_signature() {
        let imports = vec![("wasi:cli/stderr@0.2.12".to_string(), "write-via-stream".to_string())];
        let deps = build_dependency_documents_for(&imports, WasiPreview::Preview2);
        let cli = deps.iter().find(|(name, _)| name.contains("cli")).expect("cli dep");
        assert!(cli.1.contains("write-via-stream: func(data: stream<u8>) -> future<result>;"));
        assert!(!cli.1.contains("write-via-stream: func(arg0: s32) -> s32;"));
    }

    #[test]
    fn derives_preview_from_host_flavor() {
        assert_eq!(WasiPreview::from_host_flavor("wasi-component-model"), WasiPreview::Preview2);
        assert_eq!(WasiPreview::from_host_flavor("wasi-component-model-p3"), WasiPreview::Preview3);
    }

    #[test]
    fn emits_world_imports_for_wasi_packages() {
        let wit = build_component_wit_document(
            "demo_wasi",
            &[
                ("wasi:clocks/monotonic-clock".to_string(), "now".to_string()),
                ("wasi:io/streams".to_string(), "blocking-write-and-flush".to_string()),
            ],
        );
        assert!(wit.contains("import wasi:clocks/monotonic-clock@0.2.12;"), "wit={wit}");
        assert!(wit.contains("import wasi:io/streams@0.2.12;"), "wit={wit}");
    }

    #[test]
    fn emits_honest_get_arguments_signature() {
        let dependencies = build_dependency_documents(&[("wasi:cli/environment".to_string(), "get-arguments".to_string())]);
        let cli = dependencies.iter().find(|(file_name, _)| file_name.contains("wasi-cli")).expect("cli dependency");
        assert!(cli.1.contains("package wasi:cli@0.2.12;"), "wit={}", cli.1);
        assert!(cli.1.contains("get-arguments: func() -> list<string>;"), "wit={}", cli.1);
        assert!(cli.1.contains("interface run"), "wit={}", cli.1);
        assert!(cli.1.contains("run: func() -> result;"), "wit={}", cli.1);
    }

    #[test]
    fn builds_dependency_documents_for_import_packages() {
        let dependencies = build_dependency_documents(&[
            ("wasi:clocks/monotonic-clock".to_string(), "now".to_string()),
            ("wasi:io/streams".to_string(), "blocking-write-and-flush".to_string()),
        ]);
        let clocks = dependencies.iter().find(|(file_name, _)| file_name.contains("wasi-clocks")).expect("clocks dependency");
        let io = dependencies.iter().find(|(file_name, _)| file_name.contains("wasi-io")).expect("io dependency");
        assert!(clocks.1.contains("package wasi:clocks@0.2.12;"), "wit={}", clocks.1);
        assert!(clocks.1.contains("interface monotonic-clock"), "wit={}", clocks.1);
        assert!(clocks.1.contains("now: func() -> u64;"), "wit={}", clocks.1);
        assert!(io.1.contains("package wasi:io@0.2.12;"), "wit={}", io.1);
        assert!(io.1.contains("interface streams"), "wit={}", io.1);
        assert!(io.1.contains("blocking-write-and-flush: func(arg0: s32) -> s32;"), "wit={}", io.1);
    }

    #[test]
    fn emits_honest_wall_clock_datetime_signature() {
        let dependencies = build_dependency_documents(&[("wasi:clocks/wall-clock".to_string(), "now".to_string())]);
        let clocks = dependencies.iter().find(|(file_name, _)| file_name.contains("wasi-clocks")).expect("clocks dependency");
        assert!(clocks.1.contains("package wasi:clocks@0.2.12;"), "wit={}", clocks.1);
        assert!(clocks.1.contains("interface wall-clock"), "wit={}", clocks.1);
        assert!(clocks.1.contains("record datetime"), "wit={}", clocks.1);
        assert!(clocks.1.contains("seconds: u64"), "wit={}", clocks.1);
        assert!(clocks.1.contains("nanoseconds: u32"), "wit={}", clocks.1);
        assert!(clocks.1.contains("now: func() -> datetime;"), "wit={}", clocks.1);
        assert!(!clocks.1.contains("now: func() -> u64;"), "wit={}", clocks.1);
    }
}
