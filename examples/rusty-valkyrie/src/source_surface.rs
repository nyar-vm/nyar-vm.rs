//! 可格式化源码面的扩展名与种类（与根 `.editorconfig` glob 一致）。
//!
//! 扩展名与语言种类是语言族事实，由 frontend 拥有；平台格式化注册表只消费这些事实。
//!
//! | 语言面 | 扩展名 | `SourceKind` |
//! |:---|:---|:---|
//! | Valkyrie 核心 | `.v`, `.valkyrie` | `V` |
//! | Valkyrie + X-Grammar | `.vx` | `Vx` |
//! | VON 数据 | `.von` | `Von` |
//! | Asgard AWSL | `.awsl` | `Awsl` |

use std::path::Path;

/// 可格式化的源码种类（语言侧便利枚举）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// 核心 Valkyrie：`.v` / `.valkyrie`（`oak-valkyrie::formatter`）。
    V,
    /// Valkyrie + X-Grammar：`.vx`（`oak-valkyrie::formatter`）。
    Vx,
    /// VON 数据：`.von`（`oak-von::formatter`）。
    Von,
    /// Asgard AWSL 模板：`.awsl`（`oak-awsl::formatter`）。
    Awsl,
}

impl SourceKind {
    /// 对应 `SourceFormatter::language_id`。
    pub fn language_id(self) -> &'static str {
        match self {
            Self::V => "v",
            Self::Vx => "vx",
            Self::Von => "von",
            Self::Awsl => "awsl",
        }
    }

    /// 由文件扩展名推断种类。
    pub fn from_extension(ext: &str) -> Option<Self> {
        source_kind_from_extension(ext)
    }

    /// 由路径推断。
    pub fn from_path(path: &Path) -> Option<Self> {
        path.extension().and_then(|ext| ext.to_str()).and_then(Self::from_extension)
    }
}

/// 核心 Valkyrie 源文件（无 X-Grammar）。
pub const V_EXTENSIONS: &[&str] = &["v", "valkyrie"];

/// Valkyrie + X-Grammar / widget 视图（`.vx`）。
pub const VX_EXTENSIONS: &[&str] = &["vx"];

/// VON 配置/清单。
pub const VON_EXTENSIONS: &[&str] = &["von"];

/// Asgard 模板（与 `.v`/`.vx` 不同语言面）。
pub const AWSL_EXTENSIONS: &[&str] = &["awsl"];

/// 路径扩展名是否属于任一已知的 Valkyrie 族源文件（`.v` / `.valkyrie` / `.vx`）。
pub fn is_valkyrie_family_extension(ext: &str) -> bool {
    V_EXTENSIONS.contains(&ext) || VX_EXTENSIONS.contains(&ext)
}

/// 由扩展名推断 `SourceKind`（legacy `.vk` 不再识别）。
pub fn source_kind_from_extension(ext: &str) -> Option<SourceKind> {
    if V_EXTENSIONS.contains(&ext) {
        Some(SourceKind::V)
    }
    else if VX_EXTENSIONS.contains(&ext) {
        Some(SourceKind::Vx)
    }
    else if VON_EXTENSIONS.contains(&ext) {
        Some(SourceKind::Von)
    }
    else if AWSL_EXTENSIONS.contains(&ext) {
        Some(SourceKind::Awsl)
    }
    else {
        None
    }
}

/// 收集 Valkyrie 族源码时认可的扩展名（不含 `.awsl`）。
pub fn valkyrie_family_extensions() -> impl Iterator<Item = &'static str> {
    V_EXTENSIONS.iter().chain(VX_EXTENSIONS.iter()).copied()
}
