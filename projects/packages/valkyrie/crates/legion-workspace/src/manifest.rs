use std::{
    collections::BTreeMap,
    fmt::{Display, Formatter},
    marker::PhantomData,
    ops::Range,
};

use miette::{Diagnostic, LabeledSpan, Severity};

use crate::oak::{labeled_span, source_point_span};
use rusty_valkyrie::{CanonicalTarget, PublishFormat, RunnerSelector};
use oak_core::OakError as VonError;
use oak_von::from_str;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, MapAccess, Visitor},
};

/// Oak VON 标量可直接写字符串；serde 的 `Option<T>` 默认需要显式 option 包装。
fn deserialize_option_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct OptionStringVisitor;

    impl<'de> Visitor<'de> for OptionStringVisitor {
        type Value = Option<String>;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("string, null, or absent optional string")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(value.to_owned()))
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
            Ok(Some(value))
        }
    }

    deserializer.deserialize_any(OptionStringVisitor)
}

/// Oak VON 嵌套对象直接写 map；serde 的 `Option<T>` 默认需要显式 option 包装。
fn deserialize_option_nested<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct OptionNestedVisitor<T>(PhantomData<T>);

    impl<'de, T> Visitor<'de> for OptionNestedVisitor<T>
    where
        T: Deserialize<'de>,
    {
        type Value = Option<T>;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("object, null, or absent optional nested value")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            Ok(Some(T::deserialize(de::value::MapAccessDeserializer::new(map))?))
        }
    }

    deserializer.deserialize_any(OptionNestedVisitor(PhantomData))
}

/// Consumer package artifact mode (`legion build` target interpretation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProjectArtifactKind {
    /// CLI / command: `@main` entry, glue auto-runs `main`.
    #[default]
    Binary,
    /// Importable library: `[export]` surface, glue exposes `callExport` only.
    Library,
}

impl<'de> Deserialize<'de> for ProjectArtifactKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct KindVisitor;

        impl<'de> Visitor<'de> for KindVisitor {
            type Value = ProjectArtifactKind;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("'binary' or 'library'")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                match value.to_ascii_lowercase().as_str() {
                    "binary" => Ok(ProjectArtifactKind::Binary),
                    "library" => Ok(ProjectArtifactKind::Library),
                    other => Err(E::custom(format!("unsupported artifact kind '{other}'"))),
                }
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                self.visit_str(&value)
            }
        }

        deserializer.deserialize_any(KindVisitor)
    }
}

/// 标准库自动链接开关：控制工程是否自动引入 `core` / `std` 依赖。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AutoLinkConfig {
    /// 是否自动链接 `core` 标准库。
    #[serde(default)]
    pub core: bool,
    /// 是否自动链接 `std` 标准库。
    #[serde(default)]
    pub std: bool,
}

/// 单项依赖的声明形态：布尔开关、workspace 关键字或详细字段对象。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencySpec {
    /// 显式禁用该依赖（对应 `false`）。
    Disabled,
    /// 使用当前 workspace 成员（对应 `true` 或 `version: "workspace"`）。
    Workspace,
    /// 以对象形式声明版本、来源、路径、registry、git 等字段。
    Detailed {
        /// 版本约束或 `workspace` 关键字。
        version: Option<String>,
        /// 本地路径来源。
        path: Option<String>,
        /// 依赖 ABI 标识。
        abi: Option<String>,
        /// 来源偏好（`workspace` / `registry` / `path` / `git`）。
        source: Option<String>,
        /// registry 名称。
        registry: Option<String>,
        /// git 仓库地址。
        git: Option<String>,
        /// git 引用（分支 / 标签 / commit）。
        git_ref: Option<String>,
    },
}

/// 依赖来源偏好：解析 `source` 字段或路径 / git 字段后的归一化结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencySourcePreference {
    /// 未显式声明来源，交由上层继续推断。
    Auto,
    /// 使用 workspace 成员。
    Workspace,
    /// 从 registry 安装。
    Registry,
    /// 使用本地路径。
    Path,
    /// 从 git 仓库检出。
    Git,
}

/// 本地覆盖配置（`.config/legion/legions.von`）声明的依赖集合。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LocalLegionConfig {
    /// 配置名称。
    #[serde(default, deserialize_with = "deserialize_option_string")]
    pub name: Option<String>,
    /// 按名称声明的依赖覆盖表。
    #[serde(default)]
    pub dependencies: BTreeMap<String, DependencySpec>,
}

impl LocalLegionConfig {
    /// 从 VON 源码解析本地覆盖配置。
    pub fn parse(source: &str) -> Result<Self, ManifestError> {
        from_str(source).map_err(ManifestError::from)
    }
}

/// 单个构建目标的配置：目标平台、source map / TypeScript / WAT 产物开关与发布过滤。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildTargetSpec {
    /// 目标平台（如 `node` / `clr` / `wasm`）。
    #[serde(default = "default_canonical_target")]
    pub target: CanonicalTarget,
    /// 是否输出 source map。
    #[serde(default)]
    pub source_map: bool,
    /// 是否同时输出 TypeScript 声明。
    #[serde(default)]
    pub typescript: bool,
    /// 是否额外输出 WAT 文本。
    #[serde(default)]
    pub wat: bool,
    /// 发布格式过滤（如 `mini-game` / `mini-program` / `web-app`）。
    #[serde(default)]
    pub publish: Vec<String>,
    /// CLR Runtime Async V2（.NET 11+ await-only）；默认 state machine PE。
    #[serde(default)]
    pub runtime_async: bool,
}

impl Default for BuildTargetSpec {
    fn default() -> Self {
        Self { target: default_canonical_target(), source_map: false, typescript: false, wat: false, publish: Vec::new(), runtime_async: false }
    }
}

/// 单个发布目标的配置：目标平台、发布通道 / 格式、包 ID 与版本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishTargetSpec {
    /// 目标平台（如 `node` / `clr`）。
    #[serde(default = "default_canonical_target")]
    pub target: CanonicalTarget,
    /// Registry (`npm` / `jsr`) or artifact publish format (`web-app`, …).
    #[serde(rename = "type", default, deserialize_with = "deserialize_option_string")]
    pub channel_type: Option<String>,
    /// 发布到 registry 时的包 ID；缺省时由上层推断。
    #[serde(default, deserialize_with = "deserialize_option_string")]
    pub package_id: Option<String>,
    /// 发布版本号；缺省时使用 manifest 版本。
    #[serde(default, deserialize_with = "deserialize_option_string")]
    pub version: Option<String>,
}

impl PublishTargetSpec {
    /// When `type` names an artifact publish format, return the parsed enum.
    pub fn artifact_publish_format(&self) -> Option<PublishFormat> {
        self.channel_type.as_deref().and_then(|value| value.parse::<PublishFormat>().ok())
    }
}

impl Default for PublishTargetSpec {
    fn default() -> Self {
        Self { target: default_canonical_target(), channel_type: None, package_id: None, version: None }
    }
}

/// 第三方构建器插件配置（如 Unity 工程导出）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BuildPluginSpec {
    /// 插件类型标识（如 `unity`）。
    pub kind: String,
    /// 插件 SDK 名称。
    #[serde(default)]
    pub sdk: Option<String>,
    /// 插件运行模式。
    #[serde(default)]
    pub mode: Option<String>,
    /// 插件输入目录。
    #[serde(rename = "input_directory", default)]
    pub input_directory: Option<String>,
    /// 插件输出目录。
    #[serde(rename = "output_directory", default)]
    pub output_directory: Option<String>,
    /// 插件执行后的后续步骤说明。
    #[serde(rename = "next_step", default)]
    pub next_step: Option<String>,
    /// 导出路由表：源码导出名到插件产物路径的映射。
    #[serde(rename = "export_routes", default)]
    pub export_routes: BTreeMap<String, String>,
}

fn deserialize_option_sdk_vendor<'de, D>(deserializer: D) -> Result<Option<SdkVendorConfig>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_option_nested(deserializer)
}

fn deserialize_option_build_plugin<'de, D>(deserializer: D) -> Result<Option<BuildPluginSpec>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_option_nested(deserializer)
}

/// 第三方 SDK 厂商配置：组织、宿主、类型以及目标 / 发布列表。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SdkVendorConfig {
    /// SDK 厂商组织标识。
    #[serde(default, deserialize_with = "deserialize_option_string")]
    pub organization: Option<String>,
    /// SDK 宿主环境（如 `node` / `browser`）。
    #[serde(default, deserialize_with = "deserialize_option_string")]
    pub host: Option<String>,
    /// SDK 类型标识。
    #[serde(default, deserialize_with = "deserialize_option_string")]
    pub kind: Option<String>,
    /// 该 SDK 支持的目标平台列表。
    #[serde(default)]
    pub targets: Vec<String>,
    /// 该 SDK 支持的发布格式列表。
    #[serde(default)]
    pub publish: Vec<String>,
}

/// workspace 级默认值：版本、作者、许可证与自动链接配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkspaceDefaults {
    /// 默认版本号。
    #[serde(default, deserialize_with = "deserialize_option_string")]
    pub version: Option<String>,
    /// 默认作者。
    #[serde(default, deserialize_with = "deserialize_option_string")]
    pub author: Option<String>,
    /// 默认许可证。
    #[serde(default, deserialize_with = "deserialize_option_string")]
    pub license: Option<String>,
    /// 默认自动链接配置。
    #[serde(default)]
    pub auto_link: AutoLinkConfig,
}

/// workspace 声明的运行器绑定：目标选择器、可执行命令与参数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerBinding {
    /// 匹配的运行器目标选择器。
    pub target: RunnerSelector,
    /// 运行命令。
    pub command: String,
    /// 附加命令行参数。
    #[serde(default)]
    pub args: Vec<String>,
}

/// workspace 根清单（`legions.von`）的模型：成员列表、运行器绑定与默认值。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceManifest {
    /// workspace 名称。
    #[serde(default, deserialize_with = "deserialize_option_string")]
    pub name: Option<String>,
    /// workspace 版本号。
    #[serde(default, deserialize_with = "deserialize_option_string")]
    pub version: Option<String>,
    /// 成员工程路径列表。
    #[serde(default)]
    pub members: Vec<String>,
    /// 运行器绑定列表。
    #[serde(default)]
    pub runner: Vec<RunnerBinding>,
    /// workspace 级默认值。
    #[serde(default)]
    pub workspace: WorkspaceDefaults,
}

/// 单个工程清单（`legion.von`）的模型：依赖、构建 / 发布目标与插件配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectManifest {
    /// 工程名称。
    pub name: String,
    /// 工程版本号。
    #[serde(default, deserialize_with = "deserialize_option_string")]
    pub version: Option<String>,
    /// 工程描述。
    #[serde(default, deserialize_with = "deserialize_option_string")]
    pub description: Option<String>,
    /// `binary`（默认）或 `library`；可执行入口与导出表面由源码 `[main]` / `[export]` 标记，manifest 不声明单文件路径。
    #[serde(default)]
    pub artifact: ProjectArtifactKind,
    /// 工程级自动链接配置。
    #[serde(default)]
    pub auto_link: AutoLinkConfig,
    /// 工程声明的依赖表。
    #[serde(default)]
    pub dependencies: BTreeMap<String, DependencySpec>,
    /// 构建目标列表。
    #[serde(default)]
    pub build: Vec<BuildTargetSpec>,
    /// 发布目标列表。
    #[serde(default)]
    pub publish: Vec<PublishTargetSpec>,
    /// 第三方 SDK 厂商配置。
    #[serde(default, deserialize_with = "deserialize_option_sdk_vendor")]
    pub sdk_vendor: Option<SdkVendorConfig>,
    /// 第三方构建器插件配置。
    #[serde(default, deserialize_with = "deserialize_option_build_plugin")]
    pub build_plugin: Option<BuildPluginSpec>,
    /// 工具链版本约束（如 `{ vcc: "0.1.*" }`）；解析后由上层消费，manifest 层仅保留。
    #[serde(default)]
    pub engines: BTreeMap<String, String>,
}

/// 清单解析失败错误：保留原始 Oak 错误与可选的源码字节区间。
#[derive(Debug)]
pub enum ManifestError {
    /// VON 语法或结构解析失败。
    Parse {
        /// 原始 Oak 解析错误。
        error: VonError,
        /// 出错位置对应的源码字节区间（无法定位时为 `None`）。
        span: Option<Range<usize>>,
    },
}

impl ManifestError {
    fn from_von(error: VonError) -> Self {
        let span = source_point_span(&error);
        Self::Parse { error, span }
    }
}

impl Display for ManifestError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse { error, .. } => Display::fmt(error, f),
        }
    }
}

impl std::error::Error for ManifestError {}

impl Diagnostic for ManifestError {
    fn code<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        Some(Box::new("legion::manifest::parse"))
    }

    fn severity(&self) -> Option<Severity> {
        Some(Severity::Error)
    }

    fn help<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        Some(Box::new("请检查 `legion.von` / `legions.von` 的 `VON` 语法和字段结构"))
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        match self {
            Self::Parse { span: Some(span), .. } => Some(Box::new(std::iter::once(labeled_span("VON 解析失败位置", span)))),
            _ => None,
        }
    }
}

impl From<VonError> for ManifestError {
    fn from(value: VonError) -> Self {
        Self::from_von(value)
    }
}

impl WorkspaceManifest {
    /// 从 VON 源码解析 workspace 根清单。
    pub fn parse(source: &str) -> Result<Self, ManifestError> {
        from_str(source).map_err(ManifestError::from)
    }
}

impl ProjectManifest {
    /// 从 VON 源码解析工程清单。
    pub fn parse(source: &str) -> Result<Self, ManifestError> {
        from_str(source).map_err(ManifestError::from)
    }
}

impl DependencySpec {
    /// 归一化该依赖的来源偏好（`source` 字段优先，其次路径 / git 推断）。
    pub fn source_preference(&self) -> DependencySourcePreference {
        match self {
            Self::Disabled => DependencySourcePreference::Auto,
            Self::Workspace => DependencySourcePreference::Workspace,
            Self::Detailed { version: Some(version), path: None, abi: None, source: None, registry: None, git: None, git_ref: None }
                if version == "workspace" =>
            {
                DependencySourcePreference::Workspace
            }
            Self::Detailed { source, path, git, .. } => match source.as_deref().map(|value| value.trim().to_ascii_lowercase()) {
                Some(value) if value == "workspace" => DependencySourcePreference::Workspace,
                Some(value) if value == "registry" => DependencySourcePreference::Registry,
                Some(value) if value == "path" => DependencySourcePreference::Path,
                Some(value) if value == "git" => DependencySourcePreference::Git,
                _ if git.is_some() => DependencySourcePreference::Git,
                _ if path.is_some() => DependencySourcePreference::Path,
                _ => DependencySourcePreference::Auto,
            },
        }
    }

    /// 返回声明的版本约束或 `workspace` 关键字。
    pub fn version_hint(&self) -> Option<&str> {
        match self {
            Self::Detailed { version: Some(version), .. } => Some(version.as_str()),
            Self::Workspace => Some("workspace"),
            _ => None,
        }
    }

    /// 返回声明的 registry 名称。
    pub fn registry_hint(&self) -> Option<&str> {
        match self {
            Self::Detailed { registry: Some(registry), .. } => Some(registry.as_str()),
            _ => None,
        }
    }

    /// 返回声明的本地路径。
    pub fn path_hint(&self) -> Option<&str> {
        match self {
            Self::Detailed { path: Some(path), .. } => Some(path.as_str()),
            _ => None,
        }
    }

    /// 返回声明的 git 仓库地址。
    pub fn git_url(&self) -> Option<&str> {
        match self {
            Self::Detailed { git: Some(git), .. } => Some(git.as_str()),
            _ => None,
        }
    }

    /// 返回声明的 git 引用（分支 / 标签 / commit）。
    pub fn git_ref_hint(&self) -> Option<&str> {
        match self {
            Self::Detailed { git_ref: Some(git_ref), .. } => Some(git_ref.as_str()),
            _ => None,
        }
    }
}

impl Serialize for DependencySpec {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Disabled => false.serialize(serializer),
            Self::Workspace => true.serialize(serializer),
            Self::Detailed { version, path, abi, source, registry, git, git_ref }
                if path.is_none() && abi.is_none() && source.is_none() && registry.is_none() && git.is_none() && git_ref.is_none() =>
            {
                version.serialize(serializer)
            }
            Self::Detailed { version, path, abi, source, registry, git, git_ref } => DetailedDependencySpec {
                version: version.clone(),
                path: path.clone(),
                abi: abi.clone(),
                source: source.clone(),
                registry: registry.clone(),
                git: git.clone(),
                git_ref: git_ref.clone(),
            }
            .serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for DependencySpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match DependencySpecDef::deserialize(deserializer)? {
            DependencySpecDef::Bool(false) => Ok(Self::Disabled),
            DependencySpecDef::Bool(true) => Ok(Self::Workspace),
            DependencySpecDef::String(version) => {
                Ok(Self::Detailed { version: Some(version), path: None, abi: None, source: None, registry: None, git: None, git_ref: None })
            }
            DependencySpecDef::Detailed(value) => Ok(Self::Detailed {
                version: value.version,
                path: value.path,
                abi: value.abi,
                source: value.source,
                registry: value.registry,
                git: value.git,
                git_ref: value.git_ref,
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DetailedDependencySpec {
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    abi: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    registry: Option<String>,
    #[serde(default)]
    git: Option<String>,
    #[serde(default, rename = "ref")]
    git_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
enum DependencySpecDef {
    Bool(bool),
    String(String),
    Detailed(DetailedDependencySpec),
}

fn default_canonical_target() -> CanonicalTarget {
    CanonicalTarget::clr()
}
