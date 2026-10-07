//! 已解析 AST → [`PrettyDocument`] 转换契约（**printer** 路径，非源码 fmt）。
//!
//! 布局代数来自 `oak-pretty-print`，与 `nyar_analyzer::format::Document`（CST formatter）严格分离。
//! 源码正规格式化使用 [`FormatSyntax`]。

use nyar_analyzer::format::FormatError;
use oak_valkyrie::printer::{Document as PrettyDocument, PrintError, to_document as oak_to_document};

use crate::valkyrie::frontend::ValkyrieRoot;

/// 将已解析 **Oak Valkyrie AST** 转为 `oak-pretty-print` 布局文档（尚未 `render`）。
pub trait ToDocument {
    /// 按 Oak printer 合同建成布局文档。
    fn to_document(&self) -> Result<PrettyDocument<'static>, FormatError>;
}

impl ToDocument for ValkyrieRoot {
    fn to_document(&self) -> Result<PrettyDocument<'static>, FormatError> {
        oak_to_document(self).map_err(map_print_error)
    }
}

fn map_print_error(error: PrintError) -> FormatError {
    match error {
        PrintError::Parse(message) => FormatError::Parse { path: None, message },
        PrintError::Unsupported { context } => FormatError::Parse { path: None, message: context },
    }
}
