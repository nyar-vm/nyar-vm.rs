//! Format buffer and options.

/// 格式化 / printer 布局选项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatOptions {
    /// 每级缩进空格数。
    pub indent_width: usize,
    /// pretty-print 目标行宽（字符数）；`0` 表示始终折行。
    pub max_width: usize,
    /// 非空文件是否以换行结尾。
    pub ensure_trailing_newline: bool,
    /// LSP：制表符宽度（`insert_spaces` 为 true 时映射到 `indent_width`）。
    pub tab_size: u32,
    /// LSP：用空格代替制表符。
    pub insert_spaces: bool,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self { indent_width: 4, max_width: 80, ensure_trailing_newline: true, tab_size: 4, insert_spaces: true }
    }
}

impl FormatOptions {
    /// 从 LSP 风格选项构造。
    pub fn from_lsp(tab_size: u32, insert_spaces: bool) -> Self {
        let indent = if insert_spaces { tab_size as usize } else { tab_size as usize };
        Self { indent_width: indent.max(1), tab_size, insert_spaces, ..Self::default() }
    }
}

/// 缩进感知的写出缓冲。
///
/// **已弃用**：正规源码格式化应走 CST → [`super::Document`]；本类型仅供过渡。
#[deprecated(note = "use CST + Document via SourceFormatter")]
#[derive(Debug, Clone)]
pub struct FormatBuffer {
    options: FormatOptions,
    indent_level: usize,
    at_line_start: bool,
    out: String,
}

impl FormatBuffer {
    /// 使用给定选项创建缓冲。
    pub fn new(options: &FormatOptions) -> Self {
        Self { options: options.clone(), indent_level: 0, at_line_start: true, out: String::new() }
    }

    /// 当前列（从 0 起；行首为 0）。
    pub fn column(&self) -> usize {
        if self.at_line_start { 0 } else { self.out.rfind('\n').map(|idx| self.out.len() - idx - 1).unwrap_or(self.out.len()) }
    }

    /// 增加一层缩进。
    pub fn indent(&mut self) {
        self.indent_level = self.indent_level.saturating_add(1);
    }

    /// 减少一层缩进。
    pub fn dedent(&mut self) {
        self.indent_level = self.indent_level.saturating_sub(1);
    }

    /// 写入换行。
    pub fn newline(&mut self) {
        self.out.push('\n');
        self.at_line_start = true;
    }

    /// 写入文本；若在行首则先写缩进。
    pub fn write(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        for (index, line) in text.split('\n').enumerate() {
            if index > 0 {
                self.out.push('\n');
                self.at_line_start = true;
            }
            if line.is_empty() {
                continue;
            }
            if self.at_line_start {
                let spaces = self.indent_level * self.options.indent_width;
                for _ in 0..spaces {
                    self.out.push(' ');
                }
                self.at_line_start = false;
            }
            self.out.push_str(line);
        }
    }

    /// 写入字面量（行首仍可补缩进）。
    pub fn write_raw(&mut self, text: &str) {
        if text.contains('\n') {
            for (index, line) in text.split('\n').enumerate() {
                if index > 0 {
                    self.newline();
                }
                if !line.is_empty() {
                    if self.at_line_start {
                        let spaces = self.indent_level * self.options.indent_width;
                        for _ in 0..spaces {
                            self.out.push(' ');
                        }
                        self.at_line_start = false;
                    }
                    self.out.push_str(line);
                }
            }
        }
        else {
            self.write(text);
        }
    }

    /// 完成并取出字符串。
    pub fn finish(self) -> String {
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_indent_and_newline() {
        let options = FormatOptions { indent_width: 2, max_width: 80, ensure_trailing_newline: false, ..Default::default() };
        let mut buf = FormatBuffer::new(&options);
        buf.write("a");
        buf.newline();
        buf.indent();
        buf.write("b");
        buf.newline();
        buf.dedent();
        buf.write("c");
        assert_eq!(buf.finish(), "a\n  b\nc");
    }
}
