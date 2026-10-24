//! 微信小程序工程补齐。

use std::{
    fs,
    path::{Path, PathBuf},
};

use miette::{IntoDiagnostic, Result};

use super::util::copy_dir_recursive;
use crate::{codegen::find_asgard_ui_section, delivery::PackReport};

/// 补齐 VOA 小程序 dist 为可导入开发者工具的工程。
pub fn pack_mini_program(input: &Path, output: Option<&Path>, project_name: &str) -> Result<PackReport> {
    let dist = resolve_miniprogram_dist(input)?;
    let out = output.map(PathBuf::from).unwrap_or_else(|| dist.clone());
    if dist != out {
        copy_dir_recursive(&dist, &out)?;
    }
    if !out.join("app.json").exists() {
        return Err(miette::miette!("缺少 app.json；请先 `asgard build`（platform: wechat-miniprogram）"));
    }
    validate_miniprogram_dist(&out)?;
    fs::write(
        out.join("project.config.json"),
        format!(
            r#"{{
  "description": "Asgard WeChat mini-program",
  "packOptions": {{ "ignore": [] }},
  "setting": {{
    "es6": true,
    "minified": false
  }},
  "compileType": "miniprogram",
  "appid": "touristappid",
  "projectname": "{project_name}"
}}
"#
        ),
    )
    .into_diagnostic()?;
    fs::write(
        out.join("sitemap.json"),
        r#"{
  "rules": [{ "action": "allow", "page": "*" }]
}
"#,
    )
    .into_diagnostic()?;
    if !out.join("app.wxss").exists() {
        fs::write(out.join("app.wxss"), "/* asgard miniprogram */\n").into_diagnostic()?;
    }
    Ok(PackReport { artifact_path: out.clone(), message: format!("mini-program project {}", out.display()) })
}

fn resolve_miniprogram_dist(input: &Path) -> Result<PathBuf> {
    for candidate in [input.to_path_buf(), input.join("dist")] {
        if candidate.join("app.json").exists() {
            return Ok(candidate);
        }
    }
    Err(miette::miette!("未找到小程序 dist（需 app.json）。请先 `asgard build`（platform: wechat-miniprogram）"))
}

fn validate_miniprogram_dist(dist: &Path) -> Result<()> {
    let app_json = fs::read_to_string(dist.join("app.json")).into_diagnostic()?;
    if !app_json.contains("\"pages\"") {
        return Err(miette::miette!("app.json 缺少 pages 字段"));
    }
    let pages_dir = dist.join("pages");
    if pages_dir.exists() {
        for entry in fs::read_dir(&pages_dir).into_diagnostic()? {
            let entry = entry.into_diagnostic()?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("wxml") {
                continue;
            }
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !path.join(format!("{name}.wxml")).exists() {
                    return Err(miette::miette!("页面目录 {} 缺少对应 .wxml", path.display()));
                }
            }
        }
    }
    let host_dir = dist.join("host");
    if host_dir.exists() {
        return Err(miette::miette!("Release dist 不应含 host/ 目录；逻辑与 UI 应在 *.wasm（含 asgard ui 尾段）"));
    }
    let wasm_path = find_wasm_in_dist(dist)?;
    let wasm_bytes = fs::read(&wasm_path).into_diagnostic()?;
    if find_asgard_ui_section(&wasm_bytes).is_none() {
        return Err(miette::miette!("{} 缺少 asgard ui 段", wasm_path.display()));
    }
    let runtime = fs::read_to_string(dist.join("asgard-runtime.js")).into_diagnostic()?;
    if runtime.contains("__ASGARD_PRODUCT__") {
        return Err(miette::miette!("asgard-runtime.js 不应内嵌 __ASGARD_PRODUCT__；产品应在 .wasm"));
    }
    if !runtime.contains("WXWebAssembly") {
        return Err(miette::miette!("asgard-runtime.js 缺少 WXWebAssembly 加载逻辑"));
    }
    Ok(())
}

fn find_wasm_in_dist(dist: &Path) -> Result<PathBuf> {
    for entry in fs::read_dir(dist).into_diagnostic()? {
        let entry = entry.into_diagnostic()?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("wasm") {
            return Ok(path);
        }
    }
    Err(miette::miette!("缺少 *.wasm；请先 `asgard build`（platform: wechat-miniprogram）"))
}
