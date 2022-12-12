//! JavaScript / TypeScript / JSON `SourceFormatter` plugin.

use nyar_analyzer::format::{FormatError, FormatOptions, FormattedOutput, SourceFormatter};

/// Language ids registered for this frontend (canonical first).
pub fn javascript_language_ids() -> &'static [&'static str] {
    &["javascript", "js", "jsx", "mjs", "cjs", "typescript", "ts", "tsx", "mts", "cts", "json"]
}

/// Map a file extension to a formatter language id.
pub fn language_id_from_extension(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "js" | "jsx" | "mjs" | "cjs" => Some("javascript"),
        "ts" | "tsx" | "mts" | "cts" => Some("typescript"),
        "json" => Some("json"),
        _ => None,
    }
}

/// Format JS/TS/JSON source text.
pub fn format_javascript_source(source: &str, language_id: &str, options: &FormatOptions) -> Result<String, FormatError> {
    JavascriptFormatter { language_id: language_id.to_string() }.format(source, options)
}

/// First-party JS/TS/JSON formatter.
pub struct JavascriptFormatter {
    language_id: String,
}

impl JavascriptFormatter {
    /// Create a formatter for `language_id` (`javascript` / `typescript` / `json`).
    pub fn new(language_id: impl Into<String>) -> Self {
        Self { language_id: language_id.into() }
    }
}

impl Default for JavascriptFormatter {
    fn default() -> Self {
        Self::new("javascript")
    }
}

impl SourceFormatter for JavascriptFormatter {
    fn language_id(&self) -> &str {
        &self.language_id
    }

    fn format_document(&self, source: &str, _options: &FormatOptions) -> Result<FormattedOutput, FormatError> {
        let text = if self.language_id.eq_ignore_ascii_case("json") { format_json(source)? } else { normalize_script(source) };
        Ok(FormattedOutput::text_only(text))
    }
}

fn format_json(source: &str) -> Result<String, FormatError> {
    let value: serde_json::Value =
        serde_json::from_str(source).map_err(|error| FormatError::Parse { path: None, message: error.to_string() })?;
    let pretty = serde_json::to_string_pretty(&value).map_err(|error| FormatError::Parse { path: None, message: error.to_string() })?;
    Ok(if pretty.ends_with('\n') { pretty } else { format!("{pretty}\n") })
}

fn normalize_script(source: &str) -> String {
    let newline = if source.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out: Vec<String> = Vec::new();
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
    fn formats_json() {
        let out = format_javascript_source("{ \"a\":1 }", "json", &FormatOptions::default()).unwrap();
        assert!(out.contains("\n"));
        assert!(out.contains("\"a\""));
    }

    #[test]
    fn strips_trailing_space() {
        let out = format_javascript_source("const x = 1;  \n", "javascript", &FormatOptions::default()).unwrap();
        assert_eq!(out, "const x = 1;\n");
    }
}
