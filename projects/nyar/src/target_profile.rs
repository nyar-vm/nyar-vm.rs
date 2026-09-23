//! 规范化目标到运行时策略的派生层。

use std::{
    fmt::{Display, Formatter},
    str::FromStr,
};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as DeError};

use crate::abstractions::{
    BinaryArch, BinaryFlavor, BinaryTarget, ByteOrder, CanonicalAbi, CanonicalArch, CanonicalSpecification, CanonicalTarget,
    CanonicalTargetParseError, CanonicalVendor, TargetFamily,
};
use nyar_optimizer::{HostProjectionBoundary, ReferenceManagement};

/// 编译目标模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TargetMode {
    /// 开发模式。
    Dev,
    /// 生产模式。
    #[default]
    Prod,
}

/// 后端家族。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TargetBackendFamily {
    /// `NyarVM`
    NyarVm,
    /// `CLR`
    Clr,
    /// `JVM`
    Jvm,
    /// `WASM`
    Wasm,
    /// 原生目标。
    Native,
    /// GPU / Shader
    Gpu,
    /// 未知。
    #[default]
    Unknown,
}

/// 宿主类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TargetHostKind {
    /// 未知宿主。
    #[default]
    Unknown,
    /// `NyarVM`
    NyarVm,
    /// `JVM`
    Jvm,
    /// `.NET`
    DotNet,
    /// `JavaScript` 宿主。
    JavaScript,
    /// 原生宿主。
    Native,
    /// 浏览器宿主。
    Browser,
    /// `WASI`
    Wasi,
    /// GPU / Shader
    Gpu,
}

/// 入口包装策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum WrapStrategy {
    /// 直接调用入口。
    #[default]
    Direct,
    /// 生成包装入口。
    Wrapper,
    /// 由宿主托管入口。
    Hosted,
}

/// 发布格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PublishFormat {
    /// 目录结构。
    Directory,
    /// bundle。
    Bundle,
    /// `WASM` 模块。
    WasmModule,
    /// `WASM Component`
    WasmComponent,
    /// Web 应用。
    WebApp,
    /// 扩展包。
    Extension,
    /// 小游戏包。
    MiniGame,
    /// 微信小程序包。
    MiniProgram,
    /// `jar`
    Jar,
    /// `jlink` 镜像。
    JlinkImage,
    /// `apk`
    Apk,
    /// `aab`
    Aab,
    /// `ipa`
    Ipa,
    /// App bundle。
    AppBundle,
    /// `pkg`
    Pkg,
    /// `zip`
    Zip,
    /// `tar`
    Tar,
    /// 单文件。
    SingleFile,
    /// Shader 模块。
    ShaderModule,
    /// 资源包。
    AssetPack,
    /// `OCI`
    Oci,
    /// `deb`
    Deb,
    /// `rpm`
    Rpm,
    /// `AppImage`
    AppImage,
    /// `nupkg` / `nuget` 包。
    Nuget,
    /// Unity Player 脚本包（MSIL → Editor 导入）。
    UnityPlayer,
}

impl PublishFormat {
    /// 解析发布格式。
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "directory" => Some(Self::Directory),
            "bundle" => Some(Self::Bundle),
            "wasm-module" => Some(Self::WasmModule),
            "wasm-component" => Some(Self::WasmComponent),
            "web-app" => Some(Self::WebApp),
            "extension" => Some(Self::Extension),
            "mini-game" => Some(Self::MiniGame),
            "mini-program" => Some(Self::MiniProgram),
            "jar" => Some(Self::Jar),
            "jlink-image" => Some(Self::JlinkImage),
            "apk" => Some(Self::Apk),
            "aab" => Some(Self::Aab),
            "ipa" => Some(Self::Ipa),
            "app-bundle" => Some(Self::AppBundle),
            "pkg" => Some(Self::Pkg),
            "zip" => Some(Self::Zip),
            "tar" => Some(Self::Tar),
            "single-file" => Some(Self::SingleFile),
            "shader-module" => Some(Self::ShaderModule),
            "asset-pack" => Some(Self::AssetPack),
            "oci" => Some(Self::Oci),
            "deb" => Some(Self::Deb),
            "rpm" => Some(Self::Rpm),
            "appimage" => Some(Self::AppImage),
            "nuget" | "nupkg" => Some(Self::Nuget),
            "unity-player" => Some(Self::UnityPlayer),
            _ => None,
        }
    }

    /// 返回标准字符串。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Directory => "directory",
            Self::Bundle => "bundle",
            Self::WasmModule => "wasm-module",
            Self::WasmComponent => "wasm-component",
            Self::WebApp => "web-app",
            Self::Extension => "extension",
            Self::MiniGame => "mini-game",
            Self::MiniProgram => "mini-program",
            Self::Jar => "jar",
            Self::JlinkImage => "jlink-image",
            Self::Apk => "apk",
            Self::Aab => "aab",
            Self::Ipa => "ipa",
            Self::AppBundle => "app-bundle",
            Self::Pkg => "pkg",
            Self::Zip => "zip",
            Self::Tar => "tar",
            Self::SingleFile => "single-file",
            Self::ShaderModule => "shader-module",
            Self::AssetPack => "asset-pack",
            Self::Oci => "oci",
            Self::Deb => "deb",
            Self::Rpm => "rpm",
            Self::AppImage => "appimage",
            Self::Nuget => "nuget",
            Self::UnityPlayer => "unity-player",
        }
    }
}

impl Display for PublishFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for PublishFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| format!("unsupported publish format '{}'", s))
    }
}

impl Serialize for PublishFormat {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PublishFormat {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(D::Error::custom)
    }
}

/// 运行器家族。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RunnerFamily {
    /// `dotnet`
    Clr,
    /// `java`
    Jvm,
    /// `node`
    Node,
    /// Windows 原生进程。
    Windows,
    /// `WASI`
    Wasi,
    /// `NyarVM`
    NyarVm,
}

impl RunnerFamily {
    /// 解析运行器家族。
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "clr" => Some(Self::Clr),
            "jvm" => Some(Self::Jvm),
            "node" => Some(Self::Node),
            "windows" => Some(Self::Windows),
            "wasi" => Some(Self::Wasi),
            "nyar-vm" | "nyarvm" | "nyar" | "nvm" => Some(Self::NyarVm),
            _ => None,
        }
    }

    /// 返回标准字符串。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Clr => "clr",
            Self::Jvm => "jvm",
            Self::Node => "node",
            Self::Windows => "windows",
            Self::Wasi => "wasi",
            Self::NyarVm => "nyar-vm",
        }
    }
}

impl Display for RunnerFamily {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for RunnerFamily {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| format!("unsupported runner family '{}'", s))
    }
}

impl Serialize for RunnerFamily {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RunnerFamily {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(D::Error::custom)
    }
}

/// 运行器目标选择器。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RunnerSelector {
    /// 按运行器家族匹配。
    Family(RunnerFamily),
    /// 按完整规范化目标匹配。
    Canonical(CanonicalTarget),
}

impl RunnerSelector {
    /// 判断是否匹配指定运行器家族与规范化目标。
    pub fn matches(&self, family: RunnerFamily, target: &CanonicalTarget) -> bool {
        match self {
            Self::Family(value) => *value == family,
            Self::Canonical(value) => value == target,
        }
    }

    /// 转换为运行器家族。
    pub fn to_runner_family(&self) -> RunnerFamily {
        match self {
            Self::Family(value) => *value,
            Self::Canonical(value) => value.to_profile(None).runner_family(),
        }
    }
}

impl Display for RunnerSelector {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Family(value) => Display::fmt(value, f),
            Self::Canonical(value) => Display::fmt(value, f),
        }
    }
}

impl FromStr for RunnerSelector {
    type Err = CanonicalTargetParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(family) = s.parse::<RunnerFamily>() {
            return Ok(Self::Family(family));
        }
        Ok(Self::Canonical(s.parse()?))
    }
}

impl Serialize for RunnerSelector {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for RunnerSelector {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(D::Error::custom)
    }
}

/// 入口策略。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryPolicy {
    /// 默认入口名。
    pub default_entry: String,
    /// 入口包装策略。
    pub wrap_strategy: WrapStrategy,
    /// 是否生成包装入口。
    pub generate_wrapper: bool,
}

impl Default for EntryPolicy {
    fn default() -> Self {
        Self { default_entry: "main".to_string(), wrap_strategy: WrapStrategy::Direct, generate_wrapper: false }
    }
}

/// 产物策略。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactPolicy {
    /// 主产物扩展名。
    pub primary_extension: String,
    /// 默认发布格式。
    pub default_publish_format: PublishFormat,
    /// 支持的发布格式。
    pub supported_publish_formats: Vec<PublishFormat>,
    /// 必需适配层。
    pub required_adaptors: Vec<String>,
    /// 是否生成运行时配置。
    pub generate_runtime_config: bool,
    /// 是否生成调试符号。
    pub generate_debug_symbols: bool,
    /// 是否生成启动脚本。
    pub generate_launch_scripts: bool,
    /// 是否生成 `XML` 文档。
    pub generate_xml_doc: bool,
    /// 是否要求代码签名。
    pub requires_code_signing: bool,
    /// 是否支持商店分发。
    pub supports_store_distribution: bool,
}

impl Default for ArtifactPolicy {
    fn default() -> Self {
        Self {
            primary_extension: String::new(),
            default_publish_format: PublishFormat::Directory,
            supported_publish_formats: vec![PublishFormat::Directory],
            required_adaptors: Vec::new(),
            generate_runtime_config: false,
            generate_debug_symbols: false,
            generate_launch_scripts: true,
            generate_xml_doc: false,
            requires_code_signing: false,
            supports_store_distribution: false,
        }
    }
}

/// 目标策略配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetProfile {
    /// 规范化目标。
    pub canonical_target: CanonicalTarget,
    /// 目标模式。
    pub target_mode: TargetMode,
    /// 后端家族。
    pub backend_family: TargetBackendFamily,
    /// 宿主类型。
    pub host_kind: TargetHostKind,
    /// 宿主边界。
    pub host_boundary: HostProjectionBoundary,
    /// 宿主风味。
    pub host_flavor: String,
    /// 实际 ABI。
    pub abi: CanonicalAbi,
    /// 引用对象管理策略。
    pub reference_management: ReferenceManagement,
    /// 能力标签。
    pub capability_tags: Vec<String>,
    /// 入口策略。
    pub entry_policy: EntryPolicy,
    /// 产物策略。
    pub artifact_policy: ArtifactPolicy,
}

impl TargetProfile {
    /// 选择默认运行器家族。
    pub fn runner_family(&self) -> RunnerFamily {
        match self.host_kind {
            TargetHostKind::NyarVm => RunnerFamily::NyarVm,
            TargetHostKind::DotNet => RunnerFamily::Clr,
            TargetHostKind::Jvm => RunnerFamily::Jvm,
            TargetHostKind::Wasi => RunnerFamily::Wasi,
            TargetHostKind::Browser | TargetHostKind::JavaScript => RunnerFamily::Node,
            TargetHostKind::Native if self.canonical_target.specification == CanonicalSpecification::Windows => RunnerFamily::Windows,
            _ => RunnerFamily::Clr,
        }
    }

    /// 判断发布格式是否受支持。
    pub fn supports_publish_format(&self, format: PublishFormat) -> bool {
        self.artifact_policy.supported_publish_formats.contains(&format)
    }
}

impl CanonicalTarget {
    /// 计算该目标的有效 ABI。
    pub fn resolved_abi(self) -> CanonicalAbi {
        if let Some(abi) = self.abi {
            return abi;
        }
        match self.arch {
            CanonicalArch::Clr => CanonicalAbi::Clr,
            CanonicalArch::Jvm => CanonicalAbi::Jvm,
            CanonicalArch::NyarVm => CanonicalAbi::Managed,
            CanonicalArch::Wasm32 | CanonicalArch::Wasm64 => CanonicalAbi::WebAssembly,
            CanonicalArch::AArch64 => CanonicalAbi::Aapcs64,
            CanonicalArch::X86_64 if self.specification == CanonicalSpecification::Windows => CanonicalAbi::Msvc,
            CanonicalArch::X86_64 => CanonicalAbi::SystemV,
            _ => CanonicalAbi::SystemV,
        }
    }

    /// 派生目标策略配置。
    pub fn to_profile(self, target_mode: Option<TargetMode>) -> TargetProfile {
        let abi = self.resolved_abi();
        TargetProfile {
            canonical_target: self,
            target_mode: target_mode.unwrap_or_default(),
            backend_family: self.derive_backend_family(),
            host_kind: self.derive_host_kind(abi),
            host_boundary: self.derive_host_boundary(abi),
            host_flavor: self.derive_host_flavor(abi).to_string(),
            abi,
            reference_management: self.derive_reference_management(),
            capability_tags: self.derive_capability_tags(),
            entry_policy: self.derive_entry_policy(abi),
            artifact_policy: self.derive_artifact_policy(abi),
        }
    }

    fn derive_backend_family(self) -> TargetBackendFamily {
        match self.arch {
            CanonicalArch::NyarVm => TargetBackendFamily::NyarVm,
            CanonicalArch::Clr => TargetBackendFamily::Clr,
            CanonicalArch::Jvm => TargetBackendFamily::Jvm,
            CanonicalArch::Wasm32 | CanonicalArch::Wasm64 => TargetBackendFamily::Wasm,
            _ => TargetBackendFamily::Native,
        }
    }

    fn derive_host_kind(self, abi: CanonicalAbi) -> TargetHostKind {
        match self.arch {
            CanonicalArch::NyarVm => return TargetHostKind::NyarVm,
            CanonicalArch::Clr => return TargetHostKind::DotNet,
            CanonicalArch::Jvm => return TargetHostKind::Jvm,
            _ => {}
        }
        match self.vendor {
            CanonicalVendor::Node | CanonicalVendor::Deno | CanonicalVendor::Bun => return TargetHostKind::JavaScript,
            _ => {}
        }
        match self.specification {
            CanonicalSpecification::Browser => TargetHostKind::Browser,
            CanonicalSpecification::Wasi => TargetHostKind::Wasi,
            CanonicalSpecification::Windows
            | CanonicalSpecification::Linux
            | CanonicalSpecification::MacOs
            | CanonicalSpecification::Android
            | CanonicalSpecification::Ios => TargetHostKind::Native,
            CanonicalSpecification::Unknown if abi.is_wasi_component() => TargetHostKind::Wasi,
            _ => TargetHostKind::Unknown,
        }
    }

    fn derive_host_flavor(self, abi: CanonicalAbi) -> &'static str {
        match self.arch {
            CanonicalArch::NyarVm => "nyarvm",
            CanonicalArch::Clr => "dotnet",
            CanonicalArch::Jvm => {
                if self.specification == CanonicalSpecification::Android {
                    "android-art"
                }
                else {
                    "openjdk"
                }
            }
            _ => match self.vendor {
                CanonicalVendor::Node => "node",
                CanonicalVendor::Deno => "deno",
                CanonicalVendor::Bun => "bun",
                _ => match self.specification {
                    CanonicalSpecification::Browser => "web-standard",
                    CanonicalSpecification::Wasi => match abi {
                        CanonicalAbi::WasiP3 => "wasi-component-model-p3",
                        _ => "wasi-component-model",
                    },
                    CanonicalSpecification::Windows => "win32",
                    CanonicalSpecification::Linux => "linux-gnu",
                    CanonicalSpecification::MacOs => "apple-darwin",
                    CanonicalSpecification::Android => "android-native",
                    CanonicalSpecification::Ios => "ios-native",
                    CanonicalSpecification::Unknown => "default",
                },
            },
        }
    }

    fn derive_host_boundary(self, abi: CanonicalAbi) -> HostProjectionBoundary {
        match self.arch {
            CanonicalArch::NyarVm => return HostProjectionBoundary::Vm,
            CanonicalArch::Clr => return HostProjectionBoundary::Clr,
            CanonicalArch::Jvm => return HostProjectionBoundary::Jvm,
            _ => {}
        }
        match self.vendor {
            CanonicalVendor::Node | CanonicalVendor::Deno | CanonicalVendor::Bun => return HostProjectionBoundary::WasmJsGlue,
            _ => {}
        }
        match self.specification {
            CanonicalSpecification::Browser => HostProjectionBoundary::WasmJsGlue,
            CanonicalSpecification::Wasi => HostProjectionBoundary::WasiComponent,
            CanonicalSpecification::Windows
            | CanonicalSpecification::Linux
            | CanonicalSpecification::MacOs
            | CanonicalSpecification::Android
            | CanonicalSpecification::Ios => HostProjectionBoundary::Native,
            CanonicalSpecification::Unknown if abi.is_wasi_component() => HostProjectionBoundary::WasiComponent,
            _ => HostProjectionBoundary::Native,
        }
    }

    fn derive_reference_management(self) -> ReferenceManagement {
        match self.derive_backend_family() {
            TargetBackendFamily::Native | TargetBackendFamily::Gpu => ReferenceManagement::PerceusRc,
            _ => ReferenceManagement::HostGc,
        }
    }

    fn derive_capability_tags(self) -> Vec<String> {
        let values: &[&str] = match self.arch {
            CanonicalArch::NyarVm => &["vm", "module-loader", "precise-gc"],
            CanonicalArch::Clr => &["managed", "reflection", "filesystem", "precise-gc"],
            CanonicalArch::Jvm => {
                if self.specification == CanonicalSpecification::Android {
                    &["managed", "android-lifecycle", "asset-loader", "precise-gc"]
                }
                else {
                    &["managed", "reflection", "filesystem", "precise-gc"]
                }
            }
            _ => match self.vendor {
                CanonicalVendor::Node | CanonicalVendor::Deno | CanonicalVendor::Bun => {
                    &["javascript", "esmodule", "filesystem", "timers", "precise-gc"]
                }
                _ => match self.specification {
                    CanonicalSpecification::Browser => &["javascript", "dom", "canvas", "fetch", "esmodule", "precise-gc"],
                    CanonicalSpecification::Wasi => match self.resolved_abi() {
                        CanonicalAbi::WasiP3 => &["wasi", "wasip3", "filesystem", "cli", "component-model", "precise-gc"],
                        _ => &["wasi", "wasip2", "filesystem", "cli", "component-model", "precise-gc"],
                    },
                    CanonicalSpecification::Windows | CanonicalSpecification::Linux | CanonicalSpecification::MacOs => {
                        &["native", "filesystem", "process", "perceus-rc"]
                    }
                    CanonicalSpecification::Android => &["native", "mobile-lifecycle", "asset-loader", "perceus-rc"],
                    CanonicalSpecification::Ios => &["native", "mobile-lifecycle", "bundle-resource", "perceus-rc"],
                    CanonicalSpecification::Unknown => &[],
                },
            },
        };
        values.iter().map(|value| (*value).to_string()).collect()
    }

    fn derive_entry_policy(self, abi: CanonicalAbi) -> EntryPolicy {
        if abi.is_wasi_component() {
            return EntryPolicy { default_entry: "main".to_string(), wrap_strategy: WrapStrategy::Hosted, generate_wrapper: true };
        }
        match self.arch {
            CanonicalArch::Clr => {
                EntryPolicy { default_entry: "Main".to_string(), wrap_strategy: WrapStrategy::Wrapper, generate_wrapper: true }
            }
            CanonicalArch::Jvm => {
                EntryPolicy { default_entry: "main".to_string(), wrap_strategy: WrapStrategy::Wrapper, generate_wrapper: true }
            }
            _ if self.specification == CanonicalSpecification::Browser => {
                EntryPolicy { default_entry: "main".to_string(), wrap_strategy: WrapStrategy::Hosted, generate_wrapper: false }
            }
            _ if matches!(self.vendor, CanonicalVendor::Node | CanonicalVendor::Deno | CanonicalVendor::Bun) => {
                EntryPolicy { default_entry: "main".to_string(), wrap_strategy: WrapStrategy::Hosted, generate_wrapper: false }
            }
            _ => EntryPolicy::default(),
        }
    }

    fn derive_artifact_policy(self, abi: CanonicalAbi) -> ArtifactPolicy {
        match self.arch {
            CanonicalArch::NyarVm => ArtifactPolicy {
                primary_extension: ".nyar".to_string(),
                default_publish_format: PublishFormat::Bundle,
                supported_publish_formats: vec![PublishFormat::Bundle],
                required_adaptors: vec!["std:nyarvm".to_string()],
                generate_launch_scripts: false,
                ..ArtifactPolicy::default()
            },
            CanonicalArch::Wasm32 | CanonicalArch::Wasm64 => ArtifactPolicy {
                primary_extension: ".wasm".to_string(),
                default_publish_format: if abi.is_wasi_component() { PublishFormat::WasmComponent } else { PublishFormat::WasmModule },
                supported_publish_formats: if abi.is_wasi_component() {
                    vec![PublishFormat::WasmComponent, PublishFormat::Oci]
                }
                else {
                    vec![
                        PublishFormat::WasmModule,
                        PublishFormat::WebApp,
                        PublishFormat::Extension,
                        PublishFormat::MiniGame,
                        PublishFormat::MiniProgram,
                    ]
                },
                required_adaptors: if abi.is_wasi_component() {
                    vec![if abi == CanonicalAbi::WasiP3 { "std:wasip3".to_string() } else { "std:wasi".to_string() }]
                }
                else {
                    let adaptor = match self.vendor {
                        CanonicalVendor::Node => "std:node",
                        CanonicalVendor::Deno => "std:deno",
                        CanonicalVendor::Bun => "std:bun",
                        _ => "std:web",
                    };
                    vec![adaptor.to_string()]
                },
                generate_launch_scripts: false,
                ..ArtifactPolicy::default()
            },
            CanonicalArch::Jvm => ArtifactPolicy {
                primary_extension: ".jar".to_string(),
                default_publish_format: PublishFormat::Jar,
                supported_publish_formats: if self.specification == CanonicalSpecification::Android {
                    vec![PublishFormat::Apk, PublishFormat::Aab, PublishFormat::Jar]
                }
                else {
                    vec![PublishFormat::Jar, PublishFormat::JlinkImage]
                },
                generate_launch_scripts: true,
                requires_code_signing: self.specification == CanonicalSpecification::Android,
                supports_store_distribution: self.specification == CanonicalSpecification::Android,
                ..ArtifactPolicy::default()
            },
            CanonicalArch::Clr => ArtifactPolicy {
                primary_extension: ".exe".to_string(),
                default_publish_format: match self.specification {
                    CanonicalSpecification::Windows => PublishFormat::Directory,
                    CanonicalSpecification::Android => PublishFormat::Apk,
                    CanonicalSpecification::Ios => PublishFormat::Ipa,
                    _ => PublishFormat::Directory,
                },
                supported_publish_formats: match self.specification {
                    CanonicalSpecification::Windows => {
                        vec![PublishFormat::Directory, PublishFormat::Zip, PublishFormat::SingleFile, PublishFormat::UnityPlayer]
                    }
                    CanonicalSpecification::Android => vec![PublishFormat::Apk, PublishFormat::Aab],
                    CanonicalSpecification::Ios => vec![PublishFormat::Ipa, PublishFormat::AppBundle],
                    CanonicalSpecification::MacOs => {
                        vec![PublishFormat::AppBundle, PublishFormat::Pkg, PublishFormat::Directory, PublishFormat::UnityPlayer]
                    }
                    _ => vec![PublishFormat::Directory, PublishFormat::Tar, PublishFormat::SingleFile, PublishFormat::UnityPlayer],
                },
                required_adaptors: match self.specification {
                    CanonicalSpecification::Android => vec!["std:android".to_string()],
                    CanonicalSpecification::Ios => vec!["std:ios".to_string()],
                    _ => vec!["std:dotnet".to_string()],
                },
                generate_runtime_config: true,
                generate_debug_symbols: true,
                generate_launch_scripts: true,
                generate_xml_doc: true,
                requires_code_signing: matches!(
                    self.specification,
                    CanonicalSpecification::Windows
                        | CanonicalSpecification::Android
                        | CanonicalSpecification::Ios
                        | CanonicalSpecification::MacOs
                ),
                supports_store_distribution: matches!(
                    self.specification,
                    CanonicalSpecification::Windows | CanonicalSpecification::Android | CanonicalSpecification::Ios
                ),
            },
            _ => ArtifactPolicy {
                primary_extension: if self.specification == CanonicalSpecification::Windows { ".exe".to_string() } else { String::new() },
                default_publish_format: match self.specification {
                    CanonicalSpecification::Windows => PublishFormat::Directory,
                    CanonicalSpecification::Android => PublishFormat::Apk,
                    CanonicalSpecification::Ios => PublishFormat::Ipa,
                    CanonicalSpecification::MacOs => PublishFormat::AppBundle,
                    _ => PublishFormat::Directory,
                },
                supported_publish_formats: match self.specification {
                    CanonicalSpecification::Windows => {
                        vec![PublishFormat::Directory, PublishFormat::Zip, PublishFormat::SingleFile]
                    }
                    CanonicalSpecification::Android => {
                        vec![PublishFormat::Apk, PublishFormat::Aab]
                    }
                    CanonicalSpecification::Ios => {
                        vec![PublishFormat::Ipa, PublishFormat::AppBundle]
                    }
                    CanonicalSpecification::MacOs => {
                        vec![PublishFormat::AppBundle, PublishFormat::Pkg, PublishFormat::Directory]
                    }
                    _ => {
                        vec![PublishFormat::Directory, PublishFormat::Tar, PublishFormat::Deb, PublishFormat::Rpm, PublishFormat::AppImage]
                    }
                },
                required_adaptors: match self.specification {
                    CanonicalSpecification::Android => vec!["std:android".to_string()],
                    CanonicalSpecification::Ios => vec!["std:ios".to_string()],
                    _ => vec!["std:native".to_string()],
                },
                generate_debug_symbols: true,
                requires_code_signing: matches!(
                    self.specification,
                    CanonicalSpecification::Windows
                        | CanonicalSpecification::Android
                        | CanonicalSpecification::Ios
                        | CanonicalSpecification::MacOs
                ),
                supports_store_distribution: matches!(
                    self.specification,
                    CanonicalSpecification::Windows | CanonicalSpecification::Android | CanonicalSpecification::Ios
                ),
                ..ArtifactPolicy::default()
            },
        }
    }
}

impl From<CanonicalTarget> for BinaryTarget {
    fn from(value: CanonicalTarget) -> Self {
        let family = match value.to_profile(None).backend_family {
            TargetBackendFamily::Clr => TargetFamily::Clr,
            TargetBackendFamily::Jvm => TargetFamily::Jvm,
            TargetBackendFamily::Wasm => TargetFamily::Wasm,
            TargetBackendFamily::NyarVm => TargetFamily::NyarVm,
            _ => TargetFamily::Native,
        };
        let arch = match value.arch {
            CanonicalArch::X86 => BinaryArch::X86,
            CanonicalArch::AArch64 => BinaryArch::Arm64,
            CanonicalArch::X86_64 | CanonicalArch::Native => BinaryArch::X64,
            _ => BinaryArch::Any,
        };
        let flavor = match family {
            TargetFamily::Clr | TargetFamily::Jvm | TargetFamily::NyarVm => BinaryFlavor::ManagedClr,
            _ => BinaryFlavor::Native,
        };
        BinaryTarget { family, arch, byte_order: ByteOrder::LittleEndian, flavor }
    }
}
