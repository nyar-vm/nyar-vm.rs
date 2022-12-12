//! 源码风格格式化契约（CLI fmt / LSP）。

use std::collections::HashMap;

use super::{FormatError, FormatOptions, source_map::FormattedOutput, syntax::ByteRange};

/// 语言无关的源码格式化器（lossless CST 路径）。
///
/// 具体语言在 `nyar-language` 中实现此 trait。
pub trait SourceFormatter: Send + Sync {
    /// 不透明语言标识（由语言插件约定并注册，本层不解释语义）。
    fn language_id(&self) -> &str;

    /// 将源码格式化为规范文本（含 [`FormattedOutput::map`]）。
    fn format_document(&self, source: &str, options: &FormatOptions) -> Result<FormattedOutput, FormatError>;

    /// 将源码格式化为规范文本（便捷：仅返回文本）。
    fn format(&self, source: &str, options: &FormatOptions) -> Result<String, FormatError> {
        Ok(self.format_document(source, options)?.text)
    }

    /// 格式化选中范围（默认：整文件格式化后裁剪；语言可覆盖为 CST 子树格式化）。
    fn format_range(&self, source: &str, range: ByteRange, options: &FormatOptions) -> Result<FormattedOutput, FormatError> {
        let full = self.format_document(source, options)?;
        let start = range.start.min(source.len());
        let end = range.end.min(source.len());
        if start >= end {
            return Ok(FormattedOutput::text_only(String::new()));
        }
        let mapped_start = full.map.orig_to_formatted(start);
        let mapped_end = full.map.orig_to_formatted(end);
        let text = full.text[mapped_start..mapped_end.min(full.text.len())].to_string();
        let mut map = super::source_map::SourceMap::new();
        map.push(range.as_range(), 0..text.len());
        Ok(FormattedOutput { text, map })
    }
}

/// 按语言 id 解析源码格式化器工厂。
pub trait SourceFormatterProvider {
    /// 支持的语言 id 列表（首个为规范 id）。
    fn language_ids(&self) -> &[&str];

    /// 该语言的格式化器实例。
    fn formatter(&self) -> Box<dyn SourceFormatter>;
}

/// 语言 → 源码格式化器注册表。
#[derive(Default)]
pub struct SourceFormatterRegistry {
    by_lang: HashMap<String, Box<dyn SourceFormatter>>,
    aliases: HashMap<String, String>,
}

impl SourceFormatterRegistry {
    /// 空注册表。
    pub fn new() -> Self {
        Self::default()
    }

    /// 从 provider 注册。
    pub fn register_provider(&mut self, provider: &dyn SourceFormatterProvider) {
        let ids = provider.language_ids();
        if ids.is_empty() {
            return;
        }
        let canonical = ids[0].to_ascii_lowercase();
        for id in ids {
            self.aliases.insert(id.to_ascii_lowercase(), canonical.clone());
        }
        self.by_lang.entry(canonical).or_insert_with(|| provider.formatter());
    }

    /// 直接注册。
    pub fn register(&mut self, language_ids: &[&str], formatter: Box<dyn SourceFormatter>) {
        if language_ids.is_empty() {
            return;
        }
        let canonical = language_ids[0].to_ascii_lowercase();
        for id in language_ids {
            self.aliases.insert(id.to_ascii_lowercase(), canonical.clone());
        }
        self.by_lang.entry(canonical).or_insert(formatter);
    }

    fn canonicalize(&self, language_id: &str) -> String {
        let lower = language_id.to_ascii_lowercase();
        self.aliases.get(&lower).cloned().unwrap_or(lower)
    }

    /// 获取格式化器。
    pub fn get(&self, language_id: &str) -> Option<&dyn SourceFormatter> {
        let lang = self.canonicalize(language_id);
        self.by_lang.get(&lang).map(|f| f.as_ref())
    }

    /// 按语言 id 格式化源码（返回文本）。
    pub fn format(&self, language_id: &str, source: &str, options: &FormatOptions) -> Result<String, FormatError> {
        match self.get(language_id) {
            Some(formatter) => {
                let mut formatted = formatter.format(source, options)?;
                if options.ensure_trailing_newline && !formatted.is_empty() && !formatted.ends_with('\n') {
                    formatted.push('\n');
                }
                Ok(formatted)
            }
            None => Err(FormatError::unsupported_language(language_id)),
        }
    }

    /// 按语言 id 格式化源码（含 SourceMap）。
    pub fn format_document(&self, language_id: &str, source: &str, options: &FormatOptions) -> Result<FormattedOutput, FormatError> {
        match self.get(language_id) {
            Some(formatter) => {
                let mut output = formatter.format_document(source, options)?;
                if options.ensure_trailing_newline && !output.text.is_empty() && !output.text.ends_with('\n') {
                    output.text.push('\n');
                    let end = output.text.len();
                    output.map.push(end.saturating_sub(1)..end, end.saturating_sub(1)..end);
                }
                Ok(output)
            }
            None => Err(FormatError::unsupported_language(language_id)),
        }
    }

    /// 格式化选中范围。
    pub fn format_range(
        &self,
        language_id: &str,
        source: &str,
        range: ByteRange,
        options: &FormatOptions,
    ) -> Result<FormattedOutput, FormatError> {
        match self.get(language_id) {
            Some(formatter) => formatter.format_range(source, range, options),
            None => Err(FormatError::unsupported_language(language_id)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EchoFormatter;

    impl SourceFormatter for EchoFormatter {
        fn language_id(&self) -> &str {
            "echo"
        }

        fn format_document(&self, source: &str, _options: &FormatOptions) -> Result<FormattedOutput, FormatError> {
            Ok(FormattedOutput::text_only(source.trim().to_string()))
        }
    }

    #[test]
    fn registry_formats_by_alias() {
        let mut reg = SourceFormatterRegistry::new();
        reg.register(&["echo", "ech"], Box::new(EchoFormatter));
        assert!(reg.format("missing", "x", &FormatOptions::default()).is_err());
        let out = reg.format("ECHO", "  hi  ", &FormatOptions { ensure_trailing_newline: false, ..Default::default() }).unwrap();
        assert_eq!(out, "hi");
    }
}
