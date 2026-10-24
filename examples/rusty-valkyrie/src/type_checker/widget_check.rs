//! widget 渲染合同检查。

use crate::types::{
    Identifier, SourceSpan,
    hir::{HirFunction, HirModule, HirWidget, ValkyrieType as HirType},
};
use std::{collections::BTreeMap, fmt};

/// widget 渲染合同违规的类别。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WidgetErrorKind {
    /// widget 缺少 render / view 方法。
    MissingRenderMethod {
        /// 涉事 widget 名。
        widget: Identifier,
    },
    /// render 方法返回类型不是 Element。
    InvalidRenderReturnType {
        /// 涉事 widget 名。
        widget: Identifier,
        /// render 实际返回的类型。
        found: HirType,
    },
    /// 在非事件/生命周期上下文中更新 widget 状态。
    InvalidStateUpdate {
        /// 涉事 widget 名。
        widget: Identifier,
        /// 被更新的状态字段名。
        field: Identifier,
    },
}

/// 单条 widget 违规：类别、渲染后的消息与源码位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WidgetError {
    /// 违规类别。
    pub kind: WidgetErrorKind,
    /// 人类可读的错误信息。
    pub message: String,
    /// 产生该违规的源码位置；不可定位时为 None。
    pub span: Option<SourceSpan>,
}

impl WidgetError {
    /// 构造「缺少 render 方法」错误。
    pub fn missing_render_method(widget: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: WidgetErrorKind::MissingRenderMethod { widget: widget.clone() },
            message: format!("widget {} is missing render method", widget),
            span,
        }
    }

    /// 构造「render 返回类型非法」错误。
    pub fn invalid_render_return_type(widget: Identifier, found: HirType, span: Option<SourceSpan>) -> Self {
        Self {
            kind: WidgetErrorKind::InvalidRenderReturnType { widget: widget.clone(), found: found.clone() },
            message: format!("widget {} render must return Element, found {:?}", widget, found),
            span,
        }
    }

    /// 构造「非法状态更新」错误。
    pub fn invalid_state_update(widget: Identifier, field: Identifier, span: Option<SourceSpan>) -> Self {
        Self {
            kind: WidgetErrorKind::InvalidStateUpdate { widget: widget.clone(), field: field.clone() },
            message: format!("widget {} has invalid state update for {}", widget, field),
            span,
        }
    }
}

impl fmt::Display for WidgetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for WidgetError {}

/// 收集模块内 widget 并校验 render 返回类型与状态更新上下文的检查器。
#[derive(Debug, Default)]
pub struct WidgetChecker {
    widgets: BTreeMap<Identifier, HirWidget>,
    errors: Vec<WidgetError>,
    current_widget: Option<Identifier>,
    in_event_handler: bool,
    in_lifecycle_method: bool,
}

impl WidgetChecker {
    /// 创建空的 widget 检查器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 递归收集模块及其子模块中的全部 widget 定义。
    pub fn collect_widgets(&mut self, module: &HirModule) {
        for widget in &module.widgets {
            self.widgets.insert(widget.name.clone(), widget.clone());
        }
        for submodule in &module.submodules {
            self.collect_widgets(submodule);
        }
    }

    /// 检查模块内所有 widget 的渲染合同并返回违规列表。
    pub fn check_module(&mut self, module: &HirModule) -> Vec<WidgetError> {
        self.clear();
        self.collect_widgets(module);
        for widget in module.widgets.iter().chain(module.submodules.iter().flat_map(|module| module.widgets.iter())) {
            self.current_widget = Some(widget.name.clone());
            if let Some(render) = self.find_render_method(widget) {
                if !self.is_element_type(&render.return_type) {
                    self.errors.push(WidgetError::invalid_render_return_type(widget.name.clone(), render.return_type.clone(), None));
                }
            }
            else {
                self.errors.push(WidgetError::missing_render_method(widget.name.clone(), None));
            }
        }
        self.current_widget = None;
        self.errors.clone()
    }

    /// 判断类型是否为 widget 要求的 `Element`。
    pub fn is_element_type(&self, ty: &HirType) -> bool {
        matches!(ty, HirType::Named(name) if name == &Identifier::new("Element"))
    }

    /// 判断方法名是否为事件处理器（`on*` 且非生命周期钩子）。
    pub fn is_event_handler_method(&self, name: &str) -> bool {
        name.starts_with("on") && !matches!(name, "on_mount" | "on_unmount" | "on_update" | "before_update" | "after_update")
    }

    /// 判断方法名是否为生命周期钩子。
    pub fn is_lifecycle_method(&self, name: &str) -> bool {
        matches!(name, "on_mount" | "on_unmount" | "on_update" | "before_update" | "after_update")
    }

    /// 判断字段名是否属于 widget 状态字段（下划线或 `state_` 前缀）。
    pub fn is_state_field(&self, name: &Identifier) -> bool {
        let text = name.as_str();
        text.starts_with('_') || text.starts_with("state_")
    }

    /// 判断当前是否处于允许更新状态的上下文（事件处理器或生命周期方法）。
    pub fn is_valid_state_update_context(&self) -> bool {
        self.in_event_handler || self.in_lifecycle_method
    }

    /// 设置「当前位于事件处理器」标记。
    pub fn set_in_event_handler(&mut self, value: bool) {
        self.in_event_handler = value;
    }

    /// 设置「当前位于生命周期方法」标记。
    pub fn set_in_lifecycle_method(&mut self, value: bool) {
        self.in_lifecycle_method = value;
    }

    /// 返回已收集的全部 widget 名。
    pub fn get_widget_names(&self) -> Vec<Identifier> {
        self.widgets.keys().cloned().collect()
    }

    /// 返回按 widget 名索引的定义表。
    pub fn widgets(&self) -> &BTreeMap<Identifier, HirWidget> {
        &self.widgets
    }

    /// 返回已收集的违规列表。
    pub fn errors(&self) -> &[WidgetError] {
        &self.errors
    }

    /// 返回当前正在检查的 widget 名。
    pub fn current_widget(&self) -> Option<&Identifier> {
        self.current_widget.as_ref()
    }

    /// 清空已收集的 widget、违规与上下文标记。
    pub fn clear(&mut self) {
        self.widgets.clear();
        self.errors.clear();
        self.current_widget = None;
        self.in_event_handler = false;
        self.in_lifecycle_method = false;
    }

    /// 查找 widget 的 render / view 方法定义。
    pub fn find_render_method<'a>(&self, widget: &'a HirWidget) -> Option<&'a HirFunction> {
        widget.methods.iter().find(|method| matches!(method.name.as_str(), "render" | "view"))
    }
}
