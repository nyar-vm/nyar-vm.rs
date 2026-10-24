//! Canonical RenderIR module and node schema.

use std::ops::Range;

use super::{
    expr::{RenderBinding, RenderDiagnostic, RenderEvent, RenderExpr},
    ids::{RenderBindingId, RenderEventId, RenderExprId, RenderNodeId, RenderRegionId},
    surface::TemplateNodeKind,
};

/// Canonical RenderIR module (side tables + rooted node forest).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RenderModule {
    /// 根节点 id 列表，定义模块可见的节点森林入口。
    pub roots: Vec<RenderNodeId>,
    /// 全量节点 side table，按节点 id 索引。
    pub nodes: Vec<RenderNode>,
    /// 区域 side table，每个区域是一组有序子节点 id。
    pub regions: Vec<RenderRegion>,
    /// 表达式记录 side table，按表达式 id 索引。
    pub exprs: Vec<RenderExpr>,
    /// 事件处理器记录 side table。
    pub events: Vec<RenderEvent>,
    /// 脚本/模板绑定记录 side table。
    pub bindings: Vec<RenderBinding>,
    /// 降级过程中收集的诊断。
    pub diagnostics: Vec<RenderDiagnostic>,
}

/// 一组有序子节点 id 构成的区域。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RenderRegion {
    /// 区域内的节点 id 列表（保持文档顺序）。
    pub nodes: Vec<RenderNodeId>,
}

/// canonical RenderIR 的节点种类。
#[derive(Debug, Clone, PartialEq)]
pub enum RenderNode {
    /// intrinsic/宿主元素节点。
    Element(RenderElement),
    /// 文本节点（静态与插值片段）。
    Text(RenderText),
    /// 条件渲染节点。
    If(RenderIfNode),
    /// 列表循环渲染节点。
    Loop(RenderLoopNode),
    /// 无包裹的片段节点。
    Fragment(RenderFragment),
    /// 组件实例节点。
    Component(RenderComponentNode),
}

/// intrinsic/宿主元素节点。
#[derive(Debug, Clone, PartialEq)]
pub struct RenderElement {
    /// 元素标签名。
    pub tag: String,
    /// 模板节点种类（组件 / intrinsic / host view）。
    pub kind: TemplateNodeKind,
    /// 元素属性列表。
    pub attrs: Vec<RenderAttr>,
    /// 子节点区域 id。
    pub children: RenderRegionId,
    /// 源码字节范围。
    pub span: Range<usize>,
}

/// 组件实例节点。
#[derive(Debug, Clone, PartialEq)]
pub struct RenderComponentNode {
    /// 组件标签名。
    pub tag: String,
    /// 组件属性（含 props）列表。
    pub attrs: Vec<RenderAttr>,
    /// 子节点区域 id。
    pub children: RenderRegionId,
    /// 源码字节范围。
    pub span: Range<usize>,
}

/// 无包裹的片段节点，仅承载一个子区域。
#[derive(Debug, Clone, PartialEq)]
pub struct RenderFragment {
    /// 子节点区域 id。
    pub children: RenderRegionId,
    /// 源码字节范围。
    pub span: Range<usize>,
}

/// 文本节点，由静态文本与插值片段组成。
#[derive(Debug, Clone, PartialEq)]
pub struct RenderText {
    /// 文本片段序列（静态与插值交错）。
    pub segments: Vec<RenderTextSegment>,
    /// 源码字节范围。
    pub span: Range<usize>,
}

/// 单个渲染属性。
#[derive(Debug, Clone, PartialEq)]
pub struct RenderAttr {
    /// 属性名。
    pub name: String,
    /// 属性值形态。
    pub value: RenderAttrValue,
    /// 是否为事件绑定属性。
    pub is_event: bool,
    /// 是否为组件 prop。
    pub is_prop: bool,
    /// 事件绑定对应的事件 id（仅事件属性有效）。
    pub event_id: Option<RenderEventId>,
}

/// 属性值的形态。
#[derive(Debug, Clone, PartialEq)]
pub enum RenderAttrValue {
    /// 静态字符串值。
    Static(String),
    /// 单个表达式值。
    Expr(RenderExprId),
    /// 静态与插值混合的模板值。
    Template(Vec<RenderTextSegment>),
}

/// 文本片段。
#[derive(Debug, Clone, PartialEq)]
pub enum RenderTextSegment {
    /// 静态文本。
    Static(String),
    /// 插值表达式。
    Expr(RenderExprId),
}

/// 条件渲染节点。
#[derive(Debug, Clone, PartialEq)]
pub struct RenderIfNode {
    /// 条件表达式 id。
    pub condition: RenderExprId,
    /// 条件成立时渲染的子区域。
    pub then_region: RenderRegionId,
    /// 条件不成立时渲染的子区域。
    pub else_region: RenderRegionId,
    /// 源码字节范围。
    pub span: Range<usize>,
}

/// 列表循环渲染节点。
#[derive(Debug, Clone, PartialEq)]
pub struct RenderLoopNode {
    /// 迭代列表的表达式 id。
    pub items: RenderExprId,
    /// 可选的 key 表达式 id。
    pub key: Option<RenderExprId>,
    /// 当前项绑定 id。
    pub item_binding: RenderBindingId,
    /// 可选的索引绑定 id。
    pub index_binding: Option<RenderBindingId>,
    /// 当前项变量名。
    pub item_var: String,
    /// 索引变量名。
    pub index_var: String,
    /// 循环体的子区域 id。
    pub body_region: RenderRegionId,
    /// 源码字节范围。
    pub span: Range<usize>,
}

/// Canonical RenderIR alias (public contract name).
pub type RenderIr = RenderModule;

impl RenderModule {
    /// 构造空模块（无根节点、全部 side table 为空）。
    pub fn empty() -> Self {
        Self::default()
    }

    /// 按 id 取节点；越界会直接 panic。
    pub fn node(&self, id: RenderNodeId) -> &RenderNode {
        &self.nodes[id.0 as usize]
    }

    /// 按 id 取区域；`RenderRegionId::EMPTY` 返回共享的空区域。
    pub fn region(&self, id: RenderRegionId) -> &RenderRegion {
        if id == RenderRegionId::EMPTY {
            return &EMPTY_REGION;
        }
        &self.regions[id.0 as usize]
    }

    /// 按 id 取表达式记录。
    pub fn expr(&self, id: RenderExprId) -> &RenderExpr {
        &self.exprs[id.0 as usize]
    }

    /// 取表达式记录的源码文本。
    pub fn expr_source(&self, id: RenderExprId) -> &str {
        &self.expr(id).source
    }

    /// 按 id 取绑定记录。
    pub fn binding(&self, id: RenderBindingId) -> &RenderBinding {
        &self.bindings[id.0 as usize]
    }
}

static EMPTY_REGION: RenderRegion = RenderRegion { nodes: Vec::new() };

/// 判断模块是否只有一个根节点且该根为片段节点。
pub fn is_fragment_root(module: &RenderModule) -> bool {
    module.roots.len() == 1 && matches!(module.node(module.roots[0]), RenderNode::Fragment(_))
}

/// 判断标签是否属于 intrinsic 内置标签（转发 surface 层判定）。
pub fn is_intrinsic_tag(tag: &str) -> bool {
    super::surface::is_intrinsic_tag(tag)
}
