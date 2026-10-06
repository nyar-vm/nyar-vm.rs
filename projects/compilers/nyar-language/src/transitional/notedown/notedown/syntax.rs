//! Notedown AST（对齐 C# `Std.Data.Text.Notedown.Syntax` / pandoc IR）。

use std::collections::BTreeMap;

/// 通用属性：`{#id .class key=value}`。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attr {
    /// 元素 id。
    pub id: String,
    /// CSS class 列表。
    pub classes: Vec<String>,
    /// 键值属性。
    pub key_values: Vec<(String, String)>,
}

impl Attr {
    /// 空属性。
    pub fn empty() -> Self {
        Self::default()
    }

    /// 语言标识（code block 的第一个 class）。
    pub fn language(&self) -> &str {
        self.classes.first().map(String::as_str).unwrap_or("")
    }
}

/// 元数据值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetaValue {
    /// 字符串。
    String(String),
    /// 布尔。
    Bool(bool),
}

/// YAML frontmatter 元数据。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Meta {
    /// 键值对。
    pub values: BTreeMap<String, MetaValue>,
}

impl Meta {
    /// 读取字符串元数据。
    pub fn get_string(&self, key: &str) -> Option<&str> {
        match self.values.get(key)? {
            MetaValue::String(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

/// 数学公式类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathType {
    /// 行内 `$...$`。
    Inline,
    /// 块级 `$$...$$`。
    Display,
}

/// 引用样式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteType {
    /// 双引号。
    Double,
    /// 单引号。
    Single,
}

/// 链接目标。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// URL。
    pub url: String,
    /// title 属性。
    pub title: String,
}

/// 列表编号属性。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListAttributes {
    /// 起始编号。
    pub start_number: i32,
}

impl Default for ListAttributes {
    fn default() -> Self {
        Self { start_number: 1 }
    }
}

/// 表格行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRow {
    /// 单元格（每格为行内序列）。
    pub cells: Vec<Vec<NotedownInline>>,
}

/// 表头。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableHead {
    /// 属性。
    pub attr: Attr,
    /// 行。
    pub rows: Vec<TableRow>,
}

/// 表体。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableBody {
    /// 属性。
    pub attr: Attr,
    /// 行头列数。
    pub row_head_columns: u32,
    /// 行。
    pub rows: Vec<TableRow>,
}

/// 表脚。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableFoot {
    /// 属性。
    pub attr: Attr,
    /// 行。
    pub rows: Vec<TableRow>,
}

/// 表格标题。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caption {
    /// 行内内容。
    pub inlines: Vec<NotedownInline>,
}

/// 定义列表项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionItem {
    /// 术语。
    pub term: Vec<NotedownInline>,
    /// 定义块。
    pub definitions: Vec<Vec<NotedownBlock>>,
}

/// 行内节点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotedownInline {
    /// 纯文本。
    Str(String),
    /// 斜体。
    Emph(Vec<NotedownInline>),
    /// 加粗。
    Strong(Vec<NotedownInline>),
    /// 删除线。
    Strikeout(Vec<NotedownInline>),
    /// 上标。
    Superscript(Vec<NotedownInline>),
    /// 下标。
    Subscript(Vec<NotedownInline>),
    /// 小大写。
    SmallCaps(Vec<NotedownInline>),
    /// 引号包裹。
    Quoted {
        /// 引号类型。
        quote_type: QuoteType,
        /// 内容。
        content: Vec<NotedownInline>,
    },
    /// 文献引用 `[@id]`。
    Cite {
        /// 引用 id 列表。
        citations: Vec<String>,
    },
    /// 行内代码。
    Code(String),
    /// 空格。
    Space,
    /// 软换行。
    SoftBreak,
    /// 硬换行。
    LineBreak,
    /// 数学公式。
    Math {
        /// 行内/块级。
        math_type: MathType,
        /// LaTeX 内容。
        content: String,
    },
    /// 原始 HTML 等。
    RawInline {
        /// 格式标识。
        format: String,
        /// 内容。
        content: String,
    },
    /// 链接。
    Link {
        /// 属性。
        attr: Attr,
        /// 链接文字。
        content: Vec<NotedownInline>,
        /// 目标。
        target: Target,
    },
    /// 图片。
    Image {
        /// 属性。
        attr: Attr,
        /// alt 文字。
        alt: Vec<NotedownInline>,
        /// 目标。
        target: Target,
    },
    /// 脚注引用。
    Note(Vec<NotedownBlock>),
    /// 通用 span。
    Span {
        /// 属性。
        attr: Attr,
        /// 内容。
        content: Vec<NotedownInline>,
    },
}

/// 块级节点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotedownBlock {
    /// 段落。
    Para {
        /// 行内内容。
        inlines: Vec<NotedownInline>,
    },
    /// 纯文本块（列表项内）。
    Plain {
        /// 行内内容。
        inlines: Vec<NotedownInline>,
    },
    /// 行块（诗歌等）。
    LineBlock {
        /// 每行一组行内。
        lines: Vec<Vec<NotedownInline>>,
    },
    /// 代码块。
    CodeBlock {
        /// 属性（含 language class）。
        attr: Attr,
        /// 代码文本。
        text: String,
    },
    /// 原始块。
    RawBlock {
        /// 格式。
        format: String,
        /// 内容。
        content: String,
    },
    /// 引用块。
    BlockQuote {
        /// 子块。
        children: Vec<NotedownBlock>,
    },
    /// 有序列表。
    OrderedList {
        /// 列表属性。
        attrs: ListAttributes,
        /// 列表项（每项为块序列）。
        items: Vec<Vec<NotedownBlock>>,
    },
    /// 无序列表。
    BulletList {
        /// 列表项。
        items: Vec<Vec<NotedownBlock>>,
    },
    /// 定义列表。
    DefinitionList {
        /// 项。
        items: Vec<DefinitionItem>,
    },
    /// 标题。
    Header {
        /// 级别 1-6。
        level: u8,
        /// 属性。
        attr: Attr,
        /// 行内内容。
        inlines: Vec<NotedownInline>,
    },
    /// 分割线。
    HorizontalRule,
    /// 表格。
    Table {
        /// 属性。
        attr: Attr,
        /// 标题。
        caption: Option<Caption>,
        /// 表头。
        head: TableHead,
        /// 表体。
        bodies: Vec<TableBody>,
        /// 表脚。
        foot: TableFoot,
    },
    /// Div 容器。
    Div {
        /// 属性。
        attr: Attr,
        /// 子块。
        children: Vec<NotedownBlock>,
    },
    /// 空块占位。
    Null,
}

/// Notedown 根文档。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotedownDocument {
    /// frontmatter 元数据。
    pub meta: Meta,
    /// 块级内容。
    pub blocks: Vec<NotedownBlock>,
}

impl NotedownDocument {
    /// 空文档。
    pub fn empty() -> Self {
        Self { meta: Meta::default(), blocks: Vec::new() }
    }

    /// 提取第一个 ATX `#` 标题纯文本（供 legion doc 标题用）。
    pub fn first_heading_text(&self) -> Option<String> {
        for block in &self.blocks {
            if let NotedownBlock::Header { inlines, .. } = block {
                return Some(inlines_to_plain_text(inlines));
            }
        }
        None
    }
}

/// 将行内序列合并为纯文本。
pub fn inlines_to_plain_text(inlines: &[NotedownInline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            NotedownInline::Str(s) => out.push_str(s),
            NotedownInline::Code(s) => out.push_str(s),
            NotedownInline::Space | NotedownInline::SoftBreak => out.push(' '),
            NotedownInline::LineBreak => out.push('\n'),
            NotedownInline::Emph(c)
            | NotedownInline::Strong(c)
            | NotedownInline::Strikeout(c)
            | NotedownInline::Superscript(c)
            | NotedownInline::Subscript(c)
            | NotedownInline::SmallCaps(c) => out.push_str(&inlines_to_plain_text(c)),
            NotedownInline::Quoted { content, .. } => out.push_str(&inlines_to_plain_text(content)),
            NotedownInline::Link { content, .. } | NotedownInline::Image { alt: content, .. } => {
                out.push_str(&inlines_to_plain_text(content));
            }
            NotedownInline::Span { content, .. } => out.push_str(&inlines_to_plain_text(content)),
            NotedownInline::Math { content, .. } => out.push_str(content),
            NotedownInline::RawInline { content, .. } => out.push_str(content),
            NotedownInline::Cite { .. } | NotedownInline::Note(_) => {}
        }
    }
    out.trim().to_string()
}
