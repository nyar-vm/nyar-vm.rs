//! Render expression side-table records.

use super::ids::{RenderBindingId, RenderEventId, RenderExprId};

/// Expression record in canonical RenderIR.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderExpr {
    /// 表达式种类。
    pub kind: RenderExprKind,
    /// 表达式源码文本。
    pub source: String,
    /// 表达式引用的绑定 id 列表。
    pub binding_refs: Vec<RenderBindingId>,
    /// 推断出的值类型。
    pub value_kind: RenderValueKind,
    /// 纯度（是否可安全重算）。
    pub purity: RenderPurity,
    /// 是否可被 memo 缓存。
    pub memoizable: bool,
    /// 事件调用形态（仅事件处理器有效）。
    pub event_call: Option<RenderEventShape>,
}

/// 表达式种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderExprKind {
    /// 模板中的插值表达式。
    Template,
    /// 绑定初始化表达式。
    BindingInit,
    /// 事件处理器表达式。
    EventHandler,
}

/// 表达式的值类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderValueKind {
    /// 无返回值。
    Void,
    /// 布尔值。
    Bool,
    /// 32 位整数。
    I32,
    /// UTF-8 字符串。
    Utf8,
    /// 列表。
    List,
    /// 未知或未推断。
    Unknown,
}

/// 表达式纯度分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderPurity {
    /// 纯表达式，无副作用。
    Pure,
    /// 含副作用。
    Impure,
    /// 无法判定。
    Unknown,
}

/// 事件调用形态：目标事件 id 加可选参数表达式。
#[derive(Debug, Clone, PartialEq)]
pub struct RenderEventShape {
    /// 对应的事件 id。
    pub event_id: RenderEventId,
    /// 可选的事件参数表达式源码。
    pub arg_expr: Option<String>,
}

/// Script / template binding record.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderBinding {
    /// 绑定名。
    pub name: String,
    /// 初始化表达式 id。
    pub init_expr: RenderExprId,
    /// 是否为响应式（signal）绑定。
    pub reactive: bool,
    /// 生成的 signal 变量名。
    pub sig_var: String,
}

/// Event handler record.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderEvent {
    /// 事件名。
    pub name: String,
    /// 处理器表达式 id。
    pub handler_expr: RenderExprId,
}

/// 降级过程中收集的诊断记录。
#[derive(Debug, Clone, PartialEq)]
pub struct RenderDiagnostic {
    /// 诊断文本。
    pub message: String,
    /// 可选源码字节范围。
    pub span: Option<std::ops::Range<usize>>,
}
