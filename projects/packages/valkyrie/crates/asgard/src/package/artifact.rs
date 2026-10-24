//! Package 产物：文本元数据或二进制交付（无 Kotlin/Swift 等宿主源码）。

/// 单份打包文件。
#[derive(Debug, Clone)]
pub enum PackageArtifact {
    /// UTF-8 元数据（如 AndroidManifest、Info.plist）。
    Text(String),
    /// 二进制交付（DEX、UI 包、Mach-O 占位等）。
    Bytes(Vec<u8>),
}

impl PackageArtifact {
    /// 写入磁盘。
    pub fn write_to(&self, path: &std::path::Path) -> std::io::Result<()> {
        match self {
            Self::Text(text) => std::fs::write(path, text.as_bytes()),
            Self::Bytes(bytes) => std::fs::write(path, bytes),
        }
    }
}
