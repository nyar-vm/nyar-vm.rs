//! Unity `build_plugin` 导出：将 MSIL 产物复制到 `build/unity/msil` 并写入 `valkyrie-export.json`。

use legion_workspace::{
    manifest::{BuildPluginSpec, ProjectManifest},
    planner::BuildPlan,
};
use std::{fs, path::Path};

use miette::{IntoDiagnostic, Result, WrapErr, miette};
use nyar_emitter::DriverCompileReport;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct ValkyrieExportManifest {
    #[serde(default, skip_serializing_if = "is_version_one")]
    version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    entry: Option<ValkyrieExportEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    artifacts: Vec<ValkyrieExportArtifact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    entry_assembly: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    entry_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    entry_method: Option<String>,
    target: String,
    publish: Vec<String>,
    sdk_version: String,
}

#[derive(Debug, Serialize)]
struct ValkyrieExportEntry {
    assembly: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    r#type: String,
    method: String,
}

/// `valkyrie-export.json` 中的单条 MSIL 分区产物记录。
#[derive(Debug, Serialize, Clone, Deserialize)]
pub struct ValkyrieExportArtifact {
    /// DLL 文件名。
    pub assembly: String,
    /// 分区名（多分区导出时使用）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partition: Option<String>,
    /// 产物角色：`plugin-runtime` / `plugin-editor` / `partition`。
    pub role: String,
}

fn is_version_one(version: &u32) -> bool {
    *version <= 1
}

/// 在 `legion build` 成功后执行 Unity 工程导出。
pub fn export_unity_project(plan: &BuildPlan, report: &DriverCompileReport, plugin: &BuildPluginSpec) -> Result<()> {
    if plugin.kind != "unity-project-export" {
        return Ok(());
    }

    let input_dir = plugin
        .input_directory
        .as_deref()
        .map(|path| plan.project.manifest_dir.join(path))
        .unwrap_or_else(|| plan.project.manifest_dir.join("build").join("unity").join("msil"));

    let deps_dir = input_dir.join("deps");
    fs::create_dir_all(&input_dir).into_diagnostic().wrap_err_with(|| format!("创建 Unity MSIL 目录失败：{}", input_dir.display()))?;
    fs::create_dir_all(&deps_dir).into_diagnostic().wrap_err_with(|| format!("创建 Unity 依赖目录失败：{}", deps_dir.display()))?;

    let entry_assembly = format!("{}.dll", plan.project.name);
    let main_dll = plan.output_dir.join(&entry_assembly);
    if !main_dll.is_file() {
        return Err(miette!("Unity 导出未找到主程序集 {}，请先确认 `legion build` 已成功生成 MSIL DLL", main_dll.display()));
    }

    copy_dll(&main_dll, &input_dir.join(&entry_assembly))?;

    let mut dependency_names = Vec::new();
    let mut partition_artifacts = Vec::new();
    for entry in fs::read_dir(&plan.output_dir).into_diagnostic()? {
        let entry = entry.into_diagnostic()?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("dll") {
            continue;
        }
        let file_name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path.file_name() == main_dll.file_name() {
            continue;
        }
        if let Some(artifact) = artifact_from_dll_name(&plan.project.name, &file_name) {
            copy_dll(&path, &input_dir.join(&file_name))?;
            partition_artifacts.push(artifact);
            continue;
        }
        copy_dll(&path, &deps_dir.join(path.file_name().unwrap()))?;
        dependency_names.push(file_name);
    }
    dependency_names.sort();
    partition_artifacts.sort_by(|left, right| left.assembly.cmp(&right.assembly));

    let manifest_source = fs::read_to_string(&plan.project.manifest_path)
        .into_diagnostic()
        .wrap_err_with(|| format!("读取项目清单失败：{}", plan.project.manifest_path.display()))?;
    let project_manifest = ProjectManifest::parse(&manifest_source)?;

    let entry_method = report.entry_symbol.clone().unwrap_or_else(|| "main".to_string());
    let export = ValkyrieExportManifest {
        version: if partition_artifacts.is_empty() { 1 } else { 2 },
        entry: Some(ValkyrieExportEntry { assembly: entry_assembly.clone(), r#type: String::new(), method: entry_method.clone() }),
        artifacts: partition_artifacts,
        entry_assembly: Some(entry_assembly),
        entry_type: Some(String::new()),
        entry_method: Some(entry_method),
        target: plan.project.build_target.target.to_string(),
        publish: plan.project.build_target.publish.clone(),
        sdk_version: project_manifest.version.unwrap_or_else(|| "0.0.0".to_string()),
    };

    let export_path = input_dir.join("valkyrie-export.json");
    let export_json = serde_json::to_string_pretty(&export).into_diagnostic().wrap_err("序列化 valkyrie-export.json 失败")?;
    fs::write(&export_path, export_json).into_diagnostic().wrap_err_with(|| format!("写入导出清单失败：{}", export_path.display()))?;

    if let Some(output_directory) = &plugin.output_directory {
        let project_export_dir = plan.project.manifest_dir.join(output_directory);
        fs::create_dir_all(&project_export_dir)
            .into_diagnostic()
            .wrap_err_with(|| format!("创建 Unity 工程导出目录失败：{}", project_export_dir.display()))?;
        let pointer_path = project_export_dir.join("valkyrie-msil-pointer.json");
        let pointer = serde_json::json!({
            "msil_directory": relative_path(&plan.project.manifest_dir, &input_dir),
            "export_manifest": relative_path(&plan.project.manifest_dir, &export_path),
            "dependency_assemblies": dependency_names,
        });
        fs::write(&pointer_path, serde_json::to_string_pretty(&pointer).into_diagnostic()?)
            .into_diagnostic()
            .wrap_err_with(|| format!("写入 Unity 工程指针文件失败：{}", pointer_path.display()))?;
    }

    Ok(())
}

fn copy_dll(source: &Path, destination: &Path) -> Result<()> {
    fs::copy(source, destination)
        .into_diagnostic()
        .wrap_err_with(|| format!("复制 DLL 失败：{} -> {}", source.display(), destination.display()))?;
    Ok(())
}

fn relative_path(base: &Path, target: &Path) -> String {
    if let (Ok(base), Ok(target)) = (base.canonicalize(), target.canonicalize()) {
        if target.starts_with(&base) {
            return target.strip_prefix(&base).unwrap_or(&target).to_string_lossy().trim_start_matches(['/', '\\']).to_string();
        }
    }
    target.to_string_lossy().into_owned()
}

/// 将 `build/unity/msil` 中的 DLL 同步到 Unity 工程插件目录。
pub fn sync_msil_to_unity_plugins(msil_dir: &Path, plugins_dir: &Path) -> Result<()> {
    if !msil_dir.is_dir() {
        return Err(miette!("MSIL 目录不存在：{}", msil_dir.display()));
    }

    fs::create_dir_all(plugins_dir).into_diagnostic().wrap_err_with(|| format!("创建 Unity Plugins 目录失败：{}", plugins_dir.display()))?;

    let export_path = msil_dir.join("valkyrie-export.json");
    if export_path.is_file() {
        let manifest = read_export_manifest(&export_path)?;
        if let Some(entry) = &manifest.entry {
            copy_dll(&msil_dir.join(&entry.assembly), &plugins_dir.join(&entry.assembly))?;
        }
        else if let Some(entry_assembly) = manifest.entry_assembly.as_ref() {
            copy_dll(&msil_dir.join(entry_assembly), &plugins_dir.join(entry_assembly))?;
        }
    }

    for entry in fs::read_dir(msil_dir).into_diagnostic()? {
        let entry = entry.into_diagnostic()?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("dll") {
            let file_name = path.file_name().unwrap();
            copy_dll(&path, &plugins_dir.join(file_name))?;
        }
    }

    let deps_dir = msil_dir.join("deps");
    if deps_dir.is_dir() {
        for entry in fs::read_dir(&deps_dir).into_diagnostic()? {
            let entry = entry.into_diagnostic()?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("dll") {
                let file_name = path.file_name().unwrap();
                copy_dll(&path, &plugins_dir.join(file_name))?;
            }
        }
    }

    Ok(())
}

/// 按 `export_routes` 将 MSIL 分区产物同步到 Unity 工程。
pub fn sync_msil_with_routes(msil_dir: &Path, unity_project_root: &Path, export_routes: &BTreeMap<String, String>) -> Result<()> {
    if !msil_dir.is_dir() {
        return Err(miette!("MSIL 目录不存在：{}", msil_dir.display()));
    }

    let export_path = msil_dir.join("valkyrie-export.json");
    let export_manifest = if export_path.is_file() { Some(read_export_manifest(&export_path)?) } else { None };

    if let Some(manifest) = &export_manifest {
        if !manifest.artifacts.is_empty() {
            for artifact in &manifest.artifacts {
                let route = export_routes
                    .get(artifact.partition.as_deref().unwrap_or("default"))
                    .or_else(|| export_routes.get("default"))
                    .map(String::as_str)
                    .unwrap_or("Assets/Valkyrie/Plugins");
                let destination_dir = unity_project_root.join(route);
                fs::create_dir_all(&destination_dir)
                    .into_diagnostic()
                    .wrap_err_with(|| format!("创建 Unity 导出目录失败：{}", destination_dir.display()))?;
                let destination_name = artifact_destination_name(artifact);
                copy_dll(&msil_dir.join(&artifact.assembly), &destination_dir.join(&destination_name))?;
            }
            if let Some(entry) = &manifest.entry {
                let route = export_routes.get("default").map(String::as_str).unwrap_or("Assets/Valkyrie/Plugins");
                let destination_dir = unity_project_root.join(route);
                fs::create_dir_all(&destination_dir)
                    .into_diagnostic()
                    .wrap_err_with(|| format!("创建 Unity 导出目录失败：{}", destination_dir.display()))?;
                copy_dll(&msil_dir.join(&entry.assembly), &destination_dir.join(&entry.assembly))?;
            }
            return Ok(());
        }
    }

    let plugins_dir = unity_project_root.join("Assets").join("Valkyrie").join("Plugins");
    sync_msil_to_unity_plugins(msil_dir, &plugins_dir)
}

/// 反序列化后的导出清单（供 status 与同步流程读取）。
#[derive(Debug, Deserialize)]
pub struct ParsedExportManifest {
    /// 清单 schema 版本。
    #[serde(default)]
    pub version: u32,
    /// 入口程序集与入口方法（v2 清单）。
    pub entry: Option<ParsedExportEntry>,
    /// 分区产物列表。
    #[serde(default)]
    pub artifacts: Vec<ValkyrieExportArtifact>,
    /// 入口程序集文件名（v1 清单）。
    pub entry_assembly: Option<String>,
    /// 入口方法名（v1 清单）。
    pub entry_method: Option<String>,
}

/// 导出清单中的入口记录。
#[derive(Debug, Deserialize)]
pub struct ParsedExportEntry {
    /// 入口程序集文件名。
    pub assembly: String,
}

fn read_export_manifest(path: &Path) -> Result<ParsedExportManifest> {
    let text = fs::read_to_string(path).into_diagnostic()?;
    serde_json::from_str(&text).into_diagnostic().wrap_err("解析 valkyrie-export.json 失败")
}

/// 读取导出清单供 `legion-unknown-wasm32 status` 展示。
pub fn read_export_manifest_for_status(path: &Path) -> Result<ParsedExportManifest> {
    read_export_manifest(path)
}

fn artifact_destination_name(artifact: &ValkyrieExportArtifact) -> String {
    match artifact.role.as_str() {
        "plugin-runtime" => "Valkyrie.Unity.Runtime.dll".to_string(),
        "plugin-editor" => "Valkyrie.Unity.Editor.dll".to_string(),
        _ => artifact.assembly.clone(),
    }
}

fn artifact_from_dll_name(project_name: &str, dll_name: &str) -> Option<ValkyrieExportArtifact> {
    let stem = dll_name.strip_suffix(".dll")?;
    let marker = format!("{project_name}__");
    let suffix = stem.strip_prefix(&marker)?;
    let partition = suffix.replace('_', ".");
    let role = match partition.as_str() {
        "unity.runtime" => "plugin-runtime",
        "unity.editor" => "plugin-editor",
        _ => "partition",
    };
    Some(ValkyrieExportArtifact { assembly: dll_name.to_string(), partition: Some(partition), role: role.to_string() })
}

fn read_json_string_field(json: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\"");
    let index = json.find(&marker)?;
    let colon = json[index..].find(':')? + index;
    let start = json[colon..].find('"')? + colon + 1;
    let end = json[start..].find('"')? + start;
    Some(json[start..end].to_string())
}
#[cfg(test)]
#[path = "../tests/path_wired/unity_export.rs"]
mod unity_export_tests;
