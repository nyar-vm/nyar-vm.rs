//! 单脚本模式（Valkyrie 版 `cargo-script`）：`.v` 文件首段内嵌 `legion.von` 清单。
//!
//! ```v
//! #!/usr/bin/env legion run
//! # ```legion
//! # {
//! #     name: "demo",
//! #     build: [{ target: "node" }]
//! # }
//! # ```
//!
//! namespace demo;
//! [test]
//! micro smoke() -> unit {}
//! ```

use std::{
    fmt::{Display, Formatter},
    path::{Path, PathBuf},
};

use miette::{Diagnostic, Severity, SourceSpan};

use crate::manifest::{ManifestError, ProjectManifest};

const FENCE_OPEN: &str = "```legion";
const FENCE_CLOSE: &str = "```";

/// 内嵌清单的单脚本上下文。
#[derive(Debug, Clone)]
pub struct SingleScriptContext {
    /// 脚本文件绝对路径。
    pub script_path: PathBuf,
    /// 脚本所在目录，作为工程根。
    pub project_dir: PathBuf,
    /// 从内嵌块解析出的工程清单。
    pub manifest: ProjectManifest,
}

/// 单脚本模式读取与内嵌清单解析的错误。
#[derive(Debug)]
pub enum ScriptError {
    /// 读取脚本文件失败。
    Io {
        /// 出错的脚本路径。
        path: PathBuf,
        /// 底层 I/O 错误。
        source: std::io::Error,
    },
    /// 脚本缺少内嵌 ` ```legion ` 清单块。
    MissingEmbeddedManifest {
        /// 出错的脚本路径。
        path: PathBuf,
    },
    /// 内嵌清单解析失败。
    Manifest {
        /// 出错的脚本路径。
        path: PathBuf,
        /// 底层清单解析错误。
        source: ManifestError,
    },
    /// 内嵌清单块未闭合。
    UnclosedFence {
        /// 出错的脚本路径。
        path: PathBuf,
        /// 未闭合的清单块起始位置。
        span: SourceSpan,
    },
}

impl Display for ScriptError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "读取脚本 `{}` 失败: {source}", path.display()),
            Self::MissingEmbeddedManifest { path } => write!(f, "脚本 `{}` 缺少内嵌 `{FENCE_OPEN}` 清单块", path.display()),
            Self::Manifest { path, source } => write!(f, "脚本 `{}` 内嵌清单解析失败: {source}", path.display()),
            Self::UnclosedFence { path, .. } => write!(f, "脚本 `{}` 内嵌清单块未闭合", path.display()),
        }
    }
}

impl std::error::Error for ScriptError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Manifest { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl Diagnostic for ScriptError {
    fn code<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        Some(Box::new(match self {
            Self::Io { .. } => "legion::script::io",
            Self::MissingEmbeddedManifest { .. } => "legion::script::missing_embedded_manifest",
            Self::Manifest { .. } => "legion::script::manifest",
            Self::UnclosedFence { .. } => "legion::script::unclosed_fence",
        }))
    }

    fn severity(&self) -> Option<Severity> {
        Some(Severity::Error)
    }

    fn help<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        Some(Box::new(match self {
            Self::MissingEmbeddedManifest { .. } => "在 `.v` 文件头部添加 `# ```legion` … `# ```` 注释块，内嵌 `legion.von` 正文",
            Self::UnclosedFence { .. } => "请在内嵌清单末尾补上 `# ```` 闭合行",
            _ => "请确认单脚本头部内嵌清单语法正确",
        }))
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        match self {
            Self::UnclosedFence { span, .. } => {
                Some(Box::new(std::iter::once(miette::LabeledSpan::new_with_span(Some("此处开始".to_string()), *span))))
            }
            _ => None,
        }
    }

    fn diagnostic_source(&self) -> Option<&dyn Diagnostic> {
        match self {
            Self::Manifest { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl ScriptError {
    fn io(path: PathBuf, source: std::io::Error) -> Self {
        Self::Io { path, source }
    }
}

/// 路径是否为 Valkyrie 单脚本 `.v` 文件。
pub fn is_script_path(path: &Path) -> bool {
    path.is_file() && path.extension().is_some_and(|ext| ext == "v")
}

/// 从 `.v` 读取内嵌 `legion.von`；无内嵌块时返回 `None`。
pub fn try_load_single_script(path: &Path) -> Result<Option<SingleScriptContext>, ScriptError> {
    if !is_script_path(path) {
        return Ok(None);
    }

    let content = std::fs::read_to_string(path).map_err(|source| ScriptError::io(path.to_path_buf(), source))?;
    let Some(manifest_source) = extract_embedded_manifest(&content, path)?
    else {
        return Ok(None);
    };

    let manifest = ProjectManifest::parse(&manifest_source).map_err(|source| ScriptError::Manifest { path: path.to_path_buf(), source })?;
    let project_dir = path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));

    Ok(Some(SingleScriptContext { script_path: path.to_path_buf(), project_dir, manifest }))
}

/// 提取内嵌 VON 文本（不含 `#` 前缀）。
pub fn extract_embedded_manifest(content: &str, path: &Path) -> Result<Option<String>, ScriptError> {
    let mut in_fence = false;
    let mut fence_start_line = 0usize;
    let mut lines: Vec<String> = Vec::new();

    for (index, line) in content.lines().enumerate() {
        let trimmed = strip_leading_comment(line).trim();
        if !in_fence {
            if trimmed == FENCE_OPEN {
                in_fence = true;
                fence_start_line = index;
            }
            continue;
        }

        if trimmed == FENCE_CLOSE {
            return Ok(Some(lines.join("\n")));
        }

        lines.push(trimmed.to_string());
    }

    if in_fence {
        return Err(ScriptError::UnclosedFence { path: path.to_path_buf(), span: SourceSpan::new(fence_start_line.into(), 1) });
    }

    Ok(None)
}

fn strip_leading_comment(line: &str) -> &str {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix('#') { rest.strip_prefix(' ').unwrap_or(rest) } else { line }
}
