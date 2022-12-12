//! 可格式化源码扩展名（与根 `.editorconfig` glob 一致）。
//!
//! | 语言面 | 扩展名 | `SourceKind` | std-data |
//! |:---|:---|:---|:---|
//! | Valkyrie 核心 | `.v`, `.valkyrie` | `V` | `text::valkyrie` |
//! | Valkyrie + X-Grammar | `.vx` | `Vx` | `text::valkyrie` |
//! | VON 数据 | `.von` | `Von` | `text::von` |
//! | Asgard AWSL | `.awsl` | `Awsl` | `text::awsl` |

use super::SourceKind;

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
