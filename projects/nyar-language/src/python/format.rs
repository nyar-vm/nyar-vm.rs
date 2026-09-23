//! Python `SourceFormatter` plugin.

use nyar_analyzer::format::{FormatError, FormatOptions, FormattedOutput, SourceFormatter};

/// Language ids (canonical first).
pub fn python_language_ids() -> &'static [&'static str] {
    &["python", "py"]
}

/// Map extension to language id.
pub fn language_id_from_extension(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "py" => Some("python"),
        _ => None,
    }
}

/// Format Python source.
pub fn format_python_source(source: &str, options: &FormatOptions) -> Result<String, FormatError> {
    PythonFormatter.format(source, options)
}

/// First-party Python formatter.
pub struct PythonFormatter;

impl SourceFormatter for PythonFormatter {
    fn language_id(&self) -> &str {
        "python"
    }

    fn format_document(&self, source: &str, _options: &FormatOptions) -> Result<FormattedOutput, FormatError> {
        Ok(FormattedOutput::text_only(normalize_python(source)))
    }
}

/// Strip trailing spaces, collapse 3+ blank lines to 2, ensure final newline.
pub fn normalize_python(source: &str) -> String {
    let newline = if source.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = Vec::new();
    let mut blank_run = 0usize;
    for line in source.split_inclusive('\n') {
        let body = line.trim_end_matches(['\r', '\n']).trim_end();
        if body.is_empty() {
            blank_run += 1;
            if blank_run <= 2 {
                out.push(String::new());
            }
        }
        else {
            blank_run = 0;
            out.push(body.to_string());
        }
    }
    let mut text = out.join(newline);
    if !text.is_empty() && !text.ends_with(newline) {
        text.push_str(newline);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_blanks() {
        assert_eq!(normalize_python("a = 1  \n\n\n\nb = 2\n"), "a = 1\n\n\nb = 2\n");
    }
}
