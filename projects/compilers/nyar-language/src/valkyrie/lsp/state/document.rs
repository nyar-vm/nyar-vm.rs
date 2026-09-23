use crate::types::Position;
use oak_valkyrie::ast::ValkyrieRoot;
use crate::types::{hir::HirModule, SourceID, ValkyrieError};
use std_data::text::awsl::AwslRoot;

/// 文档编译结果
#[derive(Debug, Clone)]
pub struct DocumentState {
    pub uri: String,
    pub version: i32,
    pub text: String,
    pub hash: u64,
    pub file_id: Option<SourceID>,
    pub ast: Option<ValkyrieRoot>,
    pub awsl_root: Option<AwslRoot>,
    /// AWSL 文件内 `<script>` 正文起始字节偏移（用于 span 映射）
    pub awsl_script_base: Option<usize>,
    /// synthetic `widget { … }` 源码中 script 正文的起始偏移
    pub awsl_synthetic_prefix: Option<usize>,
    pub hir: Option<HirModule>,
    pub diagnostics: Vec<ValkyrieError>,
    pub line_offsets: Vec<usize>,
    /// Extracted component ABI from `<script>`.
    pub component_abi: Option<std_data::text::awsl::ComponentAbi>,
    /// ABI extraction / cross-file validation issues.
    pub abi_issues: Vec<std_data::text::awsl::AbiIssue>,
}

impl DocumentState {
    pub fn new(uri: String, version: i32, text: String) -> Self {
        let line_offsets = Self::compute_line_offsets(&text);
        let hash = Self::compute_hash(&text);
        Self { uri, version, text, hash, file_id: None, ast: None, awsl_root: None, awsl_script_base: None, awsl_synthetic_prefix: None, hir: None, diagnostics: Vec::new(), line_offsets, component_abi: None, abi_issues: Vec::new() }
    }

    pub fn compute_hash(text: &str) -> u64 {
        use std::{
            collections::hash_map::DefaultHasher,
            hash::{Hash, Hasher},
        };
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        hasher.finish()
    }

    pub fn compute_line_offsets(text: &str) -> Vec<usize> {
        let mut offsets = vec![0];
        for (i, c) in text.char_indices() {
            if c == '\n' {
                offsets.push(i + 1);
            }
        }
        offsets
    }

    pub fn offset_to_position(&self, offset: usize) -> Position {
        let line = match self.line_offsets.binary_search(&offset) {
            Ok(line) => line,
            Err(line) => line - 1,
        };
        let line_start = self.line_offsets[line];
        let character_offset = offset - line_start;
        let line_text = &self.text[line_start..line_start + character_offset];
        let character = line_text.chars().count();
        Position { line: line as u32, character: character as u32 }
    }

    pub fn position_to_offset(&self, position: Position) -> usize {
        let line = position.line as usize;
        if line >= self.line_offsets.len() {
            return self.text.len();
        }
        let line_start = self.line_offsets[line];
        let line_text = &self.text[line_start..];

        let mut char_count = 0;
        for (i, _) in line_text.char_indices() {
            if char_count == position.character as usize {
                return line_start + i;
            }
            char_count += 1;
        }

        // If character position is beyond line length, return end of line
        line_start + line_text.len()
    }
}
