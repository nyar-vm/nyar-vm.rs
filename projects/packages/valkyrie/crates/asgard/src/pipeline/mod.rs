//! VOA 编译主编排。

mod discover;
mod island_kind;
mod partition;

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    awsl::{LoweredComponent, LoweringOptions, ThemeRegistryOptions, compile_awsl_source, theme_registry_pass},
    codegen::{
        IslandPackageOptions, build_awsl_host_source, build_awsl_mp_source, build_awsl_terminal_source, build_awsl_wasm_source,
        embed_asgard_ui_section, encode_mobile_ui_package, generate_mp_runtime, maybe_build_tailwind_css, package_browser_islands,
    },
    compile::{GeneratedSource, HostArtifactKind, compile_v_bundle, resolve_project_source_groups, terminal_runtime_source},
    config::{UiConfig, VoaConfig},
    debug_sidecar::{should_emit_render_ir_sidecar, write_render_ir_sidecar},
    host::HostPlatform,
    host_backend::HostBackend,
    host_validation::validate_host_logic_bytes,
    package::{
        generate_app_json, generate_index_html, package_android_project, package_desktop_project, package_ios_project,
        package_miniprogram_pages, package_terminal_project,
    },
    tailwind::collect_from_components,
    wasm::{compile_wasm_bundle, copy_wasm_artifacts_to_dist},
};
use miette::{IntoDiagnostic, Result, WrapErr};
use rusty_valkyrie::{CanonicalAbi, CanonicalArch, CanonicalSpecification, CanonicalTarget, CanonicalVendor};

pub use discover::{DiscoveredAwsFile, DiscoveredAwslFile, DiscoveredSources, discover_sources};
pub use island_kind::{IslandKind, awsl_island_kind};

/// 编译选项。
#[derive(Debug, Clone)]
pub struct CompileOptions {
    /// 项目目录。
    pub project_dir: PathBuf,
    /// 输出目录（默认读取配置）。
    pub output_dir: Option<PathBuf>,
    /// 覆盖编译目标。
    pub target: Option<CanonicalTarget>,
}

/// 编译报告。
#[derive(Debug, Clone)]
pub struct CompileReport {
    /// 输出目录。
    pub output_dir: PathBuf,
    /// 编译的 AWSL 组件数。
    pub component_count: usize,
    /// 生成的宿主侧文件数（浏览器/小程序为 JS；Android 为工程文件数）。
    pub js_file_count: usize,
    /// 是否成功生成 WASM（browser / wechat-miniprogram）。
    pub wasm_built: bool,
    /// 宿主平台。
    pub platform: HostPlatform,
    /// 非 browser 平台是否成功编译宿主逻辑字节码。
    pub host_logic_built: bool,
    /// 宿主逻辑制品类型（非 browser）。
    pub host_artifact_kind: Option<HostArtifactKind>,
}

/// 编译 VOA 项目：browser 与 wechat-miniprogram 产出 WASM；其余平台产出宿主原生制品。
pub fn compile_voa_project(options: &CompileOptions) -> Result<CompileReport> {
    let config = VoaConfig::load(&options.project_dir)?;
    let platform = HostPlatform::parse(&config.platform);
    let backend = HostBackend::from_platform(platform).map_err(|msg| miette::miette!("{msg}"))?;
    let sources = discover_sources(&options.project_dir)?;
    let output_dir = options.output_dir.clone().unwrap_or_else(|| options.project_dir.join(&config.build.output));
    fs::create_dir_all(&output_dir).into_diagnostic().wrap_err("创建 dist 失败")?;

    let lowering_options = LoweringOptions { strict_mode: config.language.awsl.strict_mode, ..LoweringOptions::default() };

    let mut components = Vec::new();
    for awsl_file in &sources.awsl_files {
        let kind = island_kind::awsl_island_kind(&awsl_file.relative_path);
        // static = SSG only；server = Atlas 请求片段（planned）— 二者都不进客户端 hydrate/WASM 管线
        if kind.is_static() || kind.is_server() {
            continue;
        }
        let source = partition::read_awsl_file(&awsl_file.path)?;
        let mut component = compile_awsl_source(&source, &awsl_file.component_name, awsl_file.path.to_str().unwrap_or(""), &lowering_options)
            .map_err(|error| miette::miette!("{}:{}: {}", awsl_file.path.display(), error.span.start, error.message))?;
        optimize_theme_registry(&mut component, &config.ui);
        components.push(component);
    }

    let abi_index = crate::awsl::component_abi_index_from_components(&components);
    for component in &mut components {
        component.abi_issues = crate::awsl::validate_component_abi(&abi_index, component);
    }

    let tailwind_collector = collect_from_components(&components);
    tailwind_collector.write_manifest(&output_dir)?;

    let aws_entries: Vec<_> = sources.aws_files.iter().map(|file| file.path.clone()).collect();
    let tailwind_css = maybe_build_tailwind_css(&options.project_dir, &output_dir, &config.tailwind, &tailwind_collector, &aws_entries)
        .map(|output| output.css);

    let module_name = config.name.clone().unwrap_or_else(|| "asgard-app".into());
    let wasm_stem = module_name.replace('.', "-");

    let wasm_built = if matches!(backend, HostBackend::BrowserDom | HostBackend::WechatMiniProgram) {
        let target = match options.target.clone() {
            Some(target) => target,
            None => parse_target(&config.target)?,
        };
        let resolved = resolve_project_source_groups(&options.project_dir, &target)?;
        // 生成源码按宿主选择：WASM 目标是显式入口组，不是应用源码的附加文件。
        let (producer, generated_source) = match backend {
            HostBackend::WechatMiniProgram => ("asgard.awsl.wechat-miniprogram", build_awsl_mp_source(&components)),
            _ => ("asgard.awsl.browser-dom", build_awsl_wasm_source(&components)),
        };
        let report =
            compile_wasm_bundle(&resolved.groups, GeneratedSource { producer, source: &generated_source }, &output_dir, &module_name, &target)
                .wrap_err_with(|| format!("WASM 编译失败 (platform={})", config.platform))?;
        copy_wasm_artifacts_to_dist(&output_dir, &report)?;
        true
    }
    else {
        false
    };

    if should_emit_render_ir_sidecar(&config) {
        write_render_ir_sidecar(&output_dir, &components)?;
    }

    let (js_file_count, host_logic_built, host_artifact_kind) = match backend {
        HostBackend::BrowserDom => {
            let count = package_browser(&config, &components, &output_dir, &module_name, &wasm_stem, wasm_built, tailwind_css)?;
            (count, false, None)
        }
        HostBackend::WechatMiniProgram => {
            let count = package_miniprogram_dist(&components, &output_dir, &module_name, &wasm_stem, wasm_built)?;
            (count, false, None)
        }
        HostBackend::AndroidCompose => {
            let (logic, kind, _generated_source) = compile_host_logic(&components, &config, options, &output_dir, &module_name, backend)?;
            let count = package_android_dist(&components, &output_dir, &module_name, &logic)?;
            (count, true, Some(kind))
        }
        HostBackend::IosSwiftUi => {
            let (logic, kind, _generated_source) = compile_host_logic(&components, &config, options, &output_dir, &module_name, backend)?;
            let count = package_ios_dist(&components, &output_dir, &module_name, &logic)?;
            (count, true, Some(kind))
        }
        HostBackend::WindowsNative => {
            let (logic, kind, _generated_source) = compile_host_logic(&components, &config, options, &output_dir, &module_name, backend)?;
            let count = package_desktop_dist(&components, &output_dir, &module_name, "windows", &logic)?;
            (count, true, Some(kind))
        }
        HostBackend::LinuxNative => {
            let (logic, kind, _generated_source) = compile_host_logic(&components, &config, options, &output_dir, &module_name, backend)?;
            let count = package_desktop_dist(&components, &output_dir, &module_name, "linux", &logic)?;
            (count, true, Some(kind))
        }
        HostBackend::MacOsNative => {
            let (logic, kind, _generated_source) = compile_host_logic(&components, &config, options, &output_dir, &module_name, backend)?;
            let count = package_desktop_dist(&components, &output_dir, &module_name, "macos", &logic)?;
            (count, true, Some(kind))
        }
        HostBackend::Terminal => {
            let (logic, kind, generated_source) = compile_host_logic(&components, &config, options, &output_dir, &module_name, backend)?;
            let count = package_terminal_dist(generated_source, &output_dir, &module_name, &logic)?;
            (count, true, Some(kind))
        }
    };

    Ok(CompileReport {
        output_dir,
        component_count: components.len(),
        js_file_count,
        wasm_built,
        platform,
        host_logic_built,
        host_artifact_kind,
    })
}

fn optimize_theme_registry(component: &mut LoweredComponent, ui: &UiConfig) {
    theme_registry_pass(
        &mut component.render_ir,
        ThemeRegistryOptions { single_theme: single_registry_id(&ui.themes), single_mode: single_registry_id(&ui.modes) },
    );
}

fn single_registry_id(entries: &[crate::config::UiRegistryEntry]) -> Option<&str> {
    match entries {
        [entry] => Some(entry.id.as_str()),
        _ => None,
    }
}

fn package_browser(
    config: &VoaConfig,
    components: &[LoweredComponent],
    output_dir: &Path,
    module_name: &str,
    wasm_stem: &str,
    wasm_built: bool,
    tailwind_css: Option<String>,
) -> Result<usize> {
    let opts = IslandPackageOptions {
        module_stem: module_name.to_string(),
        wasm_stem: wasm_stem.to_string(),
        wasm_built,
        relative_urls: false,
        css_mode: config.build.chunk.css_mode.clone(),
        mode: config.build.mode.clone(),
        tailwind_css,
    };
    let report = package_browser_islands(components, output_dir, &opts)?;
    let css_hrefs: Vec<String> = report.css_filenames.iter().map(|name| format!("/{name}")).collect();
    let html = generate_index_html(config, components, &css_hrefs);
    fs::write(output_dir.join(&html.relative_path), &html.content).into_diagnostic()?;
    Ok(report.js_file_count)
}

fn package_miniprogram_dist(
    components: &[LoweredComponent],
    output_dir: &Path,
    module_name: &str,
    wasm_stem: &str,
    wasm_built: bool,
) -> Result<usize> {
    let package = package_miniprogram_pages(components, wasm_stem);
    let app_json = generate_app_json(components, module_name);

    if !wasm_built {
        return Err(miette::miette!("小程序缺少 *.wasm；逻辑必须 AOT 为 WebAssembly"));
    }
    let wasm_path = output_dir.join(format!("{wasm_stem}.wasm"));
    if !wasm_path.exists() {
        return Err(miette::miette!("缺少 {}；WASM 编译未产出制品", wasm_path.display()));
    }
    let mut wasm_bytes = fs::read(&wasm_path).into_diagnostic().wrap_err("读取小程序 wasm 失败")?;
    embed_asgard_ui_section(&mut wasm_bytes, &encode_mobile_ui_package(components));
    fs::write(&wasm_path, &wasm_bytes).into_diagnostic().wrap_err("写入含 asgard ui 的 wasm 失败")?;

    fs::write(output_dir.join("app.js"), &package.app_js).into_diagnostic()?;
    fs::write(output_dir.join("asgard-runtime.js"), generate_mp_runtime(wasm_stem)).into_diagnostic()?;
    fs::write(output_dir.join(&app_json.relative_path), &app_json.content).into_diagnostic()?;

    let mut file_count = 3usize;
    for page in &package.pages {
        for (relative, content) in &page.files {
            let path = output_dir.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).into_diagnostic()?;
            }
            fs::write(&path, content).into_diagnostic()?;
            file_count += 1;
        }
    }

    Ok(file_count)
}

fn package_android_dist(components: &[LoweredComponent], output_dir: &Path, module_name: &str, native_logic: &[u8]) -> Result<usize> {
    let android_root = output_dir.join("android");
    fs::create_dir_all(&android_root).into_diagnostic().wrap_err("创建 dist/android 失败")?;
    let package = package_android_project(components, module_name, native_logic);
    let mut file_count = 0usize;
    for (relative, content) in &package.files {
        let path = android_root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).into_diagnostic()?;
        }
        content.write_to(&path).into_diagnostic()?;
        file_count += 1;
    }
    Ok(file_count)
}

fn package_ios_dist(components: &[LoweredComponent], output_dir: &Path, module_name: &str, native_logic: &[u8]) -> Result<usize> {
    let ios_root = output_dir.join("ios");
    fs::create_dir_all(&ios_root).into_diagnostic().wrap_err("创建 dist/ios 失败")?;
    let package = package_ios_project(components, module_name, native_logic);
    let mut file_count = 0usize;
    for (relative, content) in &package.files {
        let path = ios_root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).into_diagnostic()?;
        }
        content.write_to(&path).into_diagnostic()?;
        file_count += 1;
    }
    Ok(file_count)
}

fn package_desktop_dist(
    components: &[LoweredComponent],
    output_dir: &Path,
    module_name: &str,
    platform: &str,
    native_logic: &[u8],
) -> Result<usize> {
    let desktop_root = output_dir.join(platform);
    fs::create_dir_all(&desktop_root).into_diagnostic().wrap_err_with(|| format!("创建 dist/{platform} 失败"))?;
    let package = package_desktop_project(components, module_name, platform, native_logic);
    let mut file_count = 0usize;
    for (relative, content) in &package.files {
        let path = desktop_root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).into_diagnostic()?;
        }
        content.write_to(&path).into_diagnostic()?;
        file_count += 1;
    }
    Ok(file_count)
}

fn package_terminal_dist(app_v: String, output_dir: &Path, module_name: &str, native_logic: &[u8]) -> Result<usize> {
    let terminal_root = output_dir.join("terminal");
    fs::create_dir_all(&terminal_root).into_diagnostic().wrap_err("创建 dist/terminal 失败")?;
    let package = package_terminal_project(app_v, module_name, native_logic);
    let mut file_count = 0usize;
    for (relative, content) in &package.files {
        let path = terminal_root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).into_diagnostic()?;
        }
        content.write_to(&path).into_diagnostic()?;
        file_count += 1;
    }
    Ok(file_count)
}

fn android_aarch64_target() -> CanonicalTarget {
    CanonicalTarget::new(CanonicalArch::AArch64, CanonicalVendor::Android, CanonicalSpecification::Android, Some(CanonicalAbi::Aapcs64))
}

fn parse_target(target: &str) -> Result<CanonicalTarget> {
    match target {
        "jvm-android-android-managed" | "jvm-android-android-dex" => Ok(android_aarch64_target()),
        other => CanonicalTarget::parse(other).map_err(|error| miette::miette!("无法解析 target `{other}`: {error}")),
    }
}

fn compile_host_logic(
    components: &[LoweredComponent],
    config: &VoaConfig,
    options: &CompileOptions,
    output_dir: &Path,
    module_name: &str,
    backend: HostBackend,
) -> Result<(Vec<u8>, HostArtifactKind, String)> {
    let target = match options.target.clone() {
        Some(target) => target,
        None => parse_target(&config.target)?,
    };
    let resolved = resolve_project_source_groups(&options.project_dir, &target)?;
    let target_str = format!("{target:?}");
    let backend_name = format!("{backend:?}");
    let scratch = output_dir.join(".asgard-build");
    // 宿主逻辑同样由生成源码提供 UI 入口；终端走纯 V TUI，其余走原生 patch/event FFI。
    let (producer, generated_source) = match backend {
        HostBackend::Terminal => {
            let runtime = terminal_runtime_source(&resolved.groups)?;
            ("asgard.awsl.terminal", build_awsl_terminal_source(components, runtime))
        }
        HostBackend::AndroidCompose => ("asgard.awsl.android-compose", build_awsl_host_source(components)),
        HostBackend::IosSwiftUi => ("asgard.awsl.ios-swiftui", build_awsl_host_source(components)),
        _ => ("asgard.awsl.desktop-native", build_awsl_host_source(components)),
    };
    let report =
        compile_v_bundle(&resolved.groups, GeneratedSource { producer, source: &generated_source }, &scratch, module_name, &target, backend)
            .wrap_err_with(|| {
                format!(
                    "宿主编译失败: platform={}, target={}, backend={}. \
             请检查 asgard.config.v target 与 legion.von 对齐，并确认 legion 后端可用。",
                    config.platform, target_str, backend_name
                )
            })?;
    validate_host_logic_bytes(&report.artifact_bytes, &config.platform, &backend_name, &target_str, report.kind)?;
    Ok((report.artifact_bytes, report.kind, generated_source))
}
#[cfg(test)]
#[path = "../../tests/path_wired/pipeline_mod.rs"]
mod pipeline_mod_tests;
