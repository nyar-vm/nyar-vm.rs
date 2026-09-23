//! AWSL `<script>` 按 **vx**（Valkyrie + X-Grammar）解析

use crate::types::{LabeledSpan, SourceID, SourceSpan, ValkyrieError};
use std_data::text::valkyrie::parser::{AstParser, ParseError};

/// 将 AWSL `<script>` 正文作为 `.vx` 源解析（`parse_vx_root`）。
pub fn parse_awsl_script_vx(source: &str) -> Result<(), ValkyrieError> {
    AstParser::parse_vx_root(source).map_err(parse_error_to_valkyrie)
}

fn parse_error_to_valkyrie(err: ParseError) -> ValkyrieError {
    match err {
        ParseError::Io(error) => ValkyrieError::io_error(error.to_string(), None),
        ParseError::Invalid { message, span } => {
            let mut diag = ValkyrieError::parse_error(message);
            if let Some(span) = span {
                diag.labels.push(LabeledSpan {
                    primary: true,
                    span: SourceSpan::new(SourceID::default(), span.start as u32, span.end as u32),
                    key: Some("vx 解析失败".to_string()),
                    data: Vec::new(),
                });
            }
            diag
        }
    }
}
