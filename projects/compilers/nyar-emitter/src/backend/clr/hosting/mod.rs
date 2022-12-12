//! `CLR` 宿主相关产物。
//!
//! 负责生成 `dotnet` 宿主运行所需的辅助文件，
//! 避免命令层手写 `JSON` 文本。

use std::path::Path;

use miette::{IntoDiagnostic, Result, WrapErr};
use serde::Serialize;
use serde_json::{Map, Value};

/// `dotnet` 运行时配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DotNetRuntimeConfig {
    /// 运行时选项。
    #[serde(rename = "runtimeOptions")]
    pub runtime_options: DotNetRuntimeOptions,
}

/// `dotnet` 运行时选项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DotNetRuntimeOptions {
    /// 目标框架标识。
    pub tfm: String,
    /// 目标宿主框架。
    pub framework: DotNetFramework,
}

/// `dotnet` 宿主框架信息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DotNetFramework {
    /// 框架名称。
    pub name: String,
    /// 框架版本。
    pub version: String,
}

impl Default for DotNetRuntimeConfig {
    fn default() -> Self {
        Self {
            runtime_options: DotNetRuntimeOptions {
                // net8.0 matches common installed runtimes.
                tfm: "net8.0".to_string(),
                framework: DotNetFramework { name: "Microsoft.NETCore.App".to_string(), version: "8.0.0".to_string() },
            },
        }
    }
}

/// 将 `dotnet` 运行时配置写入输出目录。
pub fn write_dotnet_runtime_config(output_dir: &Path, artifact_name: &str) -> Result<()> {
    let runtime_config_path = output_dir.join(format!("{}.runtimeconfig.json", artifact_name));
    let runtime_config = serde_json::to_string_pretty(&DotNetRuntimeConfig::default())
        .into_diagnostic()
        .wrap_err_with(|| format!("序列化运行时配置失败：{}", runtime_config_path.display()))?;

    std::fs::write(&runtime_config_path, format!("{}\n", runtime_config))
        .into_diagnostic()
        .wrap_err_with(|| format!("写入运行时配置失败：{}", runtime_config_path.display()))
}

/// Framework assemblies resolved by the host without a local app-directory DLL.
fn is_framework_assembly(name: &str) -> bool {
    matches!(
        name,
        "mscorlib"
            | "netstandard"
            | "System.Runtime"
            | "System.Private.CoreLib"
            | "System.Runtime.Extensions"
            | "System.Runtime.InteropServices"
            | "System.Console"
    )
}

/// Write a minimal `.deps.json` so `dotnet` can resolve local AssemblyRefs
/// next to the generated executable.
///
/// Seed PE AssemblyRefs use major version `4.0.0.0`; local project DLLs should match.
pub fn write_dotnet_deps_json(output_dir: &Path, artifact_name: &str, assembly_externs: &[String]) -> Result<()> {
    let local_deps: Vec<&str> = assembly_externs.iter().map(String::as_str).filter(|name| !is_framework_assembly(name)).collect();
    if local_deps.is_empty() {
        return Ok(());
    }

    let runtime_target = ".NETCoreApp,Version=v8.0";
    let app_id = format!("{artifact_name}/1.0.0");
    let mut targets_runtime = Map::new();
    let mut libraries = Map::new();
    let mut app_dependencies = Map::new();

    let mut app_target = Map::new();
    let mut app_runtime = Map::new();
    app_runtime.insert(format!("{artifact_name}.exe"), Value::Object(Map::new()));
    app_target.insert("runtime".to_string(), Value::Object(app_runtime));

    for dep in &local_deps {
        let dep_id = format!("{dep}/4.0.0");
        app_dependencies.insert((*dep).to_string(), Value::String("4.0.0".into()));

        let mut dep_runtime = Map::new();
        let mut asset = Map::new();
        asset.insert("assemblyVersion".into(), Value::String("4.0.0.0".into()));
        asset.insert("fileVersion".into(), Value::String("4.0.0.0".into()));
        dep_runtime.insert(format!("{dep}.dll"), Value::Object(asset));
        let mut dep_target = Map::new();
        dep_target.insert("runtime".into(), Value::Object(dep_runtime));
        targets_runtime.insert(dep_id.clone(), Value::Object(dep_target));

        let mut lib = Map::new();
        lib.insert("type".into(), Value::String("project".into()));
        lib.insert("serviceable".into(), Value::Bool(false));
        lib.insert("sha512".into(), Value::String(String::new()));
        libraries.insert(dep_id, Value::Object(lib));
    }
    app_target.insert("dependencies".into(), Value::Object(app_dependencies));
    targets_runtime.insert(app_id.clone(), Value::Object(app_target));

    let mut app_lib = Map::new();
    app_lib.insert("type".into(), Value::String("project".into()));
    app_lib.insert("serviceable".into(), Value::Bool(false));
    app_lib.insert("sha512".into(), Value::String(String::new()));
    libraries.insert(app_id, Value::Object(app_lib));

    let mut targets = Map::new();
    targets.insert(runtime_target.into(), Value::Object(targets_runtime));

    let mut root = Map::new();
    let mut runtime_target_obj = Map::new();
    runtime_target_obj.insert("name".into(), Value::String(runtime_target.into()));
    runtime_target_obj.insert("signature".into(), Value::String(String::new()));
    root.insert("runtimeTarget".into(), Value::Object(runtime_target_obj));
    root.insert("compilationOptions".into(), Value::Object(Map::new()));
    root.insert("targets".into(), Value::Object(targets));
    root.insert("libraries".into(), Value::Object(libraries));

    let deps_path = output_dir.join(format!("{artifact_name}.deps.json"));
    let text = serde_json::to_string_pretty(&Value::Object(root))
        .into_diagnostic()
        .wrap_err_with(|| format!("序列化依赖配置失败：{}", deps_path.display()))?;
    std::fs::write(&deps_path, format!("{text}\n")).into_diagnostic().wrap_err_with(|| format!("写入依赖配置失败：{}", deps_path.display()))
}
