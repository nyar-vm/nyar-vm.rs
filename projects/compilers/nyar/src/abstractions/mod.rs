#![doc = include_str!("readme.md")]

use miette::{Diagnostic, Severity};
use std::{
    fmt::{Display, Formatter},
    str::FromStr,
};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as DeError};

/// 目标家族。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetFamily {
    /// `CLR` / `ECMA-335`
    Clr,
    /// `JVM` / `ClassFile`
    Jvm,
    /// `WASM`
    Wasm,
    /// `native`
    Native,
    /// `CPU/VM`
    NyarVm,
    /// `GPU / Shader`
    Gpu,
}

/// 二进制架构。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryArch {
    /// 与具体机器无关。
    Any,
    /// `x86`
    X86,
    /// `x64`
    X64,
    /// `arm64`
    Arm64,
}

/// 字节序。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ByteOrder {
    /// 小端。
    LittleEndian,
    /// 大端。
    BigEndian,
}

/// 产物文件格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtifactFormat {
    /// `PE/COFF`
    Pe,
    /// `COFF object`
    Coff,
    /// `ELF` 可执行或目标文件
    Elf,
    /// `MSIL` 文本
    MsilText,
    /// 原始字节流
    RawBinary,
    /// SPIR-V 着色器模块
    SpirvModule,
    /// DXIL 容器
    DxilContainer,
}

/// 产物种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtifactKind {
    /// 可执行文件。
    Executable,
    /// 动态库。
    DynamicLibrary,
    /// 静态目标文件。
    Object,
    /// 文本汇编或调试输出。
    AssemblyListing,
}

/// 容器对象种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectKind {
    /// 单个 object file。
    ObjectFile,
    /// 可执行镜像。
    ExecutableImage,
    /// 动态库镜像。
    SharedImage,
}

/// 二进制口味。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryFlavor {
    /// 纯托管 `CLR` 镜像。
    ManagedClr,
    /// 非托管原生镜像。
    Native,
    /// 混合模式。
    Mixed,
}

/// 编译后端真正消费的输入种类。
///
/// 它是路线声明，不是统一 `IR`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackendInputKind {
    /// `CLR` lane 低层输入。
    ClrImage,
    /// `JVM` lane 低层输入。
    JvmClassFile,
    /// `WASM` lane 低层输入。
    WasmModule,
    /// `COFF` 目标文件输入。
    CoffObject,
    /// `PE` 镜像输入。
    PeImage,
    /// `MSIL` 文本输入。
    MsilText,
    /// SPIR-V 模块输入。
    SpirvModule,
    /// DXIL 容器输入。
    DxilContainer,
}

/// 目标描述。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BinaryTarget {
    /// 目标家族。
    pub family: TargetFamily,
    /// 架构。
    pub arch: BinaryArch,
    /// 字节序。
    pub byte_order: ByteOrder,
    /// 容器口味。
    pub flavor: BinaryFlavor,
}

impl BinaryTarget {
    /// 创建一个新的目标描述。
    pub fn new(family: TargetFamily, arch: BinaryArch, flavor: BinaryFlavor) -> Self {
        Self { family, arch, byte_order: ByteOrder::LittleEndian, flavor }
    }

    /// 创建一个通用 `CLR` 目标。
    pub fn clr() -> Self {
        Self::new(TargetFamily::Clr, BinaryArch::Any, BinaryFlavor::ManagedClr)
    }
}

/// 规范化目标架构。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CanonicalArch {
    /// `x86`
    X86,
    /// `x86_64`
    X86_64,
    /// `aarch64`
    AArch64,
    /// `arm`
    Arm,
    /// `riscv32`
    RiscV32,
    /// `riscv64`
    RiscV64,
    /// `wasm32`
    Wasm32,
    /// `wasm64`
    Wasm64,
    /// `CLR`
    Clr,
    /// `JVM`
    Jvm,
    /// `Nyar VM`
    NyarVm,
    /// 宿主原生二进制占位。
    Native,
}

impl CanonicalArch {
    fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "x86" | "i686" => Some(Self::X86),
            "x86_64" | "x64" | "amd64" => Some(Self::X86_64),
            "aarch64" | "arm64" => Some(Self::AArch64),
            "arm" => Some(Self::Arm),
            "riscv32" => Some(Self::RiscV32),
            "riscv64" => Some(Self::RiscV64),
            "wasm32" => Some(Self::Wasm32),
            "wasm64" => Some(Self::Wasm64),
            "clr" => Some(Self::Clr),
            "jvm" => Some(Self::Jvm),
            "nyar" | "nyarvm" => Some(Self::NyarVm),
            "native" => Some(Self::Native),
            _ => None,
        }
    }

    /// 返回架构的字符串表示，用于模板预处理等场景。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::X86 => "x86",
            Self::X86_64 => "x86_64",
            Self::AArch64 => "aarch64",
            Self::Arm => "arm",
            Self::RiscV32 => "riscv32",
            Self::RiscV64 => "riscv64",
            Self::Wasm32 => "wasm32",
            Self::Wasm64 => "wasm64",
            Self::Clr => "clr",
            Self::Jvm => "jvm",
            Self::NyarVm => "nyar",
            Self::Native => "native",
        }
    }
}

/// 规范化目标提供方。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CanonicalVendor {
    /// 未指定。
    Unknown,
    /// `microsoft`
    Microsoft,
    /// `openjdk`
    OpenJdk,
    /// `pc`
    Pc,
    /// `apple`
    Apple,
    /// `android`
    Android,
    /// `node`
    Node,
    /// `deno`
    Deno,
    /// `bun`
    Bun,
}

impl CanonicalVendor {
    fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "unknown" => Some(Self::Unknown),
            "microsoft" => Some(Self::Microsoft),
            "openjdk" | "open-jdk" => Some(Self::OpenJdk),
            "pc" => Some(Self::Pc),
            "apple" => Some(Self::Apple),
            "android" => Some(Self::Android),
            "node" => Some(Self::Node),
            "deno" => Some(Self::Deno),
            "bun" => Some(Self::Bun),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Microsoft => "microsoft",
            Self::OpenJdk => "openjdk",
            Self::Pc => "pc",
            Self::Apple => "apple",
            Self::Android => "android",
            Self::Node => "node",
            Self::Deno => "deno",
            Self::Bun => "bun",
        }
    }
}

/// 规范化目标规格。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CanonicalSpecification {
    /// 未指定。
    Unknown,
    /// 浏览器宿主。
    Browser,
    /// `WASI`
    Wasi,
    /// `Windows`
    Windows,
    /// `Linux`
    Linux,
    /// `macOS`
    MacOs,
    /// `Android`
    Android,
    /// `iOS`
    Ios,
}

impl CanonicalSpecification {
    fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "unknown" => Some(Self::Unknown),
            "browser" | "web" => Some(Self::Browser),
            "wasi" => Some(Self::Wasi),
            "windows" | "win32" => Some(Self::Windows),
            "linux" => Some(Self::Linux),
            "macos" | "mac-os" | "darwin" => Some(Self::MacOs),
            "android" => Some(Self::Android),
            "ios" => Some(Self::Ios),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Browser => "browser",
            Self::Wasi => "wasi",
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::MacOs => "macos",
            Self::Android => "android",
            Self::Ios => "ios",
        }
    }
}

/// 规范化目标 ABI。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CanonicalAbi {
    /// 托管运行时。
    Managed,
    /// `CLR`
    Clr,
    /// `JVM`
    Jvm,
    /// `WASM`
    WebAssembly,
    /// `WASI` Preview 2 / wasip2 component model（默认 `wasi` / `wasip2` 别名）。
    Wasi,
    /// `WASI` 0.3 / wasip3 component model（与 Preview2 共享 core codegen，适配层版本不同）。
    WasiP3,
    /// `MSVC`
    Msvc,
    /// `GNU`
    Gnu,
    /// `System V`
    SystemV,
    /// `AAPCS64`
    Aapcs64,
}

impl CanonicalAbi {
    fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "managed" => Some(Self::Managed),
            "clr" => Some(Self::Clr),
            "jvm" => Some(Self::Jvm),
            "wasm" | "webassembly" | "web-assembly" => Some(Self::WebAssembly),
            "wasi" | "wasip1" | "wasi-p1" | "wasip2" | "wasi-p2" => Some(Self::Wasi),
            "wasip3" | "wasi-p3" => Some(Self::WasiP3),
            "msvc" | "microsoft-x64" => Some(Self::Msvc),
            "gnu" => Some(Self::Gnu),
            "systemv" | "system-v" => Some(Self::SystemV),
            "aapcs64" => Some(Self::Aapcs64),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Managed => "managed",
            Self::Clr => "clr",
            Self::Jvm => "jvm",
            Self::WebAssembly => "wasm",
            Self::Wasi => "wasi",
            Self::WasiP3 => "wasip3",
            Self::Msvc => "msvc",
            Self::Gnu => "gnu",
            Self::SystemV => "systemv",
            Self::Aapcs64 => "aapcs64",
        }
    }

    /// 是否为 WASI component-model ABI（wasip2 或 wasip3）。
    pub fn is_wasi_component(self) -> bool {
        matches!(self, Self::Wasi | Self::WasiP3)
    }
}

/// 规范化目标解析错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalTargetParseError {
    message: String,
}

impl CanonicalTargetParseError {
    fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl Display for CanonicalTargetParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.message, f)
    }
}

impl std::error::Error for CanonicalTargetParseError {}

impl Diagnostic for CanonicalTargetParseError {
    fn code<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        Some(Box::new("nyar::canonical_target::parse"))
    }

    fn severity(&self) -> Option<Severity> {
        Some(Severity::Error)
    }

    fn help<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        Some(Box::new("请使用 `<arch>-<vendor>-<specification>[-<abi>]` 格式，例如 `clr-microsoft-unknown-managed`"))
    }
}

/// 规范化目标四元组。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CanonicalTarget {
    /// 目标架构。
    pub arch: CanonicalArch,
    /// 目标提供方。
    pub vendor: CanonicalVendor,
    /// 目标规格。
    pub specification: CanonicalSpecification,
    /// 目标 ABI。
    pub abi: Option<CanonicalAbi>,
}

impl CanonicalTarget {
    /// 创建新的规范化目标。
    pub const fn new(arch: CanonicalArch, vendor: CanonicalVendor, specification: CanonicalSpecification, abi: Option<CanonicalAbi>) -> Self {
        Self { arch, vendor, specification, abi }
    }

    /// 创建通用 `CLR` 目标。
    pub const fn clr() -> Self {
        Self::new(CanonicalArch::Clr, CanonicalVendor::Microsoft, CanonicalSpecification::Unknown, Some(CanonicalAbi::Clr))
    }

    /// 创建通用 `JVM` 目标。
    pub const fn jvm() -> Self {
        Self::new(CanonicalArch::Jvm, CanonicalVendor::OpenJdk, CanonicalSpecification::Unknown, Some(CanonicalAbi::Jvm))
    }

    /// 创建浏览器 `WASM` 目标。
    pub const fn wasm() -> Self {
        Self::new(CanonicalArch::Wasm32, CanonicalVendor::Unknown, CanonicalSpecification::Browser, Some(CanonicalAbi::WebAssembly))
    }

    /// 创建 `WASI` Preview 2（wasip2）目标。
    pub const fn wasi() -> Self {
        Self::new(CanonicalArch::Wasm32, CanonicalVendor::Unknown, CanonicalSpecification::Wasi, Some(CanonicalAbi::Wasi))
    }

    /// 创建 `WASI` 0.3（wasip3）目标。
    pub const fn wasip3() -> Self {
        Self::new(CanonicalArch::Wasm32, CanonicalVendor::Unknown, CanonicalSpecification::Wasi, Some(CanonicalAbi::WasiP3))
    }

    /// 解析目标别名或四元组。
    pub fn parse(value: &str) -> Result<Self, CanonicalTargetParseError> {
        value.parse()
    }

    /// 将规范化目标投影为较粗粒度的后端目标。
    pub fn to_binary_target(self) -> BinaryTarget {
        let family = match self.arch {
            CanonicalArch::Clr => TargetFamily::Clr,
            CanonicalArch::Jvm => TargetFamily::Jvm,
            CanonicalArch::Wasm32 | CanonicalArch::Wasm64 => TargetFamily::Wasm,
            CanonicalArch::NyarVm => TargetFamily::NyarVm,
            _ => TargetFamily::Native,
        };
        let arch = match self.arch {
            CanonicalArch::X86 => BinaryArch::X86,
            CanonicalArch::X86_64 | CanonicalArch::Native => BinaryArch::X64,
            CanonicalArch::AArch64 => BinaryArch::Arm64,
            _ => BinaryArch::Any,
        };
        let flavor = match family {
            TargetFamily::Clr | TargetFamily::Jvm | TargetFamily::NyarVm => BinaryFlavor::ManagedClr,
            _ => BinaryFlavor::Native,
        };
        BinaryTarget { family, arch, byte_order: ByteOrder::LittleEndian, flavor }
    }

    /// 返回标准化字符串。
    pub fn as_canonical_str(self) -> String {
        self.to_string()
    }

    fn parse_alias(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "clr" => Some(Self::clr()),
            "jvm" => Some(Self::jvm()),
            "wasm" => Some(Self::wasm()),
            "wasi" | "wasip1" | "wasip2" => {
                Some(Self::new(CanonicalArch::Wasm32, CanonicalVendor::Unknown, CanonicalSpecification::Wasi, Some(CanonicalAbi::Wasi)))
            }
            "wasip3" => {
                Some(Self::new(CanonicalArch::Wasm32, CanonicalVendor::Unknown, CanonicalSpecification::Wasi, Some(CanonicalAbi::WasiP3)))
            }
            "node" => {
                Some(Self::new(CanonicalArch::Wasm32, CanonicalVendor::Node, CanonicalSpecification::Unknown, Some(CanonicalAbi::WebAssembly)))
            }
            "deno" => {
                Some(Self::new(CanonicalArch::Wasm32, CanonicalVendor::Deno, CanonicalSpecification::Unknown, Some(CanonicalAbi::WebAssembly)))
            }
            "bun" => {
                Some(Self::new(CanonicalArch::Wasm32, CanonicalVendor::Bun, CanonicalSpecification::Unknown, Some(CanonicalAbi::WebAssembly)))
            }
            "nyar" => {
                Some(Self::new(CanonicalArch::NyarVm, CanonicalVendor::Unknown, CanonicalSpecification::Unknown, Some(CanonicalAbi::Managed)))
            }
            "native" => Some(Self::host_native()),
            _ => None,
        }
    }

    fn host_native() -> Self {
        let arch = match std::env::consts::ARCH {
            "x86_64" => CanonicalArch::X86_64,
            "aarch64" => CanonicalArch::AArch64,
            "x86" => CanonicalArch::X86,
            _ => CanonicalArch::X86_64,
        };
        match std::env::consts::OS {
            "windows" => Self::new(arch, CanonicalVendor::Pc, CanonicalSpecification::Windows, Some(CanonicalAbi::Msvc)),
            "macos" => Self::new(CanonicalArch::AArch64, CanonicalVendor::Apple, CanonicalSpecification::MacOs, Some(CanonicalAbi::SystemV)),
            _ => Self::new(arch, CanonicalVendor::Unknown, CanonicalSpecification::Linux, Some(CanonicalAbi::Gnu)),
        }
    }
}

impl Display for CanonicalTarget {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}-{}", self.arch.as_str(), self.vendor.as_str(), self.specification.as_str())?;
        if let Some(abi) = self.abi {
            let abi_text = match (self.arch, abi) {
                (CanonicalArch::Clr, CanonicalAbi::Clr)
                | (CanonicalArch::Jvm, CanonicalAbi::Jvm)
                | (CanonicalArch::NyarVm, CanonicalAbi::Managed) => "managed",
                _ => abi.as_str(),
            };
            write!(f, "-{}", abi_text)?;
        }
        Ok(())
    }
}

impl FromStr for CanonicalTarget {
    type Err = CanonicalTargetParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(CanonicalTargetParseError::new("target cannot be empty"));
        }
        if let Some(alias) = Self::parse_alias(trimmed) {
            return Ok(alias);
        }

        let segments: Vec<_> = trimmed.split('-').collect();
        if !(3..=4).contains(&segments.len()) {
            return Err(CanonicalTargetParseError::new(format!("invalid target '{}': expected arch-vendor-spec[-abi]", trimmed)));
        }

        let arch = CanonicalArch::parse(segments[0])
            .ok_or_else(|| CanonicalTargetParseError::new(format!("unsupported target arch '{}'", segments[0])))?;
        let vendor = CanonicalVendor::parse(segments[1])
            .ok_or_else(|| CanonicalTargetParseError::new(format!("unsupported target vendor '{}'", segments[1])))?;
        let specification = CanonicalSpecification::parse(segments[2])
            .ok_or_else(|| CanonicalTargetParseError::new(format!("unsupported target specification '{}'", segments[2])))?;
        let abi = if segments.len() == 4 {
            match segments[3].to_ascii_lowercase().as_str() {
                "managed" => Some(match arch {
                    CanonicalArch::Clr => CanonicalAbi::Clr,
                    CanonicalArch::Jvm => CanonicalAbi::Jvm,
                    CanonicalArch::NyarVm => CanonicalAbi::Managed,
                    _ => return Err(CanonicalTargetParseError::new(format!("target '{}' cannot use managed abi", trimmed))),
                }),
                value => Some(
                    CanonicalAbi::parse(value)
                        .ok_or_else(|| CanonicalTargetParseError::new(format!("unsupported target abi '{}'", segments[3])))?,
                ),
            }
        }
        else {
            None
        };

        Ok(Self::new(arch, vendor, specification, abi))
    }
}

impl Serialize for CanonicalTarget {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for CanonicalTarget {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(D::Error::custom)
    }
}
