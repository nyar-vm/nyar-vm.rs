//! AWSL `<script>` 块偏移映射与 **vx** 解析视图

use super::document::script_offset_at;
use super::vx::parse_awsl_script_vx;
use crate::state::DocumentState;
use crate::types::Position;
use nyar_language::ValkyrieCompiler;
use oak_valkyrie::ast::ValkyrieRoot;

/// `<script>` 块解析后的 vx 视图（光标已映射到 script 内坐标）
pub struct AwslScriptView {
    pub script_doc: DocumentState,
    pub ast: ValkyrieRoot,
    /// script 正文在 AWSL 源文件中的字节范围
    pub file_range: std::ops::Range<usize>,
    pub script_position: Position,
}

/// 若 `position` 落在 `<script>` 内，按 **vx** 解析 script 并返回视图
pub fn resolve_script_view(doc: &DocumentState, position: Position) -> Option<AwslScriptView> {
    let root = doc.awsl_root.as_ref()?;
    let file_offset = doc.position_to_offset(position);
    let (file_range, script_offset) = script_offset_at(root, &doc.text, file_offset)?;
    let script_text = root.script.clone()?;

    parse_awsl_script_vx(&script_text).ok()?;
    let mut compiler = ValkyrieCompiler::new(script_text.clone());
    let ast = compiler.parse().ok()?;

    let script_doc = DocumentState::new(doc.uri.clone(), doc.version, script_text);
    let script_position = script_doc.offset_to_position(script_offset);

    Some(AwslScriptView { script_doc, ast, file_range, script_position })
}

/// 将 script 内字节 range 映射回 AWSL 源文件
pub fn map_script_range_to_file(
    file_range: &std::ops::Range<usize>,
    script_range: std::ops::Range<usize>,
) -> std::ops::Range<usize> {
    file_range.start + script_range.start..file_range.start + script_range.end
}
