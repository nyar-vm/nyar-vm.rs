//! `asgard.config.v` 解析（Valkyrie script `define_config(asgard)`，非 TOML / 非 VON）。

#[path = "config_script.rs"]
mod config_script;

use std::path::Path;

use miette::{IntoDiagnostic, Result, WrapErr};
use oak_von::{
    language::value::{VonArray, VonEnum, VonObject, VonTuple, VonValue, from_ast},
    parse as parse_von,
};
use serde::{Deserialize, Serialize};

/// VOA 项目配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoaConfig {
    /// 项目类型。
    #[serde(default = "default_project_type")]
    pub project_type: String,
    /// 编译目标三元组。
    #[serde(default = "default_target")]
    pub target: String,
    /// 交付宿主平台：`browser` / `wechat-miniprogram` / `wechat-minigame`。
    #[serde(default = "default_platform")]
    pub platform: String,
    /// 项目名。
    pub name: Option<String>,
    /// 版本。
    pub version: Option<String>,
    /// 渲染配置。
    #[serde(default)]
    pub render: RenderConfig,
    /// 构建配置。
    #[serde(default)]
    pub build: BuildConfig,
    /// 语言配置。
    #[serde(default)]
    pub language: LanguageConfig,
    /// 热重载配置（`asgard dev`）。
    #[serde(default)]
    pub hot_reload: HotReloadConfig,
    /// UI 主题/模式注册表。
    #[serde(default)]
    pub ui: UiConfig,
    /// Tailwind 构建（默认仅 content manifest）。
    #[serde(default)]
    pub tailwind: TailwindConfig,
}

/// 渲染配置。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RenderConfig {
    /// 默认渲染模式。
    #[serde(rename = "defaultMode", default)]
    pub default_mode: String,
    /// 路由覆盖。
    #[serde(default)]
    pub routes: Vec<RouteConfig>,
}

/// 单条路由配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteConfig {
    /// 路径。
    pub path: String,
    /// 模式：ssg / ssr / csr。
    pub mode: String,
}

/// 构建配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    /// 构建模式。
    #[serde(default = "default_mode")]
    pub mode: String,
    /// 输出目录。
    #[serde(default = "default_output")]
    pub output: String,
    /// 是否压缩。
    #[serde(default)]
    pub minify: bool,
    /// 是否生成 source map。
    #[serde(default)]
    pub sourcemap: bool,
    /// 分块配置。
    #[serde(default)]
    pub chunk: ChunkConfig,
}

/// 分块配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkConfig {
    /// JS 分块模式。
    #[serde(rename = "js_mode", default = "default_js_mode")]
    pub js_mode: String,
    /// CSS 分块模式。
    #[serde(rename = "css_mode", default = "default_css_mode")]
    pub css_mode: String,
}

/// 语言配置。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LanguageConfig {
    /// AWSL 配置。
    #[serde(default)]
    pub awsl: AwslConfig,
}

/// AWSL 语言选项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AwslConfig {
    /// 严格模式。
    #[serde(rename = "strict_mode", default)]
    pub strict_mode: bool,
    /// 允许 `<template>` 标签。
    #[serde(rename = "allow_template_tag", default = "default_true")]
    pub allow_template_tag: bool,
}

/// 热重载配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotReloadConfig {
    /// 是否启用。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 监视目录。
    #[serde(default = "default_watch_dirs")]
    pub watch: Vec<String>,
    /// 忽略目录。
    #[serde(default = "default_ignore_dirs")]
    pub ignore: Vec<String>,
    /// 防抖毫秒。
    #[serde(default = "default_debounce")]
    pub debounce: u64,
    /// 开发服务器端口。
    #[serde(default = "default_dev_port")]
    pub port: u16,
}

/// Tailwind 构建配置。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TailwindConfig {
    /// 是否调用 Tailwind CLI（默认 false，仅写 content manifest）。
    #[serde(default)]
    pub enabled: bool,
    /// `.aws` 入口（含 `@tailwind` 指令）。
    pub entry: Option<String>,
    /// `tailwind.config.js` 等配置文件路径。
    #[serde(rename = "config")]
    pub config_path: Option<String>,
}

/// UI 注册配置。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiConfig {
    /// 可用主题注册。
    #[serde(default)]
    pub themes: Vec<UiRegistryEntry>,
    /// 可用模式注册。
    #[serde(default)]
    pub modes: Vec<UiRegistryEntry>,
}

/// 单条 UI 注册项。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiRegistryEntry {
    /// 逻辑 ID。
    pub id: String,
    /// 展示名。
    #[serde(default)]
    pub label: String,
}

fn default_watch_dirs() -> Vec<String> {
    vec!["source/".into(), "assets/".into()]
}

fn default_ignore_dirs() -> Vec<String> {
    vec![".git/".into(), "node_modules/".into()]
}

fn default_debounce() -> u64 {
    100
}

fn default_dev_port() -> u16 {
    3000
}

impl Default for HotReloadConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            watch: default_watch_dirs(),
            ignore: default_ignore_dirs(),
            debounce: default_debounce(),
            port: default_dev_port(),
        }
    }
}

fn default_project_type() -> String {
    "application".into()
}
fn default_target() -> String {
    "wasm32-unknown-browser-wasm".into()
}
fn default_platform() -> String {
    "browser".into()
}
fn default_mode() -> String {
    "prod".into()
}
fn default_output() -> String {
    "dist".into()
}
fn default_js_mode() -> String {
    "per-component".into()
}
fn default_css_mode() -> String {
    "merged".into()
}
fn default_true() -> bool {
    true
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self { mode: default_mode(), output: default_output(), minify: false, sourcemap: false, chunk: ChunkConfig::default() }
    }
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self { js_mode: default_js_mode(), css_mode: default_css_mode() }
    }
}

impl Default for AwslConfig {
    fn default() -> Self {
        Self { strict_mode: false, allow_template_tag: true }
    }
}

impl VoaConfig {
    /// 从项目目录加载 `asgard.config.v`。
    pub fn load(project_dir: &Path) -> Result<Self> {
        let config_path = project_dir.join("asgard.config.v");
        if !config_path.exists() {
            return Ok(Self::default_config());
        }
        let source =
            std::fs::read_to_string(&config_path).into_diagnostic().wrap_err_with(|| format!("读取配置失败: {}", config_path.display()))?;
        Self::parse(&source)
    }

    /// 解析 `asgard.config.v`（Valkyrie script）文本。
    pub fn parse(source: &str) -> Result<Self> {
        let normalized = config_script::normalize_config_source(source)?;
        let ast = parse_von(&normalized).map_err(|error| miette::miette!("VON 解析失败: {error}"))?;
        let value = from_ast(&ast);
        von_to_config(&value)
    }

    fn default_config() -> Self {
        Self {
            project_type: default_project_type(),
            target: default_target(),
            platform: default_platform(),
            name: None,
            version: None,
            render: RenderConfig::default(),
            build: BuildConfig::default(),
            language: LanguageConfig::default(),
            hot_reload: HotReloadConfig::default(),
            ui: UiConfig::default(),
            tailwind: TailwindConfig::default(),
        }
    }
}

fn von_to_config(value: &VonValue) -> Result<VoaConfig> {
    value.as_object().ok_or_else(|| miette::miette!("asgard.config.v 根节点必须是对象"))?;
    let json = von_to_json(value);
    serde_json::from_value(json).into_diagnostic().wrap_err("解析 asgard.config.v 失败")
}

fn von_to_json(value: &VonValue) -> serde_json::Value {
    match value {
        VonValue::Null | VonValue::Undefined | VonValue::Inf | VonValue::Nan => serde_json::Value::Null,
        VonValue::Boolean(v) => serde_json::Value::Bool(*v),
        VonValue::Number(v) => serde_json::Number::from_f64(*v).map(serde_json::Value::Number).unwrap_or(serde_json::Value::Null),
        VonValue::String(v) => serde_json::Value::String(v.clone()),
        VonValue::Array(VonArray { elements }) => serde_json::Value::Array(elements.iter().map(von_to_json).collect()),
        VonValue::Tuple(VonTuple { elements }) => serde_json::Value::Array(elements.iter().map(von_to_json).collect()),
        VonValue::Object(VonObject { fields }) => {
            let mut object = serde_json::Map::new();
            for field in fields {
                object.insert(field.name.clone(), von_to_json(&field.value));
            }
            serde_json::Value::Object(object)
        }
        VonValue::Enum(VonEnum { variant, payload }) => match payload {
            None => serde_json::Value::String(variant.clone()),
            Some(payload) => {
                let mut object = serde_json::Map::new();
                object.insert(variant.clone(), von_to_json(payload));
                serde_json::Value::Object(object)
            }
        },
    }
}
#[cfg(test)]
#[path = "../tests/path_wired/config.rs"]
mod config_tests;
