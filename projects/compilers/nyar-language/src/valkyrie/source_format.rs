//! Valkyrie (`.v` / `.vx`) source formatter（Oak token-gap）。

use crate::formatter::{FormatError, FormatOptions, FormattedOutput};

pub(crate) fn format_valkyrie(source: &str, options: &FormatOptions, _vx: bool) -> Result<FormattedOutput, FormatError> {
    let oak_options = oak_valkyrie::formatter::FormatOptions {
        indent_width: options.indent_width.min(255) as u8,
        line_width: options.max_width,
    };
    let text = oak_valkyrie::formatter::format_source(source, &oak_options)
        .map_err(|error| FormatError::Parse { path: None, message: error.to_string() })?;
    let mut out = text;
    if options.ensure_trailing_newline && !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(FormattedOutput::text_only(out))
}
