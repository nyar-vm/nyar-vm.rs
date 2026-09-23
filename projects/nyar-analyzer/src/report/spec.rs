//! 报告数据契约类型。

use serde::{Deserialize, Serialize};

/// 柱图系列项（与 `InteractiveColPlot` 消费字段一致）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColSeriesItem {
    /// 系列键（图例 / 交互状态）。
    pub key: String,
    /// 显示标签。
    pub label: String,
    /// 工具提示数值文本。
    pub value_text: String,
    /// 填充色（CSS 颜色）。
    pub fill: String,
    /// 柱高百分比（0–100）。
    pub height_pct: f64,
}

/// Hydrate 图表岛规格：工具 → asgard。
#[derive(Debug, Clone, PartialEq)]
pub struct HydratedChartSpec {
    /// 组件路由（`chart-status` 等）。
    pub route: String,
    /// 图表标题。
    pub title: String,
    /// 已归一化的系列数据。
    pub series: Vec<ColSeriesItem>,
}

/// 静态 SSG 页面规格。
#[derive(Debug, Clone, PartialEq)]
pub struct StaticPageSpec {
    /// `source/` 下 AWSL 相对路径。
    pub source_relative: String,
    /// 根组件名。
    pub component_name: String,
}
