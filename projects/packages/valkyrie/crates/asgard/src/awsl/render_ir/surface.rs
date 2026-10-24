//! Surface RenderIR: AST-like template tree with source-faithful strings.

use std::ops::Range;

/// Surface render tree root list.
pub type SurfaceIr = Vec<SurfaceNode>;

/// Template node kind (unchanged from legacy surface lowering).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateNodeKind {
    /// 组件节点。
    Component,
    /// intrinsic 内置宿主组件节点。
    Intrinsic,
    /// 宿主视图节点。
    HostView,
}

/// Surface render node.
#[derive(Debug, Clone, PartialEq)]
pub enum SurfaceNode {
    /// 标签元素节点。
    Tag {
        /// 标签名。
        tag: String,
        /// 模板节点种类（组件 / intrinsic / host view）。
        kind: TemplateNodeKind,
        /// 属性列表。
        attrs: Vec<SurfaceAttr>,
        /// 子节点列表。
        children: SurfaceIr,
        /// 源码字节范围。
        span: Range<usize>,
    },
    /// 文本节点。
    Text {
        /// 文本片段。
        parts: Vec<SurfaceTextPart>,
        /// 源码字节范围。
        span: Range<usize>,
    },
    /// 条件分支节点。
    If(SurfaceIf),
    /// 循环节点。
    Loop(SurfaceLoop),
    /// 片段节点。
    Fragment {
        /// 子节点列表。
        children: SurfaceIr,
        /// 源码字节范围。
        span: Range<usize>,
    },
}

/// 表面层渲染属性。
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceAttr {
    /// 属性名。
    pub name: String,
    /// 属性值形态。
    pub value: SurfaceAttrValue,
    /// 是否为事件绑定属性。
    pub is_event: bool,
    /// 是否为组件 prop。
    pub is_prop: bool,
}

/// 表面层属性值的形态。
#[derive(Debug, Clone, PartialEq)]
pub enum SurfaceAttrValue {
    /// 静态字符串值。
    Static(String),
    /// 单个动态表达式的源码文本。
    Dynamic(String),
    /// 静态与动态混合的片段。
    Mixed(Vec<SurfaceTextPart>),
}

/// 表面层文本片段。
#[derive(Debug, Clone, PartialEq)]
pub enum SurfaceTextPart {
    /// 静态文本。
    Static(String),
    /// 动态表达式的源码文本。
    Dynamic(String),
}

/// 表面层条件分支节点。
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceIf {
    /// 条件表达式源码。
    pub condition: String,
    /// 条件成立时的分支节点列表。
    pub then_branch: SurfaceIr,
    /// 条件不成立时的分支节点列表。
    pub else_branch: SurfaceIr,
    /// 源码字节范围。
    pub span: Range<usize>,
}

/// 表面层循环节点。
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceLoop {
    /// 迭代表达式源码。
    pub items_expr: String,
    /// 当前项变量名。
    pub item_var: String,
    /// 索引变量名。
    pub index_var: String,
    /// 可选的 key 表达式源码。
    pub key_expr: Option<String>,
    /// 循环体节点列表。
    pub body: SurfaceIr,
    /// 源码字节范围。
    pub span: Range<usize>,
}

/// 判断根列表是否为单个片段节点。
pub fn is_fragment_root(ir: &SurfaceIr) -> bool {
    matches!(ir.as_slice(), [SurfaceNode::Fragment { .. }])
}

/// 判断标签是否属于 intrinsic 内置标签集合。
pub fn is_intrinsic_tag(tag: &str) -> bool {
    matches!(
        tag,
        "Column"
            | "Box"
            | "Row"
            | "Text"
            | "Button"
            | "Flex"
            | "Slot"
            | "List"
            | "Item"
            | "Checkbox"
            | "Radio"
            | "RadioGroup"
            | "column"
            | "box"
            | "row"
            | "text"
            | "button"
            | "flex"
            | "slot"
            | "list"
            | "item"
            | "checkbox"
            | "radio"
            | "radiogroup"
    )
}
